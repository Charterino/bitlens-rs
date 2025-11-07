use std::{
    collections::HashSet,
    sync::{LazyLock, Mutex},
};

use crate::types::addressportnetwork::AddressPortNetwork;

static CONNECTED_PEERS: LazyLock<Mutex<HashSet<AddressPortNetwork>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

pub struct TrackedConnection {
    apn: AddressPortNetwork,
}

impl TrackedConnection {
    pub fn new(apn: AddressPortNetwork) -> Self {
        let mut w = CONNECTED_PEERS.lock().unwrap();
        assert!(w.insert(apn.clone()));
        Self { apn }
    }
}

impl Drop for TrackedConnection {
    fn drop(&mut self) {
        let mut w = CONNECTED_PEERS.lock().unwrap();
        assert!(w.remove(&self.apn));
    }
}

pub fn get_all_connected_peers() -> Vec<String> {
    let r = CONNECTED_PEERS.lock().unwrap();
    r.iter().map(|v| v.to_string()).collect()
}

pub fn is_connected_now(apn: &AddressPortNetwork) -> bool {
    let r = CONNECTED_PEERS.lock().unwrap();
    r.contains(apn)
}
