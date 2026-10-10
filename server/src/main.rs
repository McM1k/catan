//! Web server for the games: serves the client and runs every room over a
//! WebSocket. All room logic lives in `rooms`; this file is only plumbing.

mod rooms;

use axum::{
    extract::{
        ws::{Message, WebSocket},
        State, WebSocketUpgrade,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use futures_util::{SinkExt, StreamExt};
use protocol::{ClientMsg, ServerMsg};
use rooms::{disconnect, free_dropped_seat, process, remove_if_abandoned, Ident, Rooms, Shared};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::mpsc;
use tower_http::services::{ServeDir, ServeFile};

/// How long an emptied room is kept around for players to come back to.
const ABANDONED_ROOM_TTL: Duration = Duration::from_secs(30 * 60);

/// How long a lobby seat waits for a player whose connection dropped (a
/// reload, a phone switching apps) before it is freed for someone else.
const LOBBY_GRACE: Duration = Duration::from_secs(2 * 60);

#[tokio::main]
async fn main() {
    let state: Shared = Arc::new(Mutex::new(Rooms::default()));
    let static_dir = std::env::var("STATIC_DIR").unwrap_or_else(|_| "client/dist".into());
    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".into());

    let files = ServeDir::new(&static_dir).fallback(ServeFile::new(format!("{static_dir}/index.html")));
    let app = Router::new()
        .route("/ws", get(ws_handler))
        .fallback_service(files)
        .with_state(state);

    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind");
    println!("Games server listening on http://{addr} (static files from {static_dir})");
    axum::serve(listener, app).await.expect("serve");
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<Shared>) -> impl IntoResponse {
    ws.max_message_size(16 * 1024)
        .on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: Shared) {
    let (mut sink, mut stream) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<ServerMsg>();

    let writer = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            let Ok(text) = serde_json::to_string(&msg) else { continue };
            if sink.send(Message::Text(text.into())).await.is_err() {
                break;
            }
        }
    });

    let mut ident: Option<Ident> = None;
    while let Some(Ok(msg)) = stream.next().await {
        let text = match msg {
            Message::Text(t) => t,
            Message::Close(_) => break,
            _ => continue,
        };
        match serde_json::from_str::<ClientMsg>(text.as_str()) {
            Ok(cm) => {
                if let Some(e) = process(&state, &tx, &mut ident, cm) {
                    let _ = tx.send(ServerMsg::Error(e));
                }
            }
            Err(_) => {
                let _ = tx.send(ServerMsg::Error("Malformed message.".into()));
            }
        }
    }

    if let Some(id) = disconnect(&state, &tx, ident) {
        tokio::spawn(async move {
            // Free a lobby seat its player hasn't come back to...
            tokio::time::sleep(LOBBY_GRACE).await;
            free_dropped_seat(&state, &id, LOBBY_GRACE);
            // ...and garbage-collect a room nobody comes back to.
            tokio::time::sleep(ABANDONED_ROOM_TTL - LOBBY_GRACE).await;
            remove_if_abandoned(&state, &id.room);
        });
    }
    writer.abort();
}
