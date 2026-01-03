use crate::{
    crawler,
    db::{self, sqlite::PeerData},
    packets::network_id::NetworkId,
    types::addressportnetwork::AddressPortNetwork,
};
use axum::{Json, extract::Query, http::StatusCode};
use serde::{Deserialize, Serialize};
use std::net::{Ipv4Addr, Ipv6Addr};

pub async fn current_peers() -> Result<Json<Vec<String>>, StatusCode> {
    let all_peers = crawler::peertracker::get_all_connected_peers();
    Ok(Json(all_peers))
}

#[derive(Debug, Deserialize)]
pub struct PeerParams {
    pub address: String,
    pub port: u16,
}

#[derive(Serialize, Deserialize)]
pub struct PeerResponse {
    is_connected_now: bool,
    #[serde(flatten)]
    peer_data: Option<PeerData>,
}

pub async fn get_peer(Query(params): Query<PeerParams>) -> Result<Json<PeerResponse>, StatusCode> {
    let apn = match parse_peer_address(&params.address, params.port) {
        Some(v) => v,
        None => return Err(StatusCode::BAD_REQUEST),
    };

    let is_connected_now = crawler::peertracker::is_connected_now(&apn);
    let data = db::sqlite::get_peer(&apn)
        .await
        .map(Some)
        .unwrap_or_default();

    Ok(Json(PeerResponse {
        is_connected_now,
        peer_data: data,
    }))
}

fn parse_peer_address(address: &str, port: u16) -> Option<AddressPortNetwork> {
    if let Ok(v) = address.parse::<Ipv4Addr>() {
        return Some(AddressPortNetwork {
            network_id: NetworkId::IPv4,
            port,
            address: v.octets().into(),
        });
    }
    if let Ok(v) = address.parse::<Ipv6Addr>() {
        return Some(AddressPortNetwork {
            network_id: NetworkId::IPv6,
            port,
            address: v.octets().into(),
        });
    }

    None
}
