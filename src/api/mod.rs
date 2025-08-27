use axum::{
    Json, Router,
    extract::Query,
    http::{HeaderValue, Method, StatusCode},
    response::IntoResponse,
    routing::get,
};
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;

use crate::{
    chainman::{self, FRONTPAGE_STATS},
    db::{self, rocksdb::BlockTxEntry},
    tx::AnalyzedTx,
    types::blockheaderwithnumber::BlockHeaderWithNumber,
};

pub async fn start() {
    let app = Router::new()
        .route("/api/frontpagedata", get(frontpagedata))
        .route("/api/tx", get(txdata))
        .route("/api/block", get(blockdata))
        .layer(
            CorsLayer::new()
                .allow_origin("*".parse::<HeaderValue>().unwrap())
                .allow_methods([Method::GET]),
        );

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8122").await.unwrap();
    tokio::spawn(async { axum::serve(listener, app).await });
}

async fn frontpagedata() -> impl IntoResponse {
    let r = FRONTPAGE_STATS.read().unwrap();
    (
        StatusCode::OK,
        [("content-type", "application/json")],
        r.serialized.clone(),
    )
}

#[derive(Debug, Deserialize)]
struct HashParam {
    hash: String,
}

#[derive(Serialize, Deserialize)]
pub struct TxDataResponse {
    pub header: BlockHeaderWithNumber,
    #[serde(flatten)]
    pub tx: AnalyzedTx,
    #[serde(serialize_with = "crate::util::serialize_spends::serialize_spends")]
    pub spends: Vec<(u32, [u8; 32])>,
}

async fn txdata(Query(params): Query<HashParam>) -> Result<Json<TxDataResponse>, StatusCode> {
    let mut unhexxed = match hex::decode(params.hash) {
        Err(_) => return Err(StatusCode::BAD_REQUEST),
        Ok(v) => v,
    };
    if unhexxed.len() != 32 {
        return Err(StatusCode::BAD_REQUEST);
    }
    unhexxed.reverse();
    let unhexxed: [u8; 32] = unhexxed.try_into().unwrap();
    let mut tx = match db::rocksdb::get_analyzed_tx(unhexxed).await {
        Err(_) => return Err(StatusCode::NOT_FOUND),
        Ok(v) => v,
    };

    let txouts = match db::rocksdb::get_transaction_outputs(unhexxed).await {
        Err(_) => return Err(StatusCode::NOT_FOUND),
        Ok(v) => v,
    };

    tx.tx.txouts = txouts;
    tx.tx.compute_witness_hash();

    let header = match chainman::get_header_by_hash(tx.block_hash) {
        Some(h) => h,
        None => return Err(StatusCode::NOT_FOUND),
    };

    let spends = match db::rocksdb::get_tx_spends(unhexxed).await {
        Ok(v) => v,
        Err(_) => return Err(StatusCode::NOT_FOUND),
    };

    let filtered_spends = chainman::filter_tx_spends(spends);

    Ok(Json(TxDataResponse {
        header: header,
        tx: tx,
        spends: filtered_spends,
    }))
}

#[derive(Serialize, Deserialize)]
pub struct BlockDataResponse {
    #[serde(flatten)]
    pub header: BlockHeaderWithNumber,
    pub txs: Vec<BlockTxEntry>,
}

async fn blockdata(Query(params): Query<HashParam>) -> Result<Json<BlockDataResponse>, StatusCode> {
    let mut unhexxed = match hex::decode(params.hash) {
        Err(_) => return Err(StatusCode::BAD_REQUEST),
        Ok(v) => v,
    };
    if unhexxed.len() != 32 {
        return Err(StatusCode::BAD_REQUEST);
    }
    unhexxed.reverse();
    let unhexxed: [u8; 32] = unhexxed.try_into().unwrap();
    let header = match chainman::get_header_by_hash(unhexxed) {
        None => return Err(StatusCode::NOT_FOUND),
        Some(v) => v,
    };

    let txs = db::rocksdb::get_block_tx_entries(unhexxed)
        .await
        .unwrap_or_default();

    Ok(Json(BlockDataResponse { header, txs }))
}
