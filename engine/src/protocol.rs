//! Messages exchanged over the WebSocket between client and server.

use crate::{Action, GameView};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ClientMsg {
    /// Open a new room and sit down in it.
    Create { name: String },
    /// Join (or, with a token, rejoin) a room.
    Join {
        room: String,
        name: String,
        token: Option<String>,
    },
    /// Host only: start the game once at least two players are seated.
    Start,
    /// Leave the room for good (in a lobby this frees the seat).
    Leave,
    Act(Action),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LobbyPlayer {
    pub name: String,
    pub connected: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ServerMsg {
    /// Sent once after a successful Create/Join. Keep `token` to reconnect.
    Joined { room: String, token: String },
    Lobby {
        room: String,
        players: Vec<LobbyPlayer>,
        you: usize,
        is_host: bool,
    },
    State {
        view: Box<GameView>,
        connected: Vec<bool>,
    },
    Error(String),
}
