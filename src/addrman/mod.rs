use std::{
    collections::HashMap,
    mem,
    sync::{LazyLock, Mutex},
};

use crate::{db, types::addressportnetwork::AddressPortNetwork};

static PEERS_TO_CHECK: LazyLock<Mutex<Vec<AddressPortNetwork>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

static KNOWN_PEERS: LazyLock<Mutex<HashMap<AddressPortNetwork, ()>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub async fn start() {
    let all_peers = db::get_all_peers().await;

    let mut known = KNOWN_PEERS.lock().unwrap();
    let mut to_check = PEERS_TO_CHECK.lock().unwrap();
    for peer in &all_peers {
        known.insert(peer.clone(), ());
    }
    *to_check = all_peers;
}

pub fn get_peers_to_check() -> Vec<AddressPortNetwork> {
    let mut w = PEERS_TO_CHECK.lock().unwrap();
    let mut old = Vec::new();
    mem::swap(&mut old, &mut w);
    old
}

pub async fn peers_seen(apns: Vec<AddressPortNetwork>, time: u64) {
    let mut should_pass_to_sqlite = Vec::new();
    {
        let mut w = KNOWN_PEERS.lock().unwrap();
        for apn in apns {
            if !w.contains_key(&apn) {
                w.insert(apn.clone(), ());
                should_pass_to_sqlite.push(apn);
            }
        }
    }
    if should_pass_to_sqlite.is_empty() {
        return;
    }
    {
        let mut w = PEERS_TO_CHECK.lock().unwrap();
        w.extend_from_slice(&should_pass_to_sqlite);
    }
    for apn in should_pass_to_sqlite {
        db::insert_peer(apn.clone(), time).await;
    }
}
