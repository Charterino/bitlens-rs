use axum::{Json, extract::Query, http::StatusCode, response::IntoResponse};
use slog_scope::warn;

use crate::{api::AfterHashParam, chainman, miners};

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
