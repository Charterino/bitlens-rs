use crate::{
    api::HashParam,
    chainman,
    db::{self, rocksdb::BlockTxEntry},
    types::blockheaderwithnumber::BlockHeaderWithNumber,
};
use axum::{Json, extract::Query, http::StatusCode};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct BlockDataResponse {
    #[serde(flatten)]
    pub header: BlockHeaderWithNumber,
    pub txs: Vec<BlockTxEntry>,
}

pub async fn blockdata(
    Query(params): Query<HashParam>,
) -> Result<Json<BlockDataResponse>, StatusCode> {
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
