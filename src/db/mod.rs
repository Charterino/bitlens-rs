use std::{
    borrow::Cow,
    collections::HashMap,
    sync::{LazyLock, Mutex},
    time::SystemTime,
};

use anyhow::Result;
use batch::start_batcher;
use batched::{
    BanPeerRequest, DeletePeerRequest, InsertHeaderRequest, InsertPeerRequest,
    UpdatePeerBlockHeightRequest, UpdatePeerFromVersionRequest,
};
use migrations::MIGRATIONS;
use rusqlite::Connection;
use tokio::sync::mpsc::Sender;

use crate::{
    packets::blockheader::BlockHeader,
    types::{addressportnetwork::AddressPortNetwork, blockheaderwithnumber::BlockHeaderWithNumber},
    util::genesis::GENESIS_HEADER,
};

// Suboptimal because we cant have multiple simultanious reads, but it will do for now.
static CONNECTION: LazyLock<Mutex<Connection>> = LazyLock::new(|| {
    let conn = Connection::open("bitlens.db").unwrap();
    conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA wal_autocheckpoint=1000;")
        .unwrap();
    Mutex::new(conn)
});

static INSERT_PEER_QUEUE: LazyLock<Sender<InsertPeerRequest>> = LazyLock::new(|| {
    start_batcher(
        "INSERT INTO peers (network_id, address, port, first_seen, services) VALUES (?, ?, ?, ?, 0);",
    )
});

static BAN_PEER_QUEUE: LazyLock<Sender<BanPeerRequest>> = LazyLock::new(|| {
    start_batcher(
        "INSERT INTO banned_peers (network_id, address, port, banned_at, banned_until) VALUES (?, ?, ?, ?, ?);",
    )
});

static DELETE_PEER_QUEUE: LazyLock<Sender<DeletePeerRequest>> =
    LazyLock::new(|| start_batcher("DELETE FROM peers WHERE address = ? AND port = ?;"));

static UPDATE_PEER_BLOCK_HEIGHT_QUEUE: LazyLock<Sender<UpdatePeerBlockHeightRequest>> =
    LazyLock::new(|| start_batcher("UPDATE peers SET height = ? WHERE address = ? AND port = ?;"));

static INSERT_HEADER_QUEUE: LazyLock<Sender<InsertHeaderRequest>> = LazyLock::new(|| {
    start_batcher(
        "INSERT INTO headers (version, previous_block, merkle_root, timestamp, bits, nonce, block_number, block_hash) VALUES (?, ?, ?, ?, ?, ?, ?, ?);",
    )
});

static UPDATE_PEER_FROM_VERSION_QUEUE: LazyLock<Sender<UpdatePeerFromVersionRequest>> =
    LazyLock::new(|| {
        start_batcher(
            "UPDATE peers SET services = ?, height = ?, user_agent = ? WHERE address = ? AND port = ?;",
        )
    });

mod batch;
mod batched;
mod migrations;

pub async fn setup() {
    let conn = CONNECTION.lock().unwrap();
    for m in MIGRATIONS {
        conn.execute(m, []).unwrap();
    }
}

pub async fn get_all_peers() -> Vec<AddressPortNetwork> {
    let conn = CONNECTION.lock().unwrap();
    let mut stmt = conn
        .prepare_cached("SELECT network_id, address, port FROM peers;")
        .unwrap();
    let iter = stmt
        .query_map([], |row| {
            Result::Ok(AddressPortNetwork {
                network_id: row.get(0)?,
                port: row.get(2)?,
                address: row.get(1)?,
            })
        })
        .unwrap();
    iter.map(|x| x.unwrap())
        .collect::<Vec<AddressPortNetwork>>()
}

pub async fn insert_peer(peer: AddressPortNetwork, first_seen: u64) {
    INSERT_PEER_QUEUE
        .send(InsertPeerRequest {
            apn: peer,
            first_seen,
        })
        .await
        .unwrap();
}

pub async fn ban_peer(peer: AddressPortNetwork, banned_at: SystemTime, banned_until: SystemTime) {
    BAN_PEER_QUEUE
        .send(BanPeerRequest {
            apn: peer,
            banned_at,
            banned_until,
        })
        .await
        .unwrap();
}

pub async fn delete_peer(peer: AddressPortNetwork) {
    DELETE_PEER_QUEUE
        .send(DeletePeerRequest { apn: peer })
        .await
        .unwrap();
}

pub async fn update_peer_block_height(peer: AddressPortNetwork, new_height: u32) {
    UPDATE_PEER_BLOCK_HEIGHT_QUEUE
        .send(UpdatePeerBlockHeightRequest {
            apn: peer,
            new_height,
        })
        .await
        .unwrap();
}

pub async fn update_peer_from_version(
    peer: AddressPortNetwork,
    services: u64,
    block_height: u32,
    user_agent: Vec<u8>,
) {
    UPDATE_PEER_FROM_VERSION_QUEUE
        .send(UpdatePeerFromVersionRequest {
            apn: peer,
            services,
            block_height,
            user_agent,
        })
        .await
        .unwrap();
}

pub async fn get_all_headers() -> Vec<BlockHeaderWithNumber<'static>> {
    let conn = CONNECTION.lock().unwrap();
    let mut stmt = conn.prepare_cached("SELECT version, previous_block, merkle_root, timestamp, bits, nonce, block_number, block_hash, fetched_full FROM headers ORDER BY block_number ASC;").unwrap();
    let mut work_totals = HashMap::new();
    work_totals.insert(GENESIS_HEADER.hash, GENESIS_HEADER.get_work());
    let iter = stmt
        .query_map([], |row| {
            let header = BlockHeader {
                version: row.get(0)?,
                parent: Cow::Owned(row.get(1)?),
                merkle_root: Cow::Owned(row.get(2)?),
                timestamp: row.get(3)?,
                bits: row.get(4)?,
                nonce: row.get(5)?,
                txs_count: 0,
                hash: row.get(7)?,
            };
            let parent_work = match work_totals.get(header.parent.as_slice()) {
                Some(w) => w,
                None => return Err(rusqlite::Error::ExecuteReturnedResults),
            };
            let our_work = *parent_work + header.get_work();
            work_totals.insert(header.hash, our_work);
            Ok(BlockHeaderWithNumber {
                header,
                number: row.get(6)?,
                fetched_full: row.get(8)?,
                total_work: our_work,
            })
        })
        .unwrap();
    iter.map(|x| x.unwrap())
        .collect::<Vec<BlockHeaderWithNumber>>()
}

pub async fn insert_header(header: &BlockHeaderWithNumber<'_>) {
    INSERT_HEADER_QUEUE
        .send(InsertHeaderRequest {
            parent: header.header.parent.clone().into_owned(),
            merkle_root: header.header.merkle_root.clone().into_owned(),
            timestamp: header.header.timestamp,
            bits: header.header.bits,
            nonce: header.header.nonce,
            version: header.header.version,
            number: header.number,
            hash: header.header.hash,
        })
        .await
        .unwrap();
}
