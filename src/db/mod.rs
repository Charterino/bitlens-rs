use std::sync::LazyLock;

use anyhow::{Ok, Result};
use batched::{InsertPeerRequest, start_insert_peer_batcher};
use deadpool_sqlite::{Config, Pool, Runtime};
use migrations::MIGRATIONS;
use tokio::sync::mpsc::Sender;
//..use sqlite::{Connection, ConnectionThreadSafe};

use crate::types::addressportnetwork::AddressPortNetwork;

static POOL: LazyLock<Pool> = LazyLock::new(|| {
    let config = Config::new("bitlens.db");
    config.create_pool(Runtime::Tokio1).unwrap()
});

static INSERT_PEER_QUEUE: LazyLock<Sender<InsertPeerRequest>> =
    LazyLock::new(|| start_insert_peer_batcher());

mod batch;
mod batched;
mod migrations;

pub async fn setup() {
    let conn = POOL.get().await.unwrap();
    conn.interact(|conn| {
        for m in MIGRATIONS {
            conn.execute(m, []).unwrap();
        }
    })
    .await
    .unwrap();
}

pub async fn get_all_peers() -> Vec<AddressPortNetwork> {
    let conn = POOL.get().await.unwrap();
    conn.interact(|conn| {
        let mut stmt = conn
            .prepare_cached("SELECT network_id, address, port FROM peers;")
            .unwrap();
        let iter = stmt.query_map([], |row| {
            Result::Ok(AddressPortNetwork {
                network_id: row.get(0)?,
                port: row.get(2)?,
                address: row.get(1)?,
            })
        })?;
        let c = iter
            .map(|x| x.unwrap())
            .collect::<Vec<AddressPortNetwork>>();
        Ok(c)
    })
    .await
    .unwrap()
    .unwrap()
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
