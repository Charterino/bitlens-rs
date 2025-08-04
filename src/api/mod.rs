use axum::{
    Json, Router,
    extract::Query,
    http::{HeaderValue, Method, StatusCode},
    response::IntoResponse,
    routing::get,
};
use serde::Deserialize;
use tower_http::cors::CorsLayer;

use crate::{chainman::FRONTPAGE_STATS, db, tx::AnalyzedTx};

pub async fn start() {
    let app = Router::new()
        .route("/api/frontpagedata", get(frontpagedata))
        .route("/api/tx", get(txdata))
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
struct TxByHashParams {
    hash: String,
}

async fn txdata(Query(params): Query<TxByHashParams>) -> Result<Json<AnalyzedTx>, StatusCode> {
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

    Ok(Json(tx))
}
