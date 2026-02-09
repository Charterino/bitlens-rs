use super::{
    batch::start_batcher,
    batched::{
        BanPeerRequest, DeletePeerRequest, InsertHeaderRequest, InsertPeerRequest,
        UpdatePeerBlockHeightRequest, UpdatePeerFromVersionRequest,
    },
    migrations::MIGRATIONS,
};
use crate::{
    db::batched::UpdatePeerFirstOnlineRequest,
    metrics::METRIC_SQLITE_REQUESTS_TIME,
    packets::blockheader::{BlockHeaderOwned, BlockHeaderRef},
    types::{
        addressportnetwork::AddressPortNetwork, blockheaderwithnumber::BlockHeaderWithNumber,
        blockmetrics::BlockMetrics,
    },
    util::genesis::GENESIS_HEADER,
};
use anyhow::Result;
use primitive_types::U256;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use slog_scope::warn;
use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex},
    time::SystemTime,
};
use tokio::{sync::mpsc::Sender, time::Instant};

// Suboptimal because we cant have multiple simultanious reads, but it will do for now.
pub(crate) static CONNECTION: LazyLock<Mutex<Connection>> = LazyLock::new(|| {
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
        "INSERT INTO banned_peers (network_id, address, port, banned_at, banned_until, reasons_list) VALUES (?, ?, ?, ?, ?, ?);",
    )
});

static DELETE_PEER_QUEUE: LazyLock<Sender<DeletePeerRequest>> =
    LazyLock::new(|| start_batcher("DELETE FROM peers WHERE address = ? AND port = ?;"));

static UPDATE_PEER_BLOCK_HEIGHT_QUEUE: LazyLock<Sender<UpdatePeerBlockHeightRequest>> =
    LazyLock::new(|| start_batcher("UPDATE peers SET height = ? WHERE address = ? AND port = ?;"));

static INSERT_HEADER_QUEUE: LazyLock<Sender<InsertHeaderRequest>> = LazyLock::new(|| {
    start_batcher(
        "INSERT INTO headers (version, previous_block, merkle_root, timestamp, bits, nonce, block_number, block_hash) VALUES (?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT DO NOTHING;",
    )
});

static UPDATE_PEER_FROM_VERSION_QUEUE: LazyLock<Sender<UpdatePeerFromVersionRequest>> =
    LazyLock::new(|| {
        start_batcher(
            "UPDATE peers SET services = ?, height = ?, user_agent = ? WHERE address = ? AND port = ?;",
        )
    });

static UPDATE_PEER_FIRST_ONLINE: LazyLock<Sender<UpdatePeerFirstOnlineRequest>> = LazyLock::new(
    || {
        start_batcher(
            "UPDATE peers SET first_online = ? WHERE address = ? AND port = ? AND first_online is NULL;",
        )
    },
);

pub(crate) async fn setup_sqlite() {
    {
        let conn = CONNECTION.lock().unwrap();
        for m in MIGRATIONS {
            match conn.execute(m, []) {
                Ok(_) => {}
                Err(e) => {
                    warn!("failed to execute migration"; "error" => e.to_string());
                }
            }
        }
    }
    ensure_genesis_header_exists_in_db().await;
}

async fn ensure_genesis_header_exists_in_db() {
    insert_header(BlockHeaderWithNumber {
        header: *GENESIS_HEADER,
        number: 0,
        fetched_full: false,
        total_work: BlockHeaderRef::Owned(&GENESIS_HEADER).get_work(),
        coinbase_ascii: None,
    })
    .await;
}

pub fn get_all_peers() -> Vec<AddressPortNetwork> {
    let conn = CONNECTION.lock().unwrap();
    let start = Instant::now();
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
    let result = iter
        .map(|x| x.unwrap())
        .collect::<Vec<AddressPortNetwork>>();
    METRIC_SQLITE_REQUESTS_TIME.observe(Instant::now().duration_since(start).as_millis() as f64);
    result
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

pub async fn ban_peer(
    peer: AddressPortNetwork,
    banned_at: SystemTime,
    banned_until: SystemTime,
    reasons: Vec<String>,
) {
    BAN_PEER_QUEUE
        .send(BanPeerRequest {
            apn: peer,
            banned_at,
            banned_until,
            reasons,
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

pub fn get_all_headers() -> Vec<BlockHeaderWithNumber> {
    let conn = CONNECTION.lock().unwrap();
    let start = Instant::now();
    let mut stmt = conn.prepare_cached("SELECT version, previous_block, merkle_root, timestamp, bits, nonce, block_number, block_hash, fetched_full, coinbase_ascii FROM headers ORDER BY block_number ASC;").unwrap();
    let mut work_totals = HashMap::new();
    work_totals.insert([0u8; 32], U256::zero());
    let iter = stmt
        .query_map([], |row| {
            let header = BlockHeaderOwned {
                version: row.get(0)?,
                parent: row.get(1)?,
                merkle_root: row.get(2)?,
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
            let our_work = *parent_work + BlockHeaderRef::Owned(&header).get_work();
            work_totals.insert(header.hash, our_work);

            let coinbase_ascii = row
                .get::<usize, Option<Vec<u8>>>(9)?
                .map(|v| String::from_utf8_lossy(&v).to_string());

            Ok(BlockHeaderWithNumber {
                header,
                number: row.get(6)?,
                fetched_full: row.get(8)?,
                total_work: our_work,
                coinbase_ascii,
            })
        })
        .unwrap();
    let result = iter.flatten().collect::<Vec<BlockHeaderWithNumber>>();
    METRIC_SQLITE_REQUESTS_TIME.observe(Instant::now().duration_since(start).as_millis() as f64);
    result
}

pub async fn insert_header(header: BlockHeaderWithNumber) {
    INSERT_HEADER_QUEUE
        .send(InsertHeaderRequest {
            parent: header.header.parent,
            merkle_root: header.header.merkle_root,
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

pub async fn update_peer_first_online(peer: AddressPortNetwork, ts: u64) {
    UPDATE_PEER_FIRST_ONLINE
        .send(UpdatePeerFirstOnlineRequest { apn: peer, ts })
        .await
        .unwrap();
}

pub fn mark_blocks_as_downloaded(
    hashes: &[[u8; 32]],
    coinbase_asciis: &[&[u8]],
    metrics: &[BlockMetrics],
) {
    assert_eq!(hashes.len(), metrics.len());
    assert_eq!(hashes.len(), coinbase_asciis.len());
    let mut conn = CONNECTION.lock().unwrap();
    let start = Instant::now();
    let tx = conn.transaction().expect("to begin sqlite transaction");
    {
        let mut stmt = tx
            .prepare_cached(
                "UPDATE headers SET fetched_full = ?, coinbase_ascii = ? WHERE block_hash = ?",
            )
            .expect("to prepare statement");

        for (hash, coinbase_ascii) in hashes.iter().zip(coinbase_asciis) {
            stmt.execute((true, coinbase_ascii, hash))
                .expect("to execute fetched_full update");
        }

        let mut stmt = tx
            .prepare_cached("INSERT OR REPLACE INTO block_stats VALUES (?, ?, ?, ?, ?, ?, ?, ?)")
            .expect("to prepare statement");

        let zipped = hashes.iter().zip(metrics);
        for (hash, metrics) in zipped {
            stmt.execute((
                hash,
                metrics.fees_total,
                metrics.volume,
                metrics.txs_count,
                metrics.average_fee_rate,
                metrics.lowest_fee_rate,
                metrics.highest_fee_rate,
                metrics.median_fee_rate,
            ))
            .expect("to insert block_stats");
        }
    }
    tx.commit().expect("to commit tx");
    METRIC_SQLITE_REQUESTS_TIME.observe(Instant::now().duration_since(start).as_millis() as f64);
}

pub fn update_coinbase_asciis(hashes: &[[u8; 32]], coinbase_asciis: &[Vec<u8>]) {
    assert_eq!(hashes.len(), coinbase_asciis.len());
    let mut conn = CONNECTION.lock().unwrap();
    let start = Instant::now();
    let tx = conn.transaction().expect("to begin sqlite transaction");
    {
        let mut stmt = tx
            .prepare_cached("UPDATE headers SET coinbase_ascii = ? WHERE block_hash = ?")
            .expect("to prepare statement");

        for (hash, coinbase_ascii) in hashes.iter().zip(coinbase_asciis) {
            stmt.execute((coinbase_ascii, hash))
                .expect("to execute coinbase_ascii update");
        }
    }
    tx.commit().expect("to commit tx");
    METRIC_SQLITE_REQUESTS_TIME.observe(Instant::now().duration_since(start).as_millis() as f64);
}

pub fn find_block_metrics(hash: [u8; 32]) -> BlockMetrics {
    let conn = CONNECTION.lock().unwrap();
    let start = Instant::now();
    let mut stmt = conn.prepare_cached("SELECT fees_total, volume, txs_count, avg_fee_rate, lowest_fee_rate, highest_fee_rate, median_fee_rate FROM block_stats WHERE hash=?;").expect("to prepare stmt");
    let bms = stmt.query_row([hash], |row| {
        Ok(BlockMetrics {
            fees_total: row.get(0).unwrap(),
            volume: row.get(1).unwrap(),
            txs_count: row.get(2).unwrap(),
            average_fee_rate: row.get(3).unwrap(),
            lowest_fee_rate: row.get(4).unwrap(),
            highest_fee_rate: row.get(5).unwrap(),
            median_fee_rate: row.get(6).unwrap(),
        })
    });

    METRIC_SQLITE_REQUESTS_TIME.observe(Instant::now().duration_since(start).as_millis() as f64);
    bms.unwrap()
}

#[derive(Serialize, Deserialize)]
pub struct PeerData {
    first_seen: u64,
    first_online: Option<u64>,
    #[serde(serialize_with = "crate::util::serialize_try_string::serialize_try_string")]
    user_agent: Option<Vec<u8>>,
    height: Option<u64>,
    services: Option<u64>,
}

pub fn get_peer(apn: &AddressPortNetwork) -> Result<PeerData> {
    let conn = CONNECTION.lock().unwrap();
    let mut stmt = conn
        .prepare_cached(
            "SELECT first_seen, first_online, user_agent, height, services FROM peers WHERE address = ? AND port = ? AND network_id = ?;",
        )
        .unwrap();
    Ok(
        stmt.query_one((&apn.address, apn.port, apn.network_id), |row| {
            Ok(PeerData {
                first_seen: row.get(0)?,
                first_online: row.get(1)?,
                user_agent: row.get(2)?,
                height: row.get(3)?,
                services: row.get(4)?,
            })
        })?,
    )
}

pub fn get_block_hashes_with_missing_coinbase_ascii() -> Result<Vec<[u8; 32]>> {
    let conn = CONNECTION.lock().unwrap();
    let mut stmt = conn
        .prepare_cached(
            "SELECT block_hash FROM headers WHERE coinbase_ascii IS NULL AND fetched_full = TRUE;",
        )
        .unwrap();
    Ok(stmt
        .query_map((), |row| row.get::<usize, [u8; 32]>(0))?
        .collect::<std::result::Result<Vec<[u8; 32]>, rusqlite::Error>>()?)
}
