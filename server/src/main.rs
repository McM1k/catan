use axum::{
    extract::{
        ws::{Message, WebSocket},
        State, WebSocketUpgrade,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use engine::protocol::{ClientMsg, LobbyPlayer, ServerMsg};
use engine::{Game, Rules};
use futures_util::{SinkExt, StreamExt};
use rand::{seq::SliceRandom, Rng};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::mpsc::{self, UnboundedSender};
use tower_http::services::{ServeDir, ServeFile};

const MAX_PLAYERS: usize = 4;
const MIN_PLAYERS: usize = 2;
const MAX_NAME_LEN: usize = 20;
const ABANDONED_ROOM_TTL: Duration = Duration::from_secs(30 * 60);

type Tx = UnboundedSender<ServerMsg>;
type Shared = Arc<Mutex<Rooms>>;

#[derive(Default)]
struct Rooms {
    map: HashMap<String, Room>,
}

struct Room {
    code: String,
    seats: Vec<Seat>,
    game: Option<Game>,
}

struct Seat {
    name: String,
    token: String,
    tx: Option<Tx>,
}

/// Which seat this connection controls.
struct Ident {
    room: String,
    token: String,
}

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
    println!("Colonists listening on http://{addr} (static files from {static_dir})");
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

    disconnect(&state, &tx, ident);
    writer.abort();
}

fn random_string(len: usize, alphabet: &[u8]) -> String {
    let mut rng = rand::thread_rng();
    (0..len)
        .map(|_| alphabet[rng.gen_range(0..alphabet.len())] as char)
        .collect()
}

fn clean_name(name: &str) -> Result<String, String> {
    let name: String = name.trim().chars().take(MAX_NAME_LEN).collect();
    if name.is_empty() {
        Err("Please enter a name.".into())
    } else {
        Ok(name)
    }
}

/// Handles one client message. Returns an error string to send back, if any.
fn process(state: &Shared, tx: &Tx, ident: &mut Option<Ident>, msg: ClientMsg) -> Option<String> {
    let mut rooms = state.lock().unwrap();
    match msg {
        ClientMsg::Create { name } => {
            if ident.is_some() {
                return Some("You are already in a room.".into());
            }
            let name = match clean_name(&name) {
                Ok(n) => n,
                Err(e) => return Some(e),
            };
            let code = loop {
                let c = random_string(4, b"ABCDEFGHJKLMNPQRSTUVWXYZ");
                if !rooms.map.contains_key(&c) {
                    break c;
                }
            };
            let token = random_string(24, b"abcdefghijklmnopqrstuvwxyz0123456789");
            rooms.map.insert(
                code.clone(),
                Room {
                    code: code.clone(),
                    seats: vec![Seat {
                        name,
                        token: token.clone(),
                        tx: Some(tx.clone()),
                    }],
                    game: None,
                },
            );
            let _ = tx.send(ServerMsg::Joined { room: code.clone(), token: token.clone() });
            broadcast(&rooms.map[&code]);
            *ident = Some(Ident { room: code, token });
            None
        }
        ClientMsg::Join { room, name, token } => {
            if ident.is_some() {
                return Some("You are already in a room.".into());
            }
            let code = room.trim().to_uppercase();
            let Some(r) = rooms.map.get_mut(&code) else {
                return Some("No such room.".into());
            };
            // Reconnect with a known token.
            if let Some(tok) = token {
                if let Some(seat) = r.seats.iter_mut().find(|s| s.token == tok) {
                    seat.tx = Some(tx.clone());
                    let _ = tx.send(ServerMsg::Joined { room: code.clone(), token: tok.clone() });
                    broadcast(r);
                    *ident = Some(Ident { room: code, token: tok });
                    return None;
                }
            }
            if r.game.is_some() {
                return Some("That game has already started.".into());
            }
            if r.seats.len() >= MAX_PLAYERS {
                return Some("That room is full.".into());
            }
            let name = match clean_name(&name) {
                Ok(n) => n,
                Err(e) => return Some(e),
            };
            if r.seats.iter().any(|s| s.name.eq_ignore_ascii_case(&name)) {
                return Some("That name is taken in this room.".into());
            }
            let tok = random_string(24, b"abcdefghijklmnopqrstuvwxyz0123456789");
            r.seats.push(Seat {
                name,
                token: tok.clone(),
                tx: Some(tx.clone()),
            });
            let _ = tx.send(ServerMsg::Joined { room: code.clone(), token: tok.clone() });
            broadcast(r);
            *ident = Some(Ident { room: code, token: tok });
            None
        }
        ClientMsg::Start => {
            let id = ident.as_ref()?;
            let r = rooms.map.get_mut(&id.room)?;
            if r.game.is_some() {
                return Some("The game has already started.".into());
            }
            if r.seats.first().map(|s| &s.token) != Some(&id.token) {
                return Some("Only the host can start the game.".into());
            }
            if r.seats.len() < MIN_PLAYERS {
                return Some("You need at least two players.".into());
            }
            let mut rng = rand::thread_rng();
            r.seats.shuffle(&mut rng); // random turn order
            let names = r.seats.iter().map(|s| s.name.clone()).collect();
            r.game = Some(Game::new(Rules::default(), names, &mut rng));
            broadcast(r);
            None
        }
        ClientMsg::Leave => {
            let id = ident.take()?;
            leave(&mut rooms, &id, tx, true);
            None
        }
        ClientMsg::Act(action) => {
            let id = ident.as_ref()?;
            let r = rooms.map.get_mut(&id.room)?;
            let Some(game) = r.game.as_mut() else {
                return Some("The game hasn't started yet.".into());
            };
            let seat = r.seats.iter().position(|s| s.token == id.token)?;
            let mut rng = rand::thread_rng();
            match game.apply(seat, action, &mut rng) {
                Ok(()) => {
                    broadcast(r);
                    None
                }
                Err(e) => Some(e),
            }
        }
    }
}

/// Sends every connected seat its own view of the room.
fn broadcast(room: &Room) {
    let connected: Vec<bool> = room.seats.iter().map(|s| s.tx.is_some()).collect();
    for (i, seat) in room.seats.iter().enumerate() {
        let Some(tx) = &seat.tx else { continue };
        let msg = match &room.game {
            Some(g) => ServerMsg::State {
                view: Box::new(g.view_for(Some(i))),
                connected: connected.clone(),
            },
            None => ServerMsg::Lobby {
                room: room.code.clone(),
                players: room
                    .seats
                    .iter()
                    .map(|s| LobbyPlayer {
                        name: s.name.clone(),
                        connected: s.tx.is_some(),
                    })
                    .collect(),
                you: i,
                is_host: i == 0,
            },
        };
        let _ = tx.send(msg);
    }
}

fn leave(rooms: &mut Rooms, id: &Ident, tx: &Tx, explicit: bool) {
    let Some(r) = rooms.map.get_mut(&id.room) else { return };
    if r.game.is_none() {
        // Lobby: free the seat.
        r.seats.retain(|s| s.token != id.token);
        if r.seats.is_empty() {
            rooms.map.remove(&id.room);
            return;
        }
    } else if let Some(seat) = r.seats.iter_mut().find(|s| s.token == id.token) {
        // In a running game the seat is kept so the player can come back,
        // unless the connection was already replaced by a reconnect.
        if explicit || seat.tx.as_ref().is_some_and(|t| t.same_channel(tx)) {
            seat.tx = None;
        }
    }
    let r = &rooms.map[&id.room];
    broadcast(r);
}

fn disconnect(state: &Shared, tx: &Tx, ident: Option<Ident>) {
    let Some(id) = ident else { return };
    {
        let mut rooms = state.lock().unwrap();
        // Ignore stale disconnects from a connection that was replaced.
        if let Some(r) = rooms.map.get(&id.room) {
            let current = r
                .seats
                .iter()
                .find(|s| s.token == id.token)
                .and_then(|s| s.tx.as_ref())
                .is_some_and(|t| t.same_channel(tx));
            if !current {
                return;
            }
        }
        leave(&mut rooms, &id, tx, false);
    }
    // Garbage-collect rooms nobody comes back to.
    let state = state.clone();
    tokio::spawn(async move {
        tokio::time::sleep(ABANDONED_ROOM_TTL).await;
        let mut rooms = state.lock().unwrap();
        let abandoned = rooms
            .map
            .get(&id.room)
            .is_some_and(|r| r.seats.iter().all(|s| s.tx.is_none()));
        if abandoned {
            rooms.map.remove(&id.room);
        }
    });
}
