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
mod miner;
mod peer;
mod search;
mod tx;

pub async fn start() {
    let app = Router::new()
        .route("/api/frontpagedata", get(frontpage::frontpagedata))
        .route("/api/tx", get(tx::txdata))
        .route("/api/block/top", get(block::block_data_top))
        .route("/api/block/extra", get(block::block_data_extra))
        .route("/api/address/top", get(address::address_data_top))
        .route("/api/address/extra", get(address::address_data_extra))
        .route("/api/address/continue", get(address::address_data_continue))
        .route("/api/peers", get(peer::current_peers))
        .route("/api/getpeer", get(peer::get_peer))
        .route("/api/search", get(search::search))
        .route("/api/socket/frontpage", any(frontpage::frontpagesocket))
        .route("/api/miner/page", get(miner::get_miners_page))
        .layer(
            CorsLayer::new()
                .allow_origin("*".parse::<HeaderValue>().unwrap())
                .allow_methods([Method::GET]),
        );

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8122")
        .await
        .expect("to start http server");
    tokio::spawn(async { axum::serve(listener, app).await });
}

#[derive(Debug, Deserialize)]
struct SearchParam {
    term: String,
}

#[derive(Debug, Deserialize)]
struct HashParam {
    hash: String,
}

#[derive(Debug, Deserialize)]
struct HashTopParam {
    hash: String,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct HashExtraParam {
    hash: String,
    skip: usize,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct AddressTopParam {
    address: String,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddressExtraParam {
    address: String,
    from_timestamp: Option<u64>,
    to_timestamp: Option<u64>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddressContinueParam {
    address: String,
    after_tx: String,
    after_timestamp: u64,
    limit: Option<usize>,
}
