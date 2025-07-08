use axum::{Router, http::StatusCode, response::IntoResponse, routing::get};

use crate::chainman::FRONTPAGE_STATS;

pub async fn start() {
    let app = Router::new().route("/api/frontpagedata", get(frontpagedata));

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
