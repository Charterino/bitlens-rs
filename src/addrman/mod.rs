use std::{
    collections::HashMap,
    mem,
    sync::{LazyLock, Mutex, RwLock},
    time::{Duration, SystemTime},
};

use tokio::time::sleep;

use crate::{
    connect::{connect_and_handshake, connection::HandshakedConnection},
    db::{self, ban_peer, delete_peer},
    types::addressportnetwork::AddressPortNetwork,
    util::online_list::OnlineList,
};

const BAN_PEER_DURATION: Duration = Duration::from_secs(3600 * 24 * 14); // two weeks

static PEERS_TO_CHECK: LazyLock<Mutex<Vec<AddressPortNetwork>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

static KNOWN_PEERS: LazyLock<Mutex<HashMap<AddressPortNetwork, ()>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

static ONLINE_PEERS: LazyLock<RwLock<OnlineList>> =
    LazyLock::new(|| RwLock::new(OnlineList::new(Duration::from_secs(60 * 3))));

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

pub async fn delete_and_ban_peer(apn: AddressPortNetwork) {
    let rn = SystemTime::now();
    let banned_until = rn.checked_add(BAN_PEER_DURATION).unwrap();
    delete_peer(apn.clone()).await;
    ban_peer(apn.clone(), rn, banned_until).await;
}

pub async fn update_peer_online(apn: AddressPortNetwork, services: u64) {
    let rn = SystemTime::now();
    let mut w = ONLINE_PEERS.write().unwrap();
    w.peer_online(apn, services, rn);
}

pub async fn update_peer_from_version(
    apn: AddressPortNetwork,
    services: u64,
    block_height: u32,
    user_agent: Vec<u8>,
) {
    db::update_peer_from_version(apn, services, block_height, user_agent).await;
}

pub fn get_alive_peer(services: Option<u64>) -> Option<AddressPortNetwork> {
    let r = ONLINE_PEERS.read().unwrap();
    r.random(services)
}

pub async fn connect_to_good_peer<'a>(services: Option<u64>) -> HandshakedConnection<'a> {
    loop {
        let peer = get_alive_peer(services);
        match peer {
            Some(peer) => match connect_and_handshake(&peer).await {
                Ok(conn) => return conn,
                Err(_) => {
                    // Failed to connect, try another peer
                }
            },
            None => {
                sleep(Duration::from_secs(1)).await;
            }
        }
    }
}
