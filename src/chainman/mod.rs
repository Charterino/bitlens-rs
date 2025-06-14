use std::{
    borrow::Cow,
    sync::{
        LazyLock, RwLock,
        atomic::{AtomicBool, AtomicUsize},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Result, bail};
use chain::Chain;
use headersync::sync_headers;
use slog_scope::info;

use crate::{
    db,
    packets::{
        blockheader::BlockHeader, deepclone::DeepClone, getheaders::GetHeaders,
        packetpayload::PacketPayloadType,
    },
    types::blockheaderwithnumber::BlockHeaderWithNumber,
};

static SYNCING_HEADERS: AtomicBool = AtomicBool::new(false); // are we currently in the process of initial header sync?
static SYNCING_BODIES: AtomicBool = AtomicBool::new(false); // are we currently in the process of downloading blocks?
static DOWNLOADED_BLOCKS: AtomicUsize = AtomicUsize::new(0); // It's important that this number only counts downloaded blocks from the main chain.

static CHAIN: LazyLock<RwLock<Chain>> = LazyLock::new(|| RwLock::new(Chain::default()));

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
    });
}

async fn load_headers_from_sqlite() {
    let headers = db::get_all_headers().await;
    let mut w = CHAIN.write().unwrap();
    for new_header in headers {
        if new_header.total_work > w.top_header.total_work {
            w.top_header = new_header.clone();
        }

        w.known_headers
            .insert(new_header.header.hash, new_header.clone());
    }
}

// Do we need to sync headers?
fn need_ihd() -> bool {
    let r = CHAIN.read().unwrap();
    let top_header_time = r.top_header.header.timestamp as u64;
    let current_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    return current_time - top_header_time > 24 * 3600;
}

// Constructs a `getheaders` packet with `BlockLocator` that allows the remote peer to detect that we are on the wrong branch
fn build_get_headers<'a>() -> PacketPayloadType<'a> {
    let r = CHAIN.read().unwrap();
    if r.top_header.number == 0 {
        // Starting from zero!
        return PacketPayloadType::GetHeaders(Cow::Owned(GetHeaders {
            version: 70016,
            block_locator: Cow::Owned(vec![Cow::Owned(r.top_header.header.hash)]),
            hash_stop: Cow::Owned([0u8; 32]),
        }));
    }

    let mut block_locator = Vec::with_capacity(10);
    let mut step = 1;

    let mut current = &r.top_header;
    block_locator.push(Cow::Owned(current.header.hash));

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
                block_locator.push(Cow::Owned(current.header.hash));
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

    PacketPayloadType::GetHeaders(Cow::Owned(GetHeaders {
        version: 70016,
        block_locator: Cow::Owned(block_locator),
        hash_stop: Cow::Owned([0u8; 32]),
    }))
}

fn validate_and_apply_header(header: &BlockHeader) -> Result<()> {
    let mut w = CHAIN.write().unwrap();
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
        w.top_header = new_header.clone();
    }

    w.known_headers
        .insert(new_header.header.hash, new_header.clone());
    drop(w);
    tokio::spawn(async move {
        db::insert_header(&new_header).await;
    });

    Ok(())
}
