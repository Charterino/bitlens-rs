use axum::{
    Router,
    http::{HeaderValue, Method},
    routing::{any, get},
};
use serde::Deserialize;
use tower_http::cors::CorsLayer;

mod address;
mod block;
mod frontpage;
mod peer;
mod tx;

pub async fn start() {
    let app = Router::new()
        .route("/api/frontpagedata", get(frontpage::frontpagedata))
        .route("/api/tx", get(tx::txdata))
        .route("/api/block", get(block::blockdata))
        .route("/api/address/top", get(address::address_data_top))
        .route("/api/address/extra", get(address::address_data_extra))
        .route("/api/peers", get(peer::current_peers))
        .route("/api/getpeer", get(peer::get_peer))
        .route("/api/socket/frontpage", any(frontpage::frontpagesocket))
        .layer(
            CorsLayer::new()
                .allow_origin("*".parse::<HeaderValue>().unwrap())
                .allow_methods([Method::GET]),
        );

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8122").await.unwrap();
    tokio::spawn(async { axum::serve(listener, app).await });
}

#[derive(Debug, Deserialize)]
struct HashParam {
    hash: String,
}

#[derive(Debug, Deserialize)]
struct AddressTopParam {
    address: String,
}

#[derive(Debug, Deserialize)]
struct AddressExtraParam {
    address: String,
    from_timestamp: Option<u64>,
    to_timestamp: Option<u64>,
    limit: Option<usize>,
}
