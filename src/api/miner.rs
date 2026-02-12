use axum::{http::StatusCode, response::IntoResponse};

use crate::miners;

pub async fn get_miners_page() -> impl IntoResponse {
    (
        StatusCode::OK,
        [("content-type", "application/json")],
        miners::get_page_data(),
    )
}
