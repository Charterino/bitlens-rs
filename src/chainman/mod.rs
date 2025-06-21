use std::{
    sync::{
        LazyLock, RwLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Result, bail};
use blocksync::sync_blocks;
use chain::Chain;
use headersync::sync_headers;
use slog_scope::info;
use supercow::Supercow;

use crate::{
    db,
    packets::{
        SupercowVec, blockheader::BlockHeader, deepclone::DeepClone, getheaders::GetHeaders,
        packetpayload::PacketPayloadType,
    },
    some_or_break,
    types::blockheaderwithnumber::BlockHeaderWithNumber,
};

static SYNCING_HEADERS: AtomicBool = AtomicBool::new(false); // are we currently in the process of initial header sync?
#[allow(dead_code)]
static SYNCING_BODIES: AtomicBool = AtomicBool::new(false); // are we currently in the process of downloading blocks?
static DOWNLOADED_BLOCKS: AtomicU64 = AtomicU64::new(0); // It's important that this number only counts downloaded blocks from the main chain.

static CHAIN: LazyLock<RwLock<Chain>> = LazyLock::new(|| RwLock::new(Chain::default()));

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
        if need_ibd() {
            info!("downloading missing block bodies..");
            sync_blocks().await;
        }
    });
}

async fn load_headers_from_sqlite() {
    let headers = db::sqlite::get_all_headers().await;
    let mut w = CHAIN.write().unwrap();
    for new_header in headers {
        if new_header.total_work > w.top_header.total_work {
            w.top_header = new_header.clone();
        }

        w.known_headers
            .insert(new_header.header.hash, new_header.clone());
    }

    // After loading all headers from sqlite, figure out how many blocks we have downloaded
    DOWNLOADED_BLOCKS.store(calculate_downloaded_blocks_w(&w), Ordering::Relaxed);
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
    return r.top_header.number - DOWNLOADED_BLOCKS.load(Ordering::Relaxed) > 0;
}

// Constructs a `getheaders` packet with `BlockLocator` that allows the remote peer to detect that we are on the wrong branch
fn build_get_headers<'a>() -> PacketPayloadType<'a> {
    let r = CHAIN.read().unwrap();
    if r.top_header.number == 0 {
        // Starting from zero!
        return PacketPayloadType::GetHeaders(Supercow::owned(GetHeaders {
            version: 70016,
            block_locator: SupercowVec {
                inner: Supercow::owned(vec![Supercow::owned(r.top_header.header.hash)]),
            },
            hash_stop: Supercow::owned([0u8; 32]),
        }));
    }

    let mut block_locator = Vec::with_capacity(10);
    let mut step = 1;

    let mut current = &r.top_header;
    block_locator.push(Supercow::owned(current.header.hash));

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
                block_locator.push(Supercow::owned(current.header.hash));
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

    PacketPayloadType::GetHeaders(Supercow::owned(GetHeaders {
        version: 70016,
        block_locator: SupercowVec {
            inner: Supercow::owned(block_locator),
        },
        hash_stop: Supercow::owned([0u8; 32]),
    }))
}

fn validate_and_apply_header_inner(header: &BlockHeader, w: &mut Chain) -> Result<()> {
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
        header: header.deep_clone(),
        number: parent.number + 1,
        fetched_full: false,
        total_work: new_total_work,
    };

    if new_header.total_work > w.top_header.total_work {
        // since top_header is changing, we might need to adjust DOWNLOADED_BLOCKS
        let should_recount = *new_header.header.parent != w.top_header.header.hash;
        w.top_header = new_header.clone();
        if should_recount {
            let new_count = calculate_downloaded_blocks_w(w);
            DOWNLOADED_BLOCKS.store(new_count, Ordering::Relaxed);
        }
    }

    w.known_headers
        .insert(new_header.header.hash, new_header.clone());
    tokio::spawn(async move {
        db::sqlite::insert_header(&new_header).await;
    });

    Ok(())
}

fn validate_and_apply_headers(headers: &[&BlockHeader]) -> Result<()> {
    let mut w = CHAIN.write().unwrap();
    for header in headers {
        validate_and_apply_header_inner(header, &mut w)?;
    }

    Ok(())
}

fn calculate_downloaded_blocks_w(r: &Chain) -> u64 {
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
