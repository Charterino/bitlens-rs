use std::sync::Arc;

use axum::{Json, extract::Query, http::StatusCode, response::IntoResponse};
use serde::Serialize;
use slog_scope::warn;

use crate::{
    api::{AfterHashParam, MinerContinueParam, MinerTopParam},
    chainman,
    miners::{self, RecentBlock, config::Miner},
};

pub async fn get_miners_page_top() -> impl IntoResponse {
    (
        StatusCode::OK,
        [("content-type", "application/json")],
        miners::get_page_data(),
    )
}

pub async fn get_miners_page_continue(Query(params): Query<AfterHashParam>) -> impl IntoResponse {
    let mut unhexxed = match hex::decode(&params.hash) {
        Err(_) => return Err(StatusCode::BAD_REQUEST),
        Ok(v) => v,
    };
    if unhexxed.len() != 32 {
        return Err(StatusCode::BAD_REQUEST);
    }
    unhexxed.reverse();
    let unhexxed: [u8; 32] = unhexxed.try_into().unwrap();

    let limit = params.limit.unwrap_or(50);
    if limit > 1000 {
        return Err(StatusCode::BAD_REQUEST);
    }

    let parents = chainman::get_parents(unhexxed, limit);

    match miners::get_block_data(&parents) {
        Ok(v) => Ok(Json(v)),
        Err(e) => {
            warn!("failed to get miner block data data"; "error" => e.to_string(), "for_hash" => &params.hash);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MinerDataTopResponse {
    #[serde(flatten)]
    miner: Miner,
    recent_blocks: Vec<RecentBlock>,
}

pub async fn get_miner_data_top(Query(params): Query<MinerTopParam>) -> impl IntoResponse {
    let limit = params.limit.unwrap_or(50);
    if limit > 1000 {
        return Err(StatusCode::BAD_REQUEST);
    }

    let miner_id = Arc::new(params.id);
    let miner = match miners::get_miner(&miner_id) {
        Some(v) => v,
        None => return Err(StatusCode::NOT_FOUND),
    };

    let recent_blocks = miners::get_miner_blocks(&miner_id, limit, None);

    Ok(Json(MinerDataTopResponse {
        miner,
        recent_blocks,
    }))
}

pub async fn get_miner_data_continue(
    Query(params): Query<MinerContinueParam>,
) -> impl IntoResponse {
    let limit = params.limit.unwrap_or(50);
    if limit > 1000 {
        return Err(StatusCode::BAD_REQUEST);
    }

    let mut unhexxed = match hex::decode(&params.after_hash) {
        Err(_) => return Err(StatusCode::BAD_REQUEST),
        Ok(v) => v,
    };
    if unhexxed.len() != 32 {
        return Err(StatusCode::BAD_REQUEST);
    }
    unhexxed.reverse();
    let unhexxed: [u8; 32] = unhexxed.try_into().unwrap();

    let miner_id = Arc::new(params.id);
    let miner = match miners::get_miner(&miner_id) {
        Some(v) => v,
        None => return Err(StatusCode::NOT_FOUND),
    };

    let recent_blocks = miners::get_miner_blocks(&miner_id, limit, Some(unhexxed));

    Ok(Json(recent_blocks))
}
