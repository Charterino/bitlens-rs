use std::{
    collections::HashMap,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::types::addressportnetwork::AddressPortNetwork;

#[derive(Clone)]
struct AddressPortNetworkWithServicesAndTime {
    apn: AddressPortNetwork,
    services: u64,
    last_online: SystemTime,
}

impl Default for AddressPortNetworkWithServicesAndTime {
    fn default() -> Self {
        Self {
            apn: Default::default(),
            services: Default::default(),
            last_online: UNIX_EPOCH,
        }
    }
}

pub struct OnlineList {
    peers: Vec<AddressPortNetworkWithServicesAndTime>,
    threshold: Duration,
    next_write: usize,
    next_read: usize,
    known: HashMap<AddressPortNetwork, ()>,
}

impl OnlineList {
    pub fn new(threshold: Duration) -> Self {
        Self {
            peers: vec![AddressPortNetworkWithServicesAndTime::default(); 32],
            threshold,
            next_write: 0,
            next_read: 0,
            known: HashMap::new(),
        }
    }

    fn length(&self) -> usize {
        if self.next_write < self.next_read {
            return self.next_write - self.next_read + self.peers.len();
        }
        self.next_write - self.next_read
    }

    fn trim(&mut self) {
        let next_read = self.next_read;
        let rn = SystemTime::now();
        for i in 0..self.peers.len() {
            let real_i = (i + next_read) % self.peers.len();
            let item = self.peers.get(real_i).unwrap();
            if item.last_online < rn.checked_sub(self.threshold).unwrap() {
                self.next_read = (self.next_read + 1) % self.peers.len();
                self.known.remove(&item.apn);
            } else {
                break;
            }
        }
    }

    pub fn peer_online(
        &mut self,
        peer: AddressPortNetwork,
        services: u64,
        last_online: SystemTime,
    ) {
        self.trim();
        if self.known.contains_key(&peer) {
            return;
        }

        self.known.insert(peer.clone(), ());
        self.peers[self.next_write] = AddressPortNetworkWithServicesAndTime {
            apn: peer,
            services,
            last_online,
        };

        self.next_write = (self.next_write + 1) % self.peers.len();

        todo!()
    }
}
