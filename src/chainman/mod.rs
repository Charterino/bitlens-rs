use crate::{
    chainman::{
        blocksync::{start_syncing_blocks, stop_syncing_blocks},
        headersync::{start_syncing_headers, stop_syncing_headers},
    },
    db::{self, rocksdb::SerializedTx},
    metrics::{METRIC_FULL_BLOCKS_DOWNLOADED, METRIC_TOP_HEADER_HEIGHT},
    miners,
    packets::{
        blockheader::{BlockHeaderBorrowed, BlockHeaderOwned, BlockHeaderRef},
        getheaders::GetHeadersOwned,
        packetpayload::PayloadToSend,
    },
    some_or_break,
    types::{
        addresstransaction::AddressTransaction,
        blockheaderwithnumber::BlockHeaderWithNumber,
        blockmetrics::BlockMetrics,
        frontpagedata::{
            FrontPageData, FrontPageDataUpdate, FrontPageDataWithSerialized, ShortBlock, ShortTx,
            SyncStats,
        },
        stats::Stats,
    },
    util::genesis::GENESIS_HEADER,
};
use anyhow::{Result, anyhow, bail};
use blocksync::sync_blocks;
use chain::Chain;
use headersync::sync_headers;
use slog_scope::info;
use std::{
    cmp::min,
    collections::{HashMap, HashSet},
    sync::{
        LazyLock, RwLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

const FRONTPAGE_TXS_COUNT: usize = 50;
const FRONTPAGE_BLOCKS_COUNT: usize = 50;

pub static SYNCING_HEADERS: AtomicBool = AtomicBool::new(false); // are we currently in the process of initial header sync?
pub static SYNCING_BODIES: AtomicBool = AtomicBool::new(false); // are we currently in the process of downloading blocks?
static DOWNLOADED_BLOCKS: AtomicU64 = AtomicU64::new(0); // It's important that this number only counts downloaded blocks from the main chain.

static CHAIN: LazyLock<RwLock<Chain>> = LazyLock::new(|| RwLock::new(Chain::default()));

pub static FRONTPAGE_STATS: LazyLock<RwLock<FrontPageDataWithSerialized>> =
    LazyLock::new(|| RwLock::new(Default::default())); // empty initially
pub static FRONTPAGE_UPDATE_BROADCAST: LazyLock<tokio::sync::broadcast::Sender<String>> =
    LazyLock::new(|| {
        let (tx, _) = tokio::sync::broadcast::channel(128);
        tx
    });

mod blocksync;
mod chain;
mod headersync;
pub mod keepup;
mod syncspeedtracker;

pub async fn start() {
    load_headers_from_sqlite();
    miners::callback_after_headers_loaded();
    keepup::start();
    tokio::spawn(async {
        // get blocks to download without unlocking CHAIN
        let (top_downloaded_block_hash, block_hashes_to_download) = {
            let w = if need_ihd() {
                info!("last known header is >24 hours old, synching headers..");
                start_syncing_headers();
                sync_headers().await;
                let mut w = CHAIN.write().unwrap();
                info!("synced headers"; "top number" => w.top_header.number);
                let new_count = calculate_downloaded_blocks(&w);
                DOWNLOADED_BLOCKS.store(new_count, Ordering::Relaxed);
                METRIC_FULL_BLOCKS_DOWNLOADED.set(new_count as i64);
                generate_top_work_chain_hashset(&mut w);
                w
            } else {
                CHAIN.write().unwrap()
            };
            // if we're not syncing from 0, we already have some blocks downloaded
            // which means we can update the frontpage response
            let top_downloaded_block_hash = { get_top_downloaded_block_hash(&w) };
            // get blocks to download for the blocksync without releasing the lock
            let block_hashes_to_download = get_block_hashes_to_download(&w);
            (top_downloaded_block_hash, block_hashes_to_download)
        };
        if let Some(top_block_hash) = top_downloaded_block_hash {
            generate_frontpage_data(top_block_hash).await;
        }
        if let Some((missing_blocks, first_missing_block_number)) = block_hashes_to_download {
            start_syncing_blocks();
            stop_syncing_headers();
            info!("downloading missing block bodies..");
            sync_blocks(missing_blocks, first_missing_block_number).await;
            info!("downloaded block bodies");
            stop_syncing_blocks();
        } else {
            stop_syncing_headers();
        }
    });
}

pub fn get_top_header_hash() -> [u8; 32] {
    let c = CHAIN.read().unwrap();
    c.top_header.header.hash
}

pub fn get_top_header_number() -> u64 {
    let c = CHAIN.read().unwrap();
    c.top_header.number
}

pub fn get_header_by_hash(hash: [u8; 32]) -> Option<BlockHeaderWithNumber> {
    let r = CHAIN.read().unwrap();
    r.known_headers.get(&hash).cloned()
}

pub fn get_hash_by_number(number: u64) -> Option<[u8; 32]> {
    let r = CHAIN.read().unwrap();
    r.number_to_hash.get(&number).cloned()
}

pub fn get_top_synced_header_hash() -> Option<[u8; 32]> {
    let c = CHAIN.read().unwrap();
    let mut last_hash = c.top_header.header.hash;
    loop {
        let header = c.known_headers.get(&last_hash).unwrap();
        if header.fetched_full {
            return Some(last_hash);
        }
        if header.header.hash == GENESIS_HEADER.hash {
            return None;
        }
        last_hash = header.header.parent
    }
}

pub fn get_blocks_with_timestamps_and_coinbase_asciis(
    mut last: [u8; 32],
) -> Vec<([u8; 32], u64, u64, String)> {
    let r = CHAIN.read().unwrap();
    let mut result = vec![];
    loop {
        let header = &r.known_headers[&last];
        if let Some(v) = &header.coinbase_ascii {
            result.push((
                last,
                header.number,
                header.header.timestamp as u64,
                v.clone(),
            ));
        }
        if header.header.hash == GENESIS_HEADER.hash {
            break;
        }
        last = header.header.parent;
    }

    result.reverse();

    result
}

pub fn get_block_timestamps_and_numbers(hashes: &[[u8; 32]]) -> (Vec<u64>, Vec<u64>) {
    let r = CHAIN.read().unwrap();
    let timestamps = hashes
        .iter()
        .map(|v| r.known_headers[v].header.timestamp as u64)
        .collect();
    let numbers = hashes.iter().map(|v| r.known_headers[v].number).collect();
    (timestamps, numbers)
}

pub fn try_get_block_timestamps_and_numbers_and_cb_asciis(
    hashes: &[[u8; 32]],
) -> Result<Vec<(u64, u64, String)>> {
    let r = CHAIN.read().unwrap();
    hashes
        .iter()
        .map(|hash| match r.known_headers.get(hash) {
            Some(v) => Ok((
                v.header.timestamp as u64,
                v.number,
                v.coinbase_ascii.clone().unwrap_or_default(),
            )),
            None => Err(anyhow!("header not found")),
        })
        .collect()
}

pub fn get_parents(hash: [u8; 32], limit: usize) -> Vec<[u8; 32]> {
    let r = CHAIN.read().unwrap();

    let mut last = match r.known_headers.get(&hash) {
        Some(v) => v,
        None => return vec![],
    };

    let mut result = Vec::with_capacity(limit);
    while result.len() < limit {
        if last.header.parent == [0u8; 32] {
            break;
        }
        last = match r.known_headers.get(&last.header.parent) {
            Some(v) => v,
            None => return result,
        };
        result.push(last.header.hash);
    }

    result
}

pub fn filter_tx_spends(spends: Vec<(u32, [u8; 32], [u8; 32])>) -> Vec<(u32, [u8; 32])> {
    if spends.is_empty() {
        return vec![];
    }

    let r = CHAIN.read().unwrap();
    spends
        .into_iter()
        .filter(|item| r.most_work_chain.contains(&item.2))
        .map(|item| (item.0, item.1))
        .collect()
}

pub fn respond_to_getheaders(
    locator: &[&[u8; 32]],
    block_stop: [u8; 32],
) -> Option<Vec<BlockHeaderOwned>> {
    if locator.is_empty() {
        return None;
    }

    let r = CHAIN.read().unwrap();
    let mut found_match = None;

    for hash in locator.iter() {
        if r.most_work_chain.contains(*hash) {
            found_match = Some(**hash);
            break;
        }
    }

    let matched_hash = match found_match {
        Some(v) => v,
        None => {
            // None of the hashes in block_locator are on our most work chain.
            return None;
        }
    };

    let header = match r.known_headers.get(&matched_hash) {
        Some(v) => v,
        None => {
            // this should never happen
            let mut r = matched_hash.to_vec();
            r.reverse();
            panic!(
                "hash is in our most_work_chain but not in known_headers? {}",
                hex::encode(r)
            )
        }
    };

    let mut blocks_to_send_count = r.top_header.number - header.number;
    if blocks_to_send_count == 0 {
        return None;
    }
    if blocks_to_send_count > 2000 {
        blocks_to_send_count = 2000;
    }

    let mut result = Vec::with_capacity(blocks_to_send_count as usize);
    for i in 0..blocks_to_send_count {
        let hash = match r.number_to_hash.get(&(header.number + i)) {
            Some(v) => v,
            None => {
                // this should never happen
                panic!(
                    "we have a higher top_header but no entry in number_to_hash, number: {}",
                    header.number + i
                )
            }
        };
        if *hash == block_stop {
            break;
        }
        result.push(r.known_headers[hash].header);
    }

    Some(result)
}

pub fn filter_and_populate_address_txs(
    address_txs: Vec<([u8; 32], [u8; 32], i64)>,
) -> Vec<AddressTransaction> {
    if address_txs.is_empty() {
        return vec![];
    }

    let r = CHAIN.read().unwrap();
    address_txs
        .into_iter()
        .filter(|item| r.most_work_chain.contains(&item.1))
        .map(|item| {
            let block = r.known_headers.get(&item.1).unwrap();

            AddressTransaction {
                transaction_hash: item.0,
                block_hash: item.1,
                block_number: block.number,
                timestamp: block.header.timestamp,
                value: item.2,
                single_other_address: None,
                distinct_other_addresses: 0,
            }
        })
        .collect()
}

// generates the frontpage data from nothing but the top block hash, pulling all data from the store
async fn generate_frontpage_data(top_block_hash: [u8; 32]) {
    let stats = match calculate_stats(top_block_hash, None).await {
        Ok(v) => v,
        Err(_) => {
            // Right now calculate_stats only returns Err if it was cancelled, so we can safely return here
            return;
        }
    };
    let mut latest_blocks = Vec::with_capacity(FRONTPAGE_BLOCKS_COUNT);
    let mut latest_txs = Vec::with_capacity(FRONTPAGE_TXS_COUNT);

    let mut current_header = {
        let r = CHAIN.read().unwrap();
        r.known_headers[&top_block_hash].clone()
    };

    loop {
        if latest_blocks.len() == FRONTPAGE_BLOCKS_COUNT && latest_txs.len() == FRONTPAGE_TXS_COUNT
        {
            break;
        }

        // pull the tx hashes that are in this block from rocksdb
        let block_tx_hashes = db::rocksdb::get_block_tx_entries(current_header.header.hash)
            .await
            .expect("to fetch block txs from rocksdb");

        let (miner_id, miner_name, recent_miner_share) =
            miners::get_miner_for_block_and_share(current_header.header.hash).unwrap();
        latest_blocks.push(ShortBlock {
            number: current_header.number,
            hash: current_header.header.hash,
            tx_count: block_tx_hashes.len() as u64,
            reward_sats: block_tx_hashes[0].value_sats,
            btc_price: 0., // todo
            timestamp: current_header.header.timestamp,
            miner_id,
            miner_name,
            recent_miner_share,
        });

        let missing_txs = min(
            FRONTPAGE_TXS_COUNT - latest_txs.len(),
            block_tx_hashes.len(),
        );
        for tx_entry in block_tx_hashes.iter().take(missing_txs) {
            let tx = db::rocksdb::get_analyzed_tx(tx_entry.hash)
                .await
                .expect("to get analyzed tx from rocksdb");
            let mut human_hash = tx_entry.hash;
            human_hash.reverse();
            let human_hash = hex::encode(human_hash);
            latest_txs.push(ShortTx {
                hash: human_hash,
                value: tx.txouts_sum,
                size_wus: tx.size_wus,
                block_number: current_header.number,
                block_hash: current_header.header.hash,
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

    let update_for_broadcast = {
        let mut w = FRONTPAGE_STATS.write().unwrap();
        w.data = FrontPageData {
            stats,
            latest_blocks,
            latest_txs,
            sync_stats: None,
        };
        w.serialized = serde_json::to_string(&w.data).unwrap();
        w.serialized.clone()
    };
    _ = FRONTPAGE_UPDATE_BROADCAST.send(update_for_broadcast);
}

// appends the new blocks to the already-generated frontpage data
async fn update_frontpage_data(
    top_block_hash: [u8; 32],
    metrics_cache: &[BlockMetrics],
    analyzed_cache: &[&[SerializedTx<'_>]],
    sync_speed_blocks_per_second: Option<f64>,
) {
    let stats = match calculate_stats(top_block_hash, Some(metrics_cache)).await {
        Ok(v) => v,
        Err(_) => {
            // Right now calculate_stats only returns Err if it was cancelled, so we can safely return here
            return;
        }
    };
    let mut latest_blocks = Vec::with_capacity(FRONTPAGE_BLOCKS_COUNT);
    let mut latest_txs = Vec::with_capacity(FRONTPAGE_TXS_COUNT);

    let r = CHAIN.read().unwrap();
    let mut i = analyzed_cache.len();
    let mut current_header = &r.known_headers[&top_block_hash];

    let top_synced_block_number = current_header.number;

    while i > 0 {
        if latest_blocks.len() == FRONTPAGE_BLOCKS_COUNT && latest_txs.len() == FRONTPAGE_TXS_COUNT
        {
            break;
        }

        let block_txs = &analyzed_cache[i - 1];

        let (miner_id, miner_name, recent_miner_share) =
            miners::get_miner_for_block_and_share(current_header.header.hash).unwrap();
        latest_blocks.push(ShortBlock {
            number: current_header.number,
            hash: current_header.header.hash,
            tx_count: block_txs.len() as u64,
            reward_sats: block_txs[0].txouts_sum,
            btc_price: 0., // TODO
            timestamp: current_header.header.timestamp,
            miner_id,
            miner_name,
            recent_miner_share,
        });

        let missing_txs = min(FRONTPAGE_TXS_COUNT - latest_txs.len(), block_txs.len());
        for j in 0..missing_txs {
            let tx = block_txs[j];
            let mut human_hash = tx.hash;
            human_hash.reverse();
            let human_hash = hex::encode(human_hash);
            latest_txs.push(ShortTx {
                hash: human_hash,
                value: tx.txouts_sum,
                size_wus: tx.size_wus,
                block_number: current_header.number,
                block_hash: current_header.header.hash,
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

    // Calculate sync status here
    let remaining_blocks = r.top_header.number - top_synced_block_number;
    let sync_stats = match sync_speed_blocks_per_second {
        Some(speed) => {
            if remaining_blocks != 0 {
                let remaining_seconds = remaining_blocks as f64 / speed;
                info!("sync complete ETA"; "remaining_seconds" => remaining_seconds, "remaining_blocks" => remaining_blocks);
                Some(SyncStats {
                    total_blocks: r.top_header.number + 1,
                    synced_blocks: top_synced_block_number + 1,
                    approx_remaining_seconds: remaining_seconds,
                })
            } else {
                None
            }
        }
        None => None,
    };

    // We've either got all of the required blocks and txs, or ran out of blocks in `analyzed`.
    // If we're still missing blocks/txs, copy them from the previous response
    let missing_blocks = FRONTPAGE_BLOCKS_COUNT - latest_blocks.len();
    let missing_txs = FRONTPAGE_TXS_COUNT - latest_txs.len();
    let mut w = FRONTPAGE_STATS.write().unwrap();
    if missing_blocks != 0 {
        let to_keep_from_old = min(missing_blocks, w.data.latest_blocks.len());
        latest_blocks.extend_from_slice(&w.data.latest_blocks[0..to_keep_from_old]);
    }
    if missing_txs != 0 {
        let to_keep_from_old = min(missing_txs, w.data.latest_txs.len());
        latest_txs.extend_from_slice(&w.data.latest_txs[0..to_keep_from_old]);
    }

    w.data = FrontPageData {
        stats,
        latest_blocks,
        latest_txs,
        sync_stats,
    };
    w.serialized = serde_json::to_string(&w.data).unwrap();
    let update = if metrics_cache.len() >= FRONTPAGE_BLOCKS_COUNT {
        FrontPageDataUpdate::Snapshot(w.data.clone())
    } else {
        FrontPageDataUpdate::Delta(w.data.clone())
    };
    drop(w);
    _ = FRONTPAGE_UPDATE_BROADCAST.send(serde_json::to_string(&update).unwrap());
}

async fn calculate_stats(top: [u8; 32], cache: Option<&[BlockMetrics]>) -> Result<Stats> {
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
    for hash in relevant_blocks.iter().skip(bms.len()) {
        let hash = *hash;
        handles.push(tokio::task::spawn_blocking(move || {
            db::sqlite::find_block_metrics(hash)
        }));
    }

    let mut results = Vec::with_capacity(handles.len());
    for handle in handles {
        results.push(handle.await?);
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

            let mut total_vol = 0;
            let mut total_txs = 0;
            let mut median_fees_sum = 0.;
            for bm in &bms[boundary..i] {
                total_vol += bm.volume;
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

    Ok(result)
}

fn load_headers_from_sqlite() {
    let headers = db::sqlite::get_all_headers();
    let mut w = CHAIN.write().unwrap();
    for new_header in headers {
        if new_header.total_work > w.top_header.total_work {
            w.top_header = new_header.clone();
            METRIC_TOP_HEADER_HEIGHT.set(new_header.number as i64);
        }

        w.known_headers
            .insert(new_header.header.hash, new_header.clone());
    }

    generate_top_work_chain_hashset(&mut w);
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
    current_time > top_header_time && current_time - top_header_time > 24 * 3600
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
                    BlockHeaderRef::Owned(&current.header).human_hash()
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

struct HeaderApplicationResult {
    removed_blocks: Vec<([u8; 32], u64)>,
    // oldest to newest
    added_blocks: Vec<([u8; 32], u64)>,
}
// if the returned value is Err, the header could not be applied
// if the returned value is Ok(None), the header was inserted but no bodies need to be downloaded & applied
// if the returned value is Ok(Some(har)), we must download blocks from `har.added_blocks` and apply them in that order.
//
// if `is_headersync` is true, Ok(Some()) cannot be returned
fn validate_and_apply_header_inner(
    header: BlockHeaderRef,
    w: &mut Chain,
    is_headersync: bool,
) -> Result<Option<HeaderApplicationResult>> {
    // ignore headers we already have
    if w.known_headers.contains_key(&header.hash()) {
        return Ok(None);
    }

    // ignore headers that we dont have the parent of
    let parent = match w.known_headers.get(header.parent().as_slice()) {
        Some(parent) => parent,
        None => bail!("dont have the parent"),
    };

    // make sure the difficulty transition is valid
    let expected_bits = w.get_next_bits_required(parent);
    if expected_bits != header.bits() {
        bail!(format!(
            "invalid difficulty transition for {}, expected {} {} but got {} {}",
            header.human_hash(),
            hex::encode(expected_bits.to_be_bytes()),
            expected_bits,
            hex::encode(header.bits().to_be_bytes()),
            header.bits()
        ));
    }

    let new_total_work = parent.total_work + header.get_work();
    let new_header = BlockHeaderWithNumber {
        header: header.to_owned(),
        number: parent.number + 1,
        fetched_full: false,
        total_work: new_total_work,
        coinbase_ascii: None,
    };
    let parent_number = parent.number;

    w.known_headers
        .insert(new_header.header.hash, new_header.clone());
    let new_header_clone = new_header.clone();
    tokio::spawn(db::sqlite::insert_header(new_header_clone));

    // if this header wouldn't become the new top block, return early
    if new_header.total_work < w.top_header.total_work {
        return Ok(None);
    }
    // this header would become our new top_header

    METRIC_TOP_HEADER_HEIGHT.set(new_header.number as i64);

    // if we're syncing headers right now, simply update the top header
    if is_headersync {
        w.top_header = new_header;
        return Ok(None);
    }

    // if the parent of this header is the top header, then it's a simple chain extension
    if new_header.header.parent == w.top_header.header.hash {
        w.top_header = new_header;
        // queue this block to be downloaded and applied
        return Ok(Some(HeaderApplicationResult {
            added_blocks: vec![(w.top_header.header.hash, w.top_header.number)],
            removed_blocks: vec![],
        }));
    }

    // this is a fork, so we return all blocks since the split
    // and update DOWNLOADED_BLOCKS
    let (mut visited_new, visited_old) = {
        let mut from_new_hash = new_header.header.parent;
        let mut from_new_number = new_header.number;
        let mut visited_new = vec![(new_header.header.hash, new_header.number)];
        let mut visited_old = vec![(w.top_header.header.hash, w.top_header.number)];
        // first find the block that's on the main chain with the same block number as the parent
        let (mut from_old_hash, mut from_old_number) = {
            let mut last = w.top_header.header.hash;
            let mut last_number = w.top_header.number;
            loop {
                let header = w.known_headers.get(&last).unwrap();
                if header.number == parent_number {
                    break;
                }
                visited_old.push((header.header.hash, header.number));
                last = header.header.parent;
                last_number -= 1;
            }
            (last, last_number)
        };
        // then step back until they match
        loop {
            if from_new_hash == from_old_hash {
                break;
            }
            visited_new.push((from_new_hash, from_new_number));
            visited_old.push((from_old_hash, from_old_number));
            from_old_hash = w.known_headers.get(&from_old_hash).unwrap().header.parent;
            from_old_number -= 1;
            from_new_hash = w.known_headers.get(&from_new_hash).unwrap().header.parent;
            from_new_number -= 1;
        }
        (visited_new, visited_old)
    };

    w.top_header = new_header;

    visited_new.reverse();

    let new_count = calculate_downloaded_blocks(w);
    DOWNLOADED_BLOCKS.store(new_count, Ordering::Relaxed);
    METRIC_FULL_BLOCKS_DOWNLOADED.set(new_count as i64);

    Ok(Some(HeaderApplicationResult {
        removed_blocks: visited_old,
        added_blocks: visited_new,
    }))
}

fn validate_and_apply_headers(headers: &[BlockHeaderBorrowed]) -> Result<()> {
    let mut w = CHAIN.write().unwrap();
    for header in headers {
        validate_and_apply_header_inner(BlockHeaderRef::Borrowed(header), &mut w, true)?;
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

fn generate_top_work_chain_hashset(w: &mut Chain) {
    let mut new_hashset = HashSet::with_capacity(1024 * 1024);
    let mut new_number_to_hash = HashMap::with_capacity(1024 * 1024);
    let mut last = &w.top_header;
    loop {
        new_hashset.insert(last.header.hash);
        new_number_to_hash.insert(last.number, last.header.hash);
        if last.number == 0 {
            break;
        }
        last = w.known_headers.get(last.header.parent.as_slice()).unwrap();
    }
    w.most_work_chain = new_hashset;
    w.number_to_hash = new_number_to_hash;
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
fn get_block_hashes_to_download(chain: &Chain) -> Option<(Vec<[u8; 32]>, u64)> {
    let downloaded = DOWNLOADED_BLOCKS.load(Ordering::Relaxed);
    if downloaded > chain.top_header.number {
        return None;
    }
    let mut all = Vec::with_capacity((chain.top_header.number + 1 - downloaded) as usize);

    let mut c = &chain.top_header;
    let mut last_added: u64;
    loop {
        all.push(c.header.hash);
        last_added = c.number;
        if c.number == 0 {
            break;
        }
        c = some_or_break!(chain.known_headers.get(c.header.parent.as_slice()));

        if c.fetched_full {
            break;
        }
    }

    all.reverse();
    Some((all, last_added))
}

fn mark_as_downloaded(blocks: Vec<[u8; 32]>, coinbase_asciis: &[&[u8]]) {
    let mut w = CHAIN.write().unwrap();
    for (block, coinbase_ascii) in blocks.iter().zip(coinbase_asciis) {
        let h = w
            .known_headers
            .get_mut(block)
            .expect("the header to be present in known_headers");
        h.fetched_full = true;
        h.coinbase_ascii = Some(String::from_utf8_lossy(coinbase_ascii).to_string());
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
