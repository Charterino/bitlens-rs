use crate::chainman::FRONTPAGE_STATS;
use axum::{http::StatusCode, response::IntoResponse};

pub async fn frontpagedata() -> impl IntoResponse {
    let r = FRONTPAGE_STATS.read().unwrap();
    (
        StatusCode::OK,
        [("content-type", "application/json")],
        r.serialized.clone(),
    )
}

pub async fn frontpagesocket() {
    todo!()
}
