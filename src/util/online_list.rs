use std::{
    collections::HashMap,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use rand::Rng;

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
    end: usize,
    start: usize,
    known: HashMap<AddressPortNetwork, ()>,
}

impl OnlineList {
    pub fn new(threshold: Duration) -> Self {
        Self {
            peers: vec![AddressPortNetworkWithServicesAndTime::default(); 32],
            threshold,
            end: 0,
            start: 0,
            known: HashMap::new(),
        }
    }

    fn length(&self) -> usize {
        if self.end < self.start {
            return self.end + self.peers.len() - self.start;
        }
        self.end - self.start
    }

    fn trim(&mut self) {
        let start = self.start;
        let rn = SystemTime::now();
        for i in 0..self.peers.len() {
            let real_i = (i + start) % self.peers.len();
            let item = self.peers.get(real_i).unwrap();
            if item.last_online < rn.checked_sub(self.threshold).unwrap() {
                self.start = (self.start + 1) % self.peers.len();
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
        self.peers[self.end] = AddressPortNetworkWithServicesAndTime {
            apn: peer,
            services,
            last_online,
        };

        self.end = (self.end + 1) % self.peers.len();

        if self.end == self.start {
            // Hit max size, need to grow
            let old_size = self.peers.len();
            let mut new_peers = Vec::with_capacity(old_size * 2);
            // So from 0 to index we have the newer peers, and from index to length we have the older peers;
            new_peers.extend_from_slice(self.peers.get(self.end..).unwrap());
            new_peers.extend_from_slice(self.peers.get(0..self.end).unwrap());
            for _ in 0..(new_peers.capacity() - new_peers.len()) {
                new_peers.push(AddressPortNetworkWithServicesAndTime::default());
            }
            self.peers = new_peers;
            self.start = 0;
            self.end = old_size;
            println!("doubling cap of online_list to {}", old_size * 2);
        }
    }

    pub fn random(&self, services: Option<u64>) -> Option<AddressPortNetwork> {
        let len = self.length();
        if len == 0 {
            return None;
        }
        let index = rand::rng().random_range(0..len);
        match services {
            None => Some(self.peers[index].apn.clone()),
            Some(requirement) => {
                for i in index..index + len {
                    let real_i = i % self.peers.len();
                    if self.peers[real_i].services & requirement == requirement {
                        return Some(self.peers[real_i].apn.clone());
                    }
                }
                None
            }
        }
    }
}
