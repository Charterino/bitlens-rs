use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Params;

use crate::types::addressportnetwork::AddressPortNetwork;

use super::batch::Request;

pub struct InsertPeerRequest {
    pub apn: AddressPortNetwork,
    pub first_seen: u64,
}

impl Request for InsertPeerRequest {
    fn into_params(self) -> impl Params {
        (
            self.apn.network_id,
            self.apn.address,
            self.apn.port,
            self.first_seen,
        )
    }
}

pub struct BanPeerRequest {
    pub apn: AddressPortNetwork,
    pub banned_at: SystemTime,
    pub banned_until: SystemTime,
}

impl Request for BanPeerRequest {
    fn into_params(self) -> impl Params {
        (
            self.apn.network_id,
            self.apn.address,
            self.apn.port,
            self.banned_at.duration_since(UNIX_EPOCH).unwrap().as_secs(),
            self.banned_until
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        )
    }
}

pub struct DeletePeerRequest {
    pub apn: AddressPortNetwork,
}

impl Request for DeletePeerRequest {
    fn into_params(self) -> impl Params {
        (self.apn.address, self.apn.port)
    }
}

pub struct UpdatePeerBlockHeightRequest {
    pub apn: AddressPortNetwork,
    pub new_height: u32,
}

impl Request for UpdatePeerBlockHeightRequest {
    fn into_params(self) -> impl Params {
        (self.new_height, self.apn.address, self.apn.port)
    }
}

pub struct UpdatePeerFromVersionRequest {
    pub apn: AddressPortNetwork,
    pub services: u64,
    pub block_height: u32,
    pub user_agent: Vec<u8>,
}

impl Request for UpdatePeerFromVersionRequest {
    fn into_params(self) -> impl Params {
        (
            self.services,
            self.block_height,
            self.user_agent,
            self.apn.address,
            self.apn.port,
        )
    }
}

pub struct InsertHeaderRequest {
    pub parent: [u8; 32],
    pub merkle_root: [u8; 32],
    pub timestamp: u32,
    pub bits: u32,
    pub nonce: u32,
    pub version: u32,
    pub number: u64,
    pub hash: [u8; 32],
}

impl Request for InsertHeaderRequest {
    fn into_params(self) -> impl Params {
        (
            self.version,
            self.parent,
            self.merkle_root,
            self.timestamp,
            self.bits,
            self.nonce,
            self.number,
            self.hash,
        )
    }
}
