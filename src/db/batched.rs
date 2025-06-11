use deadpool_sqlite::rusqlite::Params;
use tokio::sync::mpsc::{Sender, channel};

use crate::types::addressportnetwork::AddressPortNetwork;

use super::batch::{Request, batch_process_requests};

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

pub fn start_insert_peer_batcher() -> Sender<InsertPeerRequest> {
    let (sender, receiver) = channel(1);
    tokio::spawn(batch_process_requests(
        "INSERT INTO peers (network_id, address, port, first_seen, services) VALUES (?, ?, ?, ?, 0)",
        receiver,
    ));
    sender
}
