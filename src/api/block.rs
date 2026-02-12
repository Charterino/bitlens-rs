use crate::{
    api::{HashExtraParam, HashTopParam},
    chainman,
    db::{self, rocksdb::BlockTxEntry},
    miners::{self, MinerId},
    types::blockheaderwithnumber::BlockHeaderWithNumber,
};
use axum::{Json, extract::Query, http::StatusCode};
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockDataResponse {
    #[serde(flatten)]
    pub header: BlockHeaderWithNumber,
    pub total_txs_count: usize,
    pub txs: Vec<BlockTxEntry>,
    pub miner_id: MinerId,
    pub miner_name: String,
    pub recent_miner_share: f64,
}

pub async fn block_data_top(
    Query(params): Query<HashTopParam>,
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

    let limit = params.limit.unwrap_or(50);

    let txs = db::rocksdb::get_block_tx_entries(unhexxed)
        .await
        .unwrap_or_default();
    let total_txs_count = txs.len();
    let txs = txs.into_iter().take(limit).collect();
    let (miner_id, miner_name, recent_miner_share) =
        miners::get_miner_for_block_and_share(header.header.hash).unwrap_or_default();

    Ok(Json(BlockDataResponse {
        header,
        txs,
        miner_id,
        miner_name,
        recent_miner_share,
        total_txs_count,
    }))
}

pub async fn block_data_extra(
    Query(params): Query<HashExtraParam>,
) -> Result<Json<Vec<BlockTxEntry>>, StatusCode> {
    let mut unhexxed = match hex::decode(params.hash) {
        Err(_) => return Err(StatusCode::BAD_REQUEST),
        Ok(v) => v,
    };
    if unhexxed.len() != 32 {
        return Err(StatusCode::BAD_REQUEST);
    }
    unhexxed.reverse();
    let unhexxed: [u8; 32] = unhexxed.try_into().unwrap();

    let limit = params.limit.unwrap_or(50);

    let txs = db::rocksdb::get_block_tx_entries(unhexxed)
        .await
        .unwrap_or_default()
        .into_iter()
        .skip(params.skip)
        .take(limit)
        .collect();

    Ok(Json(txs))
}
