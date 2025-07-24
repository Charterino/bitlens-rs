use crate::{
    db::{self, rocksdb::SerializedTx},
    metrics::{METRIC_FULL_BLOCKS_DOWNLOADED, METRIC_TOP_HEADER_HEIGHT},
    packets::{
        blockheader::BlockHeaderBorrowed,
        getheaders::GetHeadersOwned, packetpayload::PayloadToSend,
    },
    some_or_break,
    types::{
        blockheaderwithnumber::BlockHeaderWithNumber,
        blockmetrics::BlockMetrics,
        frontpagedata::{FrontPageData, FrontPageDataWithSerialized, ShortBlock, ShortTx},
        stats::Stats,
    },
};
use anyhow::{Result, bail};
use blocksync::sync_blocks;
use chain::Chain;
use headersync::sync_headers;
use slog_scope::{debug, info};
use std::{
    cmp::min,
    sync::{
        LazyLock, RwLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

const FRONTPAGE_TXS_COUNT: usize = 50;
const FRONTPAGE_BLOCKS_COUNT: usize = 50;

static SYNCING_HEADERS: AtomicBool = AtomicBool::new(false); // are we currently in the process of initial header sync?
#[allow(dead_code)]
static SYNCING_BODIES: AtomicBool = AtomicBool::new(false); // are we currently in the process of downloading blocks?
static DOWNLOADED_BLOCKS: AtomicU64 = AtomicU64::new(0); // It's important that this number only counts downloaded blocks from the main chain.

static CHAIN: LazyLock<RwLock<Chain>> = LazyLock::new(|| RwLock::new(Chain::default()));

pub static FRONTPAGE_STATS: LazyLock<RwLock<FrontPageDataWithSerialized>> =
    LazyLock::new(|| RwLock::new(Default::default())); // empty initially

mod blocksync;
mod chain;
mod headersync;

pub async fn start() {
    load_headers_from_sqlite().await;
    tokio::spawn(async {
        if need_ihd() {
            info!("last known header is >24 hours old, synching headers..");
            sync_headers().await;
            let r = CHAIN.read().unwrap();
            info!("synced headers"; "top number" => r.top_header.number);
        }
        // if we're not syncing from 0, we already have some blocks downloaded
        // which means we can update the frontpage response
        let top_downloaded_block_hash = {
            let r = CHAIN.read().unwrap();
            get_top_downloaded_block_hash(&r)
        };
        if let Some(top_block_hash) = top_downloaded_block_hash {
            update_frontpage_response(top_block_hash, None, None, true).await;
        }
        if need_ibd() {
            info!("downloading missing block bodies..");
            sync_blocks().await;
        }
    });
}

async fn update_frontpage_response(
    top_block_hash: [u8; 32],
    metrics_cache: Option<&[BlockMetrics]>,
    analyzed_cache: Option<&[&[SerializedTx<'_>]]>,
    first_time: bool,
) {
    // If this is the first time this function is called, we'll have to go pulling block data from the store
    // If not, then we have everything we need

    let stats = calculate_stats(top_block_hash, metrics_cache).await;
    let mut latest_blocks = Vec::with_capacity(FRONTPAGE_BLOCKS_COUNT);
    let mut latest_txs = Vec::with_capacity(FRONTPAGE_TXS_COUNT);
    let mut w = if first_time {
        // Pull from the disk

        let mut current_header = {
            let r = CHAIN.read().unwrap();
            r.known_headers[&top_block_hash].clone()
        };

        loop {
            if latest_blocks.len() == FRONTPAGE_BLOCKS_COUNT
                && latest_txs.len() == FRONTPAGE_TXS_COUNT
            {
                break;
            }

            // pull the tx hashes that are in this block from rocksdb
            let block_tx_hashes = db::rocksdb::get_block_tx_hashes(current_header.header.hash)
                .await
                .expect("to fetch block txs from rocksdb");

            latest_blocks.push(ShortBlock {
                number: current_header.number,
                hash: current_header.header.human_hash(),
                tx_count: block_tx_hashes.len() as u64,
                reward_btc: 0., // todo
                btc_price: 0.,  // todo
                timestamp: current_header.header.timestamp,
            });

            let missing_txs = min(
                FRONTPAGE_TXS_COUNT - latest_txs.len(),
                block_tx_hashes.len(),
            );
            for j in 0..missing_txs {
                let tx_hash = block_tx_hashes[j];
                let tx = db::rocksdb::get_analyzed_tx(tx_hash)
                    .await
                    .expect("to get analyzed tx from rocksdb");
                let mut human_hash = tx_hash;
                human_hash.reverse();
                let human_hash = hex::encode(human_hash);
                latest_txs.push(ShortTx {
                    hash: human_hash,
                    value: tx.txouts_sum as f64 / 100_000_000.,
                    size_wus: tx.size_wus,
                    block_number: current_header.number,
                    block_hash: current_header.header.human_hash(),
                    fee_sats: tx.fee,
                    btc_price: 0., // TODO
                    timestamp: current_header.header.timestamp,
                });
            }

            if current_header.header.parent == [0u8; 32] {
                break; // reached the genesis block
            }
            current_header = {
                let r = CHAIN.read().unwrap();
                r.known_headers[&current_header.header.parent].clone()
            };
        }

        let w = FRONTPAGE_STATS.write().unwrap();
        w
    } else {
        // Everything is in ram
        let r = CHAIN.read().unwrap();
        let analyzed = analyzed_cache.expect("analyzed_cache to be Some if first_time is true");
        let mut i = analyzed.len();
        let mut current_header = &r.known_headers[&top_block_hash];
        while i > 0 {
            if latest_blocks.len() == FRONTPAGE_BLOCKS_COUNT
                && latest_txs.len() == FRONTPAGE_TXS_COUNT
            {
                break;
            }

            let block_txs = &analyzed[i - 1];

            latest_blocks.push(ShortBlock {
                number: current_header.number,
                hash: current_header.header.human_hash(),
                tx_count: block_txs.len() as u64,
                reward_btc: 0., // TODO
                btc_price: 0.,  // TODO
                timestamp: current_header.header.timestamp,
            });

            let missing_txs = min(FRONTPAGE_TXS_COUNT - latest_txs.len(), block_txs.len());
            for j in 0..missing_txs {
                let tx = block_txs[j];
                let mut human_hash = tx.hash;
                human_hash.reverse();
                let human_hash = hex::encode(human_hash);
                latest_txs.push(ShortTx {
                    hash: human_hash,
                    value: tx.txouts_sum as f64 / 100_000_000.,
                    size_wus: tx.size_wus,
                    block_number: current_header.number,
                    block_hash: current_header.header.human_hash(),
                    fee_sats: tx.fee,
                    btc_price: 0., // TODO
                    timestamp: current_header.header.timestamp,
                });
            }

            i -= 1;
            if i != 0 {
                current_header = &r.known_headers[&current_header.header.parent];
            }
        }

        // We've either got all of the required blocks and txs, or ran out of blocks in `analyzed`.
        // If we're still missing blocks/txs, copy them from the previous response
        let missing_blocks = FRONTPAGE_BLOCKS_COUNT - latest_blocks.len();
        let missing_txs = FRONTPAGE_TXS_COUNT - latest_txs.len();
        let w = FRONTPAGE_STATS.write().unwrap();
        if missing_blocks != 0 {
            let to_keep_from_old = min(missing_blocks, w.data.latest_blocks.len());
            latest_blocks.extend_from_slice(&w.data.latest_blocks[0..to_keep_from_old]);
        }
        if missing_txs != 0 {
            let to_keep_from_old = min(missing_txs, w.data.latest_txs.len());
            latest_txs.extend_from_slice(&w.data.latest_txs[0..to_keep_from_old]);
        }

        w
    };

    w.data = FrontPageData {
        stats,
        latest_blocks,
        latest_txs,
    };
    w.serialized = serde_json::to_string(&w.data).unwrap();
    debug!("updated front page response"; "new_response" => w.serialized.clone());
}

async fn calculate_stats(top: [u8; 32], cache: Option<&[BlockMetrics]>) -> Stats {
    let (relevant_blocks, timestamps, now) = {
        let r = CHAIN.read().unwrap();
        let top_header = &r.known_headers[&top];
        let now = top_header.header.timestamp;
        let cutoff = now - 86400 * 15;
        let mut relevant_blocks = Vec::with_capacity(2160);
        let mut timestamps = Vec::with_capacity(2160);
        let mut last = top_header;
        loop {
            relevant_blocks.push(last.header.hash);
            timestamps.push(last.header.timestamp);
            if last.header.parent == [0u8; 32] {
                break;
            }
            last = &r.known_headers[&last.header.parent];
            if last.header.timestamp < cutoff {
                break;
            }
        }
        (relevant_blocks, timestamps, now)
    };

    // Now that we know what blocks we need, first get all the blocks we can from the cache,
    // then fetch the missing blocks from sqlite
    let mut bms = Vec::with_capacity(relevant_blocks.len());
    if let Some(cache) = cache {
        for i in bms.len()..relevant_blocks.len() {
            if let Some(bm) = cache.get(i) {
                bms.push(bm.clone());
            } else {
                // This block is not in the cache. Since the cache is always continuous,
                // e.g. if it does not have A, it definitely cant have A's parent,
                // we can break here and fetch the remaining blocks from sqlite
                break;
            }
        }
    }
    let mut handles = Vec::new();
    for i in bms.len()..relevant_blocks.len() {
        let hash = &relevant_blocks[i];
        handles.push(tokio::spawn(db::sqlite::find_block_metrics(*hash)));
    }

    let mut results = Vec::with_capacity(handles.len());
    for handle in handles {
        results.push(handle.await.unwrap());
    }

    bms.append(&mut results);

    // Got all the block metrics we'll need
    // group blocks by day
    let mut boundary = 0;
    let mut cutoff = now - (now % 86400);
    let mut result = Stats::default();

    for i in 0..bms.len() {
        if timestamps[i] < cutoff {
            // blocks[boundary:i] belong to day `cutoff`

            let mut total_vol = 0.;
            let mut total_txs = 0;
            let mut median_fees_sum = 0.;
            for bm in &bms[boundary..i] {
                total_vol += bm.volume as f64 / 100_000_000.;
                total_txs += bm.txs_count;
                median_fees_sum += bm.median_fee_rate;
            }

            result
                .average_median_fees
                .insert(cutoff, median_fees_sum / (i - boundary) as f64);
            result.volume.insert(cutoff, total_vol);
            result.transactions.insert(cutoff, total_txs);

            cutoff -= 86400;
            boundary = i;
        }
    }

    result
}

async fn load_headers_from_sqlite() {
    let headers = db::sqlite::get_all_headers().await;
    let mut w = CHAIN.write().unwrap();
    for new_header in headers {
        if new_header.total_work > w.top_header.total_work {
            w.top_header = new_header.clone();
            METRIC_TOP_HEADER_HEIGHT.set(new_header.number as i64);
        }

        w.known_headers
            .insert(new_header.header.hash, new_header.clone());
    }

    // After loading all headers from sqlite, figure out how many blocks we have downloaded
    let new_downloaded_blocks = calculate_downloaded_blocks(&w);
    DOWNLOADED_BLOCKS.store(new_downloaded_blocks, Ordering::Relaxed);
    METRIC_FULL_BLOCKS_DOWNLOADED.set(new_downloaded_blocks as i64);
}

// Do we need to sync headers?
fn need_ihd() -> bool {
    let r = CHAIN.read().unwrap();
    let top_header_time = r.top_header.header.timestamp as u64;
    let current_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    current_time - top_header_time > 24 * 3600
}

// Do we need to sync block bodies?
fn need_ibd() -> bool {
    let r = CHAIN.read().unwrap();
    r.top_header.number - DOWNLOADED_BLOCKS.load(Ordering::Relaxed) > 0
}

// Constructs a `getheaders` packet with `BlockLocator` that allows the remote peer to detect that we are on the wrong branch
fn build_get_headers() -> PayloadToSend {
    let r = CHAIN.read().unwrap();
    if r.top_header.number == 0 {
        // Starting from zero!
        return PayloadToSend::GetHeaders(GetHeadersOwned {
            version: 70016,
            block_locator: vec![r.top_header.header.hash],
            hash_stop: [0u8; 32],
        });
    }

    let mut block_locator = Vec::with_capacity(10);
    let mut step = 1;

    let mut current = &r.top_header;
    block_locator.push(current.header.hash);

    while current.number > 0 {
        if block_locator.len() >= 10 {
            step *= 2;
        }
        if step > current.number {
            step = current.number;
        }
        match r.get_ancestor(current, current.number - step) {
            Some(new) => {
                current = new;
                block_locator.push(current.header.hash);
            }
            None => {
                // Should never happen?!
                panic!(
                    "failed to get ancestor {} for {}",
                    step,
                    current.header.human_hash()
                );
            }
        }
    }

    PayloadToSend::GetHeaders(GetHeadersOwned {
        version: 70016,
        block_locator,
        hash_stop: [0u8; 32],
    })
}

fn validate_and_apply_header_inner(header: &BlockHeaderBorrowed, w: &mut Chain) -> Result<()> {
    let duplicate = w.known_headers.contains_key(&header.hash);
    let parent = match w.known_headers.get(header.parent.as_slice()) {
        Some(parent) => parent,
        None => bail!("dont have the parent"),
    };

    if duplicate {
        return Ok(());
    }

    // make sure the difficulty transition is valid
    let expected_bits = w.get_next_bits_required(parent);
    if expected_bits != header.bits {
        bail!(format!(
            "invalid difficulty transition for {}, expected {} {} but got {} {}",
            header.human_hash(),
            hex::encode(expected_bits.to_be_bytes()),
            expected_bits,
            hex::encode(header.bits.to_be_bytes()),
            header.bits
        ));
    }

    let new_total_work = parent.total_work + header.get_work();
    let new_header = BlockHeaderWithNumber {
        header: (*header).into(),
        number: parent.number + 1,
        fetched_full: false,
        total_work: new_total_work,
    };

    if new_header.total_work > w.top_header.total_work {
        // since top_header is changing, we might need to adjust DOWNLOADED_BLOCKS
        let should_recount = new_header.header.parent != w.top_header.header.hash;
        w.top_header = new_header.clone();
        METRIC_TOP_HEADER_HEIGHT.set(new_header.number as i64);
        if should_recount {
            let new_count = calculate_downloaded_blocks(w);
            DOWNLOADED_BLOCKS.store(new_count, Ordering::Relaxed);
            METRIC_FULL_BLOCKS_DOWNLOADED.set(new_count as i64);
        }
    }

    w.known_headers
        .insert(new_header.header.hash, new_header.clone());
    tokio::spawn(async move {
        db::sqlite::insert_header(&new_header).await;
    });

    Ok(())
}

fn validate_and_apply_headers(headers: &[BlockHeaderBorrowed]) -> Result<()> {
    let mut w = CHAIN.write().unwrap();
    for header in headers {
        validate_and_apply_header_inner(header, &mut w)?;
    }

    Ok(())
}

fn calculate_downloaded_blocks(r: &Chain) -> u64 {
    // Since blocks are applied sequentially, if we see a block that has FetchedFull set to true, we know all of the blocks before it are also downloaded
    let mut last = &r.top_header;
    loop {
        if last.fetched_full {
            return last.number + 1;
        }
        if last.number == 0 {
            return 0;
        }
        last = r.known_headers.get(last.header.parent.as_slice()).unwrap();
    }
}

fn get_top_downloaded_block_hash(r: &Chain) -> Option<[u8; 32]> {
    // Since blocks are applied sequentially, if we see a block that has FetchedFull set to true, we know all of the blocks before it are also downloaded
    let mut last = &r.top_header;
    loop {
        if last.fetched_full {
            return Some(last.header.hash);
        }
        if last.number == 0 {
            return None;
        }
        last = r.known_headers.get(last.header.parent.as_slice()).unwrap();
    }
}

// Option<(all block hashes start from lowest to highest, number of the first block hash)>
fn get_block_hashes_to_download() -> Option<(Vec<[u8; 32]>, u64)> {
    let r = CHAIN.read().unwrap();
    let downloaded = DOWNLOADED_BLOCKS.load(Ordering::Relaxed);
    if downloaded > r.top_header.number {
        return None;
    }
    let mut all = Vec::with_capacity((r.top_header.number - downloaded) as usize);

    let mut c = &r.top_header;
    let mut last_added: u64;
    loop {
        all.push(c.header.hash);
        last_added = c.number;
        if c.number == 0 {
            break;
        }
        c = some_or_break!(r.known_headers.get(c.header.parent.as_slice()));

        if c.fetched_full {
            break;
        }
    }

    all.reverse();
    Some((all, last_added))
}

fn mark_as_downloaded(blocks: Vec<[u8; 32]>) {
    let mut w = CHAIN.write().unwrap();
    for block in blocks {
        w.known_headers
            .get_mut(&block)
            .expect("the header to be present in known_headers")
            .fetched_full = true;
    }
    // Technically the top header is stored twice: in known_headers and as w.top_header.
    // Which means fetched_full might be different in these two structs. This is the only place where we change data inside of a header,
    // since they're otherwise immutable.
    // Change top_header's fetched_full here and we're guaranteed to be in sync.
    w.top_header.fetched_full = w
        .known_headers
        .get(&w.top_header.header.hash)
        .unwrap()
        .fetched_full;

    // Update DOWNLOADED_BLOCKS
    let new_downloaded_blocks = calculate_downloaded_blocks(&w);
    DOWNLOADED_BLOCKS.store(new_downloaded_blocks, Ordering::Relaxed);
    METRIC_FULL_BLOCKS_DOWNLOADED.set(new_downloaded_blocks as i64);
}
