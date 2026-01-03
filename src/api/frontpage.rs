use crate::chainman::{FRONTPAGE_STATS, FRONTPAGE_UPDATE_BROADCAST};
use axum::{
    extract::{
        WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::StatusCode,
    response::IntoResponse,
};

pub async fn frontpagedata() -> impl IntoResponse {
    let r = FRONTPAGE_STATS.read().unwrap();
    (
        StatusCode::OK,
        [("content-type", "application/json")],
        r.serialized.clone(),
    )
}

pub async fn frontpagesocket(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_frontpage_socket)
}

async fn handle_frontpage_socket(mut socket: WebSocket) {
    let mut subscriber = FRONTPAGE_UPDATE_BROADCAST.subscribe();
    while let Ok(d) = subscriber.recv().await {
        if let Err(_) = socket.send(Message::text(d)).await {
            break;
        }
    }
}
