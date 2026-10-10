//! Rooms: one join-code system for every game.
//!
//! A player creates a room for a game and gets a 4-letter code; the others
//! join with that code, the host (first seat) starts the game, and a
//! per-seat token lets a player come back after a dropped connection. The
//! room's code decides which game you end up in. Only what happens once
//! the game is running differs: each game keeps its own state and sends
//! every seat its own redacted view.
//!
//! Everything here is synchronous and holds the lock only briefly, so it
//! can be tested without sockets: a "connection" is just a channel.

use engine::{Game, Rules};
use hanabi_core::{ActionError, GameRules, GameState, PlayerId};
use protocol::{ClientMsg, GameKind, LobbyPlayer, ServerMsg};
use rand::{seq::SliceRandom, Rng};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::mpsc::UnboundedSender;

const MAX_NAME_LEN: usize = 20;

pub type Tx = UnboundedSender<ServerMsg>;
pub type Shared = Arc<Mutex<Rooms>>;

#[derive(Default)]
pub struct Rooms {
    pub map: HashMap<String, Room>,
}

pub struct Room {
    pub code: String,
    pub kind: GameKind,
    pub seats: Vec<Seat>,
    /// Hanabi variant rules picked in the lobby (unused by other games).
    pub rules: GameRules,
    pub game: Option<Running>,
}

/// The game in progress. Each variant is that game's authoritative state.
pub enum Running {
    Catan(Game),
    Hanabi(GameState),
    Wonderful(wonderful_core::State),
}

pub struct Seat {
    pub name: String,
    pub token: String,
    /// `None` while the player is disconnected.
    pub tx: Option<Tx>,
    /// When the connection last dropped (only meaningful while `tx` is `None`).
    pub dropped_at: Option<Instant>,
}

/// Which seat a connection controls.
pub struct Ident {
    pub room: String,
    pub token: String,
}

impl Room {
    fn is_host(&self, token: &str) -> bool {
        self.seats.first().is_some_and(|s| s.token == token)
    }

    fn lobby_players(&self) -> Vec<LobbyPlayer> {
        self.seats
            .iter()
            .map(|s| LobbyPlayer {
                name: s.name.clone(),
                connected: s.tx.is_some(),
            })
            .collect()
    }
}

fn random_string(len: usize, alphabet: &[u8]) -> String {
    let mut rng = rand::thread_rng();
    (0..len)
        .map(|_| alphabet[rng.gen_range(0..alphabet.len())] as char)
        .collect()
}

fn new_token() -> String {
    random_string(24, b"abcdefghijklmnopqrstuvwxyz0123456789")
}

fn clean_name(name: &str) -> Result<String, String> {
    let name: String = name.trim().chars().take(MAX_NAME_LEN).collect();
    if name.is_empty() {
        Err("Please enter a name.".into())
    } else {
        Ok(name)
    }
}

/// What a rejected Hanabi move means, in words for the player.
fn describe_hanabi_error(e: ActionError) -> String {
    match e {
        ActionError::NotYourTurn => "It's not your turn.",
        ActionError::GameOver => "The game is over.",
        ActionError::NoClueTokens => "No clue tokens left: play or discard a card instead.",
        ActionError::CannotClueSelf => "You can't give a clue to yourself.",
        ActionError::UnknownPlayer => "There's no such player.",
        ActionError::CannotClueMulticolor => "The multicolor suit can't be clued directly.",
        ActionError::CannotClueBlack => "The black suit has no color to clue.",
        ActionError::CannotClueSecondaryColor => {
            "In Hanabii mode only red, yellow and blue can be clued."
        }
        ActionError::ClueMatchesNothing => "That clue doesn't touch any card in their hand.",
        ActionError::CardNotInHand => "That card isn't in your hand.",
        ActionError::CannotDiscardAtMaxClues => {
            "All 8 clue tokens are available: play a card or give a clue instead."
        }
    }
    .into()
}

/// Why a game move can't be applied to this room right now.
fn not_running(room: &Room, wanted: GameKind) -> String {
    if room.kind != wanted {
        "That move belongs to a different game.".into()
    } else {
        "The game hasn't started yet.".into()
    }
}

/// Handles one client message. Returns an error string to send back, if any.
pub fn process(state: &Shared, tx: &Tx, ident: &mut Option<Ident>, msg: ClientMsg) -> Option<String> {
    let mut rooms = state.lock().unwrap();
    match msg {
        ClientMsg::Create { game, name } => {
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
            let token = new_token();
            rooms.map.insert(
                code.clone(),
                Room {
                    code: code.clone(),
                    kind: game,
                    seats: vec![Seat {
                        name,
                        token: token.clone(),
                        tx: Some(tx.clone()),
                        dropped_at: None,
                    }],
                    rules: GameRules::default(),
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
            if r.seats.len() >= r.kind.max_players() {
                return Some("That room is full.".into());
            }
            let name = match clean_name(&name) {
                Ok(n) => n,
                Err(e) => return Some(e),
            };
            if r.seats.iter().any(|s| s.name.eq_ignore_ascii_case(&name)) {
                return Some("That name is taken in this room.".into());
            }
            let tok = new_token();
            r.seats.push(Seat {
                name,
                token: tok.clone(),
                tx: Some(tx.clone()),
                dropped_at: None,
            });
            let _ = tx.send(ServerMsg::Joined { room: code.clone(), token: tok.clone() });
            broadcast(r);
            *ident = Some(Ident { room: code, token: tok });
            None
        }
        ClientMsg::SetRules(rules) => {
            let id = ident.as_ref()?;
            let r = rooms.map.get_mut(&id.room)?;
            if r.kind != GameKind::Hanabi {
                return Some("This game has no options to set.".into());
            }
            if r.game.is_some() {
                return Some("The game has already started.".into());
            }
            if !r.is_host(&id.token) {
                return Some("Only the host can change the rules.".into());
            }
            // The hanabii mode is a fixed preset that replaces every other
            // option. Resolve it here, so the room stores - and every
            // client is shown - the concrete rules that will actually be
            // played, and a hand-built message can't sneak extra options in
            // next to it.
            r.rules = rules.normalized();
            broadcast(r);
            None
        }
        ClientMsg::Start => {
            let id = ident.as_ref()?;
            let r = rooms.map.get_mut(&id.room)?;
            if r.game.is_some() {
                return Some("The game has already started.".into());
            }
            if !r.is_host(&id.token) {
                return Some("Only the host can start the game.".into());
            }
            if r.seats.len() < r.kind.min_players() {
                return Some("You need at least two players.".into());
            }
            let mut rng = rand::thread_rng();
            r.game = Some(match r.kind {
                GameKind::Catan => {
                    r.seats.shuffle(&mut rng); // random turn order
                    let names = r.seats.iter().map(|s| s.name.clone()).collect();
                    Running::Catan(Game::new(Rules::default(), names, &mut rng))
                }
                // Seats keep their join order: the host plays first.
                GameKind::Hanabi => {
                    Running::Hanabi(GameState::new(r.seats.len() as u8, rng.gen(), r.rules))
                }
                GameKind::Wonderful => {
                    // Everybody plays at once; the seat order decides who
                    // drafts from whom and which Empire you get, so shuffle it.
                    r.seats.shuffle(&mut rng);
                    Running::Wonderful(wonderful_core::State::new(r.seats.len(), rng.gen()))
                }
            });
            broadcast(r);
            None
        }
        ClientMsg::Leave => {
            let id = ident.take()?;
            leave(&mut rooms, &id, tx, true);
            None
        }
        ClientMsg::Catan(action) => {
            let id = ident.as_ref()?;
            let r = rooms.map.get_mut(&id.room)?;
            let seat = r.seats.iter().position(|s| s.token == id.token)?;
            let Some(Running::Catan(game)) = r.game.as_mut() else {
                return Some(not_running(r, GameKind::Catan));
            };
            let mut rng = rand::thread_rng();
            match game.apply(seat, action, &mut rng) {
                Ok(()) => {
                    broadcast(r);
                    None
                }
                Err(e) => Some(e),
            }
        }
        ClientMsg::Hanabi(action) => {
            let id = ident.as_ref()?;
            let r = rooms.map.get_mut(&id.room)?;
            let seat = r.seats.iter().position(|s| s.token == id.token)?;
            let Some(Running::Hanabi(game)) = r.game.as_mut() else {
                return Some(not_running(r, GameKind::Hanabi));
            };
            match game.apply_action(PlayerId(seat as u8), action) {
                Ok(_events) => {
                    broadcast(r);
                    None
                }
                Err(e) => Some(describe_hanabi_error(e)),
            }
        }
        ClientMsg::Wonderful(action) => {
            let id = ident.as_ref()?;
            let r = rooms.map.get_mut(&id.room)?;
            // The seat comes from the connection's token, never from the message.
            let seat = r.seats.iter().position(|s| s.token == id.token)?;
            let Some(Running::Wonderful(game)) = r.game.as_mut() else {
                return Some(not_running(r, GameKind::Wonderful));
            };
            match game.apply(seat, action) {
                Ok(()) => {
                    broadcast(r);
                    None
                }
                Err(e) => Some(e.message().into()),
            }
        }
    }
}

/// Sends every connected seat its own view of the room.
pub fn broadcast(room: &Room) {
    let connected: Vec<bool> = room.seats.iter().map(|s| s.tx.is_some()).collect();
    let names: Vec<String> = room.seats.iter().map(|s| s.name.clone()).collect();
    for (i, seat) in room.seats.iter().enumerate() {
        let Some(tx) = &seat.tx else { continue };
        let msg = match &room.game {
            Some(Running::Catan(g)) => ServerMsg::CatanState {
                view: Box::new(g.view_for(Some(i))),
                connected: connected.clone(),
            },
            Some(Running::Hanabi(g)) => ServerMsg::HanabiState {
                view: Box::new(g.view_for(PlayerId(i as u8))),
                names: names.clone(),
                connected: connected.clone(),
            },
            Some(Running::Wonderful(g)) => ServerMsg::WonderfulState {
                view: Box::new(g.view_for(i)),
                names: names.clone(),
                connected: connected.clone(),
            },
            None => ServerMsg::Lobby {
                room: room.code.clone(),
                game: room.kind,
                players: room.lobby_players(),
                you: i,
                is_host: i == 0,
                rules: (room.kind == GameKind::Hanabi).then_some(room.rules),
            },
        };
        let _ = tx.send(msg);
    }
}

fn leave(rooms: &mut Rooms, id: &Ident, tx: &Tx, explicit: bool) {
    let Some(r) = rooms.map.get_mut(&id.room) else { return };
    if explicit && r.game.is_none() {
        free_seat(rooms, id);
        return;
    }
    if let Some(seat) = r.seats.iter_mut().find(|s| s.token == id.token) {
        // Otherwise the seat is kept so the player can come back (to a lobby
        // only for a while, see `free_dropped_seat`), unless the connection
        // was already replaced by a reconnect.
        if explicit || seat.tx.as_ref().is_some_and(|t| t.same_channel(tx)) {
            seat.tx = None;
            seat.dropped_at = Some(Instant::now());
        }
    }
    broadcast(r);
}

/// Gives a lobby seat up (the next player in line becomes host), closing
/// the room if it was the last one.
fn free_seat(rooms: &mut Rooms, id: &Ident) {
    let Some(r) = rooms.map.get_mut(&id.room) else { return };
    r.seats.retain(|s| s.token != id.token);
    if r.seats.is_empty() {
        rooms.map.remove(&id.room);
    } else {
        broadcast(r);
    }
}

/// Frees a lobby seat whose player dropped at least `grace` ago and hasn't
/// come back, so a reload or a phone switching apps doesn't cost the seat
/// but a player who is really gone doesn't stay in the way. Seats in a
/// running game are kept for good.
pub fn free_dropped_seat(state: &Shared, id: &Ident, grace: Duration) {
    let mut rooms = state.lock().unwrap();
    let Some(r) = rooms.map.get(&id.room) else { return };
    let gone = r.game.is_none()
        && r.seats.iter().any(|s| {
            s.token == id.token && s.tx.is_none() && s.dropped_at.is_some_and(|t| t.elapsed() >= grace)
        });
    if gone {
        free_seat(&mut rooms, id);
    }
}

/// A connection has closed. Returns the seat it held, to free or to check
/// the room for abandonment later, if there is one.
pub fn disconnect(state: &Shared, tx: &Tx, ident: Option<Ident>) -> Option<Ident> {
    let id = ident?;
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
            return None;
        }
    }
    leave(&mut rooms, &id, tx, false);
    Some(id)
}

/// Drops the room if everybody has left it.
pub fn remove_if_abandoned(state: &Shared, code: &str) {
    let mut rooms = state.lock().unwrap();
    let abandoned = rooms
        .map
        .get(code)
        .is_some_and(|r| r.seats.iter().all(|s| s.tx.is_none()));
    if abandoned {
        rooms.map.remove(code);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hanabi_core::{Action, Card, Clue, Color, LastMove};
    use tokio::sync::mpsc::{self, UnboundedReceiver};

    /// A fake browser connection: what it sent goes through `process`, what
    /// the server sends back lands in `rx`.
    struct Client {
        tx: Tx,
        rx: UnboundedReceiver<ServerMsg>,
        ident: Option<Ident>,
    }

    impl Client {
        fn new() -> Client {
            let (tx, rx) = mpsc::unbounded_channel();
            Client { tx, rx, ident: None }
        }

        fn send(&mut self, state: &Shared, msg: ClientMsg) -> Option<String> {
            process(state, &self.tx, &mut self.ident, msg)
        }

        fn drain(&mut self) -> Vec<ServerMsg> {
            let mut out = Vec::new();
            while let Ok(m) = self.rx.try_recv() {
                out.push(m);
            }
            out
        }

        fn token(&self) -> String {
            self.ident.as_ref().expect("joined").token.clone()
        }
    }

    fn new_state() -> Shared {
        Arc::new(Mutex::new(Rooms::default()))
    }

    fn create(state: &Shared, game: GameKind, name: &str) -> (Client, String) {
        let mut c = Client::new();
        assert_eq!(c.send(state, ClientMsg::Create { game, name: name.into() }), None);
        let code = c.ident.as_ref().unwrap().room.clone();
        (c, code)
    }

    fn join(state: &Shared, code: &str, name: &str) -> Result<Client, String> {
        let mut c = Client::new();
        match c.send(state, ClientMsg::Join { room: code.into(), name: name.into(), token: None }) {
            None => Ok(c),
            Some(e) => Err(e),
        }
    }

    /// A lobby with `n` seated players; the first one is the host.
    fn lobby(state: &Shared, game: GameKind, n: usize) -> (String, Vec<Client>) {
        let (host, code) = create(state, game, "P0");
        let mut clients = vec![host];
        for i in 1..n {
            clients.push(join(state, &code, &format!("P{i}")).unwrap());
        }
        for c in &mut clients {
            c.drain();
        }
        (code, clients)
    }

    /// The Hanabi rules shown in the latest lobby message in `msgs`.
    fn lobby_rules(msgs: &[ServerMsg]) -> Option<GameRules> {
        msgs.iter().rev().find_map(|m| match m {
            ServerMsg::Lobby { rules, .. } => *rules,
            _ => None,
        })
    }

    fn set_rules(state: &Shared, c: &mut Client, rules: GameRules) -> Option<String> {
        c.send(state, ClientMsg::SetRules(rules))
    }

    fn hanabi_rules_of(state: &Shared, code: &str) -> GameRules {
        state.lock().unwrap().map[code].rules
    }

    // ----- the shared room system ------------------------------------

    #[test]
    fn creating_a_room_gives_a_code_a_token_and_a_lobby() {
        let state = new_state();
        let (mut host, code) = create(&state, GameKind::Catan, "Ann");
        assert_eq!(code.len(), 4);
        assert!(code.chars().all(|c| c.is_ascii_uppercase()));
        let msgs = host.drain();
        assert!(matches!(&msgs[0], ServerMsg::Joined { room, token } if *room == code && !token.is_empty()));
        match &msgs[1] {
            ServerMsg::Lobby { room, game, players, you, is_host, rules } => {
                assert_eq!(room, &code);
                assert_eq!(*game, GameKind::Catan);
                assert_eq!(players.len(), 1);
                assert_eq!((*you, *is_host), (0, true));
                assert!(rules.is_none(), "only Hanabi has lobby rules");
            }
            other => panic!("expected the lobby, got {other:?}"),
        }
    }

    #[test]
    fn the_room_code_decides_the_game() {
        let state = new_state();
        let (_a, hanabi_code) = create(&state, GameKind::Hanabi, "Ann");
        let (_b, catan_code) = create(&state, GameKind::Catan, "Bob");

        let mut c = join(&state, &hanabi_code.to_lowercase(), "Cy").unwrap();
        let mut d = join(&state, &catan_code, "Di").unwrap();
        let game_of = |c: &mut Client| {
            c.drain().into_iter().find_map(|m| match m {
                ServerMsg::Lobby { game, .. } => Some(game),
                _ => None,
            })
        };
        assert_eq!(game_of(&mut c), Some(GameKind::Hanabi));
        assert_eq!(game_of(&mut d), Some(GameKind::Catan));
    }

    #[test]
    fn unknown_rooms_names_and_double_joins_are_refused() {
        let state = new_state();
        let (mut host, code) = create(&state, GameKind::Hanabi, "Ann");
        assert_eq!(join(&state, "ZZZZ", "Bob").err().as_deref(), Some("No such room."));
        assert_eq!(join(&state, &code, "  ").err().as_deref(), Some("Please enter a name."));
        assert_eq!(join(&state, &code, "ann").err().as_deref(), Some("That name is taken in this room."));
        // Already seated: no second room, no second seat.
        assert_eq!(
            host.send(&state, ClientMsg::Create { game: GameKind::Hanabi, name: "Ann".into() }).as_deref(),
            Some("You are already in a room.")
        );
        assert_eq!(
            host.send(&state, ClientMsg::Join { room: code, name: "Ann".into(), token: None }).as_deref(),
            Some("You are already in a room.")
        );
    }

    #[test]
    fn long_names_are_cut_to_fit() {
        let state = new_state();
        let (_host, code) = create(&state, GameKind::Hanabi, "Ann");
        let guest = join(&state, &code, &"x".repeat(50)).unwrap();
        let rooms = state.lock().unwrap();
        let seat = rooms.map[&code].seats.iter().find(|s| s.token == guest.token()).unwrap();
        assert_eq!(seat.name.chars().count(), MAX_NAME_LEN);
    }

    #[test]
    fn rooms_fill_up_to_each_games_limit() {
        let state = new_state();
        let (code, _hanabi) = lobby(&state, GameKind::Hanabi, 5);
        assert_eq!(join(&state, &code, "P6").err().as_deref(), Some("That room is full."));

        let (code, _catan) = lobby(&state, GameKind::Catan, 4);
        assert_eq!(join(&state, &code, "P5").err().as_deref(), Some("That room is full."));

        let (code, _wonderful) = lobby(&state, GameKind::Wonderful, 5);
        assert_eq!(join(&state, &code, "P6").err().as_deref(), Some("That room is full."));
    }

    #[test]
    fn only_the_host_can_start_and_only_with_two_players() {
        for game in GameKind::ALL {
            let state = new_state();
            let (mut host, code) = create(&state, game, "Ann");
            assert_eq!(host.send(&state, ClientMsg::Start).as_deref(), Some("You need at least two players."));

            let mut guest = join(&state, &code, "Bob").unwrap();
            assert_eq!(guest.send(&state, ClientMsg::Start).as_deref(), Some("Only the host can start the game."));
            assert!(state.lock().unwrap().map[&code].game.is_none());

            assert_eq!(host.send(&state, ClientMsg::Start), None);
            assert!(state.lock().unwrap().map[&code].game.is_some());
            assert_eq!(host.send(&state, ClientMsg::Start).as_deref(), Some("The game has already started."));
        }
    }

    #[test]
    fn nobody_new_can_join_once_the_game_has_started() {
        let state = new_state();
        let (code, mut clients) = lobby(&state, GameKind::Hanabi, 2);
        assert_eq!(clients[0].send(&state, ClientMsg::Start), None);
        assert_eq!(join(&state, &code, "Late").err().as_deref(), Some("That game has already started."));
    }

    #[test]
    fn leaving_a_lobby_frees_the_seat_and_passes_the_host_role_on() {
        let state = new_state();
        let (code, mut clients) = lobby(&state, GameKind::Hanabi, 3);
        assert_eq!(clients[0].send(&state, ClientMsg::Leave), None);

        let msgs = clients[1].drain();
        match msgs.last().unwrap() {
            ServerMsg::Lobby { players, you, is_host, .. } => {
                let names: Vec<_> = players.iter().map(|p| p.name.as_str()).collect();
                assert_eq!(names, ["P1", "P2"]);
                assert_eq!((*you, *is_host), (0, true), "the next seat in line hosts");
            }
            other => panic!("expected the lobby, got {other:?}"),
        }
        // The freed seat can be taken again.
        join(&state, &code, "P0").unwrap();
        assert_eq!(state.lock().unwrap().map[&code].seats.len(), 3);
    }

    #[test]
    fn an_emptied_lobby_closes_the_room() {
        let state = new_state();
        let (mut host, code) = create(&state, GameKind::Catan, "Ann");
        host.send(&state, ClientMsg::Leave);
        assert!(!state.lock().unwrap().map.contains_key(&code));
    }

    /// The names in the latest lobby message in `msgs`, offline ones marked.
    fn lobby_names(msgs: &[ServerMsg]) -> Vec<String> {
        msgs.iter()
            .rev()
            .find_map(|m| match m {
                ServerMsg::Lobby { players, .. } => Some(
                    players
                        .iter()
                        .map(|p| if p.connected { p.name.clone() } else { format!("{} (offline)", p.name) })
                        .collect(),
                ),
                _ => None,
            })
            .expect("a lobby message")
    }

    #[test]
    fn a_dropped_lobby_seat_waits_for_its_player() {
        let state = new_state();
        let (code, mut clients) = lobby(&state, GameKind::Hanabi, 2);
        let mut bob = clients.remove(1);
        let token = bob.token();

        // A reload: the old socket closes first, the seat stays, offline.
        let id = disconnect(&state, &bob.tx, bob.ident.take()).unwrap();
        assert_eq!(lobby_names(&clients[0].drain()), ["P0", "P1 (offline)"]);
        free_dropped_seat(&state, &id, Duration::from_secs(120));
        assert_eq!(state.lock().unwrap().map[&code].seats.len(), 2, "still within the grace period");

        // The new page reclaims it with the token, under whatever name.
        let mut back = Client::new();
        let r = back.send(&state, ClientMsg::Join { room: code.clone(), name: "P0".into(), token: Some(token) });
        assert_eq!(r, None);
        assert_eq!(lobby_names(&back.drain()), ["P0", "P1"]);

        // The first timer firing later doesn't free a seat that came back.
        free_dropped_seat(&state, &id, Duration::ZERO);
        assert_eq!(state.lock().unwrap().map[&code].seats.len(), 2);
    }

    #[test]
    fn a_lone_host_who_drops_keeps_the_room_open_until_the_grace_runs_out() {
        let state = new_state();
        let (mut host, code) = create(&state, GameKind::Hanabi, "Ann");
        let id = disconnect(&state, &host.tx, host.ident.take()).unwrap();

        // The room survives the host switching apps to send the code...
        let mut bob = join(&state, &code, "Bob").unwrap();
        assert_eq!(lobby_names(&bob.drain()), ["Ann (offline)", "Bob"]);

        // ...and once Ann's seat is given up, Bob hosts.
        free_dropped_seat(&state, &id, Duration::ZERO);
        match bob.drain().last().unwrap() {
            ServerMsg::Lobby { players, is_host, .. } => {
                assert_eq!(players.len(), 1);
                assert!(*is_host);
            }
            other => panic!("expected the lobby, got {other:?}"),
        }

        // Alone, a dropped host's room closes when the grace runs out.
        let (mut cy, code) = create(&state, GameKind::Catan, "Cy");
        let id = disconnect(&state, &cy.tx, cy.ident.take()).unwrap();
        free_dropped_seat(&state, &id, Duration::ZERO);
        assert!(!state.lock().unwrap().map.contains_key(&code));
    }

    #[test]
    fn a_running_game_keeps_dropped_seats_past_the_grace() {
        let state = new_state();
        let (code, mut clients) = lobby(&state, GameKind::Hanabi, 2);
        clients[0].send(&state, ClientMsg::Start);
        let mut bob = clients.remove(1);
        let id = disconnect(&state, &bob.tx, bob.ident.take()).unwrap();
        free_dropped_seat(&state, &id, Duration::ZERO);
        assert_eq!(state.lock().unwrap().map[&code].seats.len(), 2);
    }

    #[test]
    fn a_dropped_player_gets_their_seat_back_with_the_token() {
        for game in GameKind::ALL {
            let state = new_state();
            let (code, mut clients) = lobby(&state, game, 2);
            clients[0].send(&state, ClientMsg::Start);
            let mut bob = clients.remove(1);
            let token = bob.token();
            let bob_name = "P1".to_string();
            bob.drain();
            clients[0].drain();

            // The connection drops: the seat stays, marked offline.
            let to_check = disconnect(&state, &bob.tx, bob.ident.take());
            assert_eq!(to_check.map(|id| id.room).as_deref(), Some(code.as_str()));
            let offline = clients[0].drain().pop().expect("the host is told");
            let connected = match offline {
                ServerMsg::CatanState { connected, .. }
                | ServerMsg::HanabiState { connected, .. }
                | ServerMsg::WonderfulState { connected, .. } => connected,
                other => panic!("expected a state message, got {other:?}"),
            };
            assert_eq!(connected.iter().filter(|c| !**c).count(), 1);

            // A stranger can't take it, the token holder can.
            assert_eq!(join(&state, &code, "Mallory").err().as_deref(), Some("That game has already started."));
            let mut back = Client::new();
            let r = back.send(
                &state,
                ClientMsg::Join { room: code.clone(), name: bob_name.clone(), token: Some(token.clone()) },
            );
            assert_eq!(r, None);
            let msgs = back.drain();
            assert!(matches!(&msgs[0], ServerMsg::Joined { token: t, .. } if *t == token));
            assert!(matches!(
                msgs.last().unwrap(),
                ServerMsg::CatanState { .. } | ServerMsg::HanabiState { .. } | ServerMsg::WonderfulState { .. }
            ));

            // Nobody left in the room: it is dropped once the timer fires.
            clients[0].send(&state, ClientMsg::Leave);
            back.send(&state, ClientMsg::Leave);
            remove_if_abandoned(&state, &code);
            assert!(!state.lock().unwrap().map.contains_key(&code));
        }
    }

    #[test]
    fn a_stale_disconnect_does_not_kick_a_reconnected_player() {
        let state = new_state();
        let (code, mut clients) = lobby(&state, GameKind::Hanabi, 2);
        clients[0].send(&state, ClientMsg::Start);
        let old = clients.remove(1);
        let token = old.token();

        // Reconnects (new tab) before the old socket is noticed as closed.
        let mut fresh = Client::new();
        fresh.send(&state, ClientMsg::Join { room: code.clone(), name: "P1".into(), token: Some(token) });
        let stale = disconnect(&state, &old.tx, old.ident);
        assert!(stale.is_none());
        let rooms = state.lock().unwrap();
        assert!(rooms.map[&code].seats.iter().all(|s| s.tx.is_some()));
    }

    #[test]
    fn a_move_for_the_wrong_game_or_before_the_start_is_refused() {
        let state = new_state();
        let (_code, mut clients) = lobby(&state, GameKind::Hanabi, 2);
        let catan_move = ClientMsg::Catan(engine::Action::EndTurn);
        assert_eq!(clients[0].send(&state, catan_move.clone()).as_deref(), Some("That move belongs to a different game."));
        let hanabi_move = ClientMsg::Hanabi(Action::Play { card_id: hanabi_core::CardId(0) });
        assert_eq!(clients[0].send(&state, hanabi_move.clone()).as_deref(), Some("The game hasn't started yet."));

        clients[0].send(&state, ClientMsg::Start);
        assert_eq!(clients[0].send(&state, catan_move).as_deref(), Some("That move belongs to a different game."));

        let (_, mut catan) = lobby(&state, GameKind::Catan, 2);
        catan[0].send(&state, ClientMsg::Start);
        assert_eq!(catan[0].send(&state, hanabi_move.clone()).as_deref(), Some("That move belongs to a different game."));

        // The same two checks for Wonderful World: its moves in another
        // game's room, another game's moves in its room, and a move early.
        let wonderful_move = ClientMsg::Wonderful(wonderful_core::Action::Ready);
        assert_eq!(catan[0].send(&state, wonderful_move.clone()).as_deref(), Some("That move belongs to a different game."));
        let (_, mut wonderful) = lobby(&state, GameKind::Wonderful, 2);
        assert_eq!(wonderful[0].send(&state, wonderful_move.clone()).as_deref(), Some("The game hasn't started yet."));
        wonderful[0].send(&state, ClientMsg::Start);
        assert_eq!(wonderful[0].send(&state, hanabi_move).as_deref(), Some("That move belongs to a different game."));
        assert_eq!(wonderful[0].send(&state, ClientMsg::Catan(engine::Action::EndTurn)).as_deref(), Some("That move belongs to a different game."));
    }

    #[test]
    fn an_empty_name_never_creates_a_room() {
        let state = new_state();
        let mut c = Client::new();
        assert_eq!(
            c.send(&state, ClientMsg::Create { game: GameKind::Hanabi, name: "   ".into() }).as_deref(),
            Some("Please enter a name.")
        );
        assert!(state.lock().unwrap().map.is_empty());
    }

    // ----- Catan through the shared rooms ------------------------

    #[test]
    fn a_catan_game_deals_every_seat_its_own_view_and_checks_whose_turn_it_is() {
        let state = new_state();
        let (code, mut clients) = lobby(&state, GameKind::Catan, 3);
        assert_eq!(clients[0].send(&state, ClientMsg::Start), None);

        // Everyone gets a view of their own seat; seats are a permutation.
        let mut views = Vec::new();
        for c in &mut clients {
            let view = c
                .drain()
                .into_iter()
                .find_map(|m| match m {
                    ServerMsg::CatanState { view, .. } => Some(view),
                    _ => None,
                })
                .expect("a state for every player");
            views.push(view);
        }
        let mut seats: Vec<usize> = views.iter().map(|v| v.seat.unwrap()).collect();
        seats.sort_unstable();
        assert_eq!(seats, [0, 1, 2]);

        // The player whose turn it isn't is turned away; the other one can place.
        let current = views[0].current;
        let (mine, other): (Vec<usize>, Vec<usize>) = (0..3).partition(|&i| views[i].seat == Some(current));
        let (cur, oth) = (mine[0], other[0]);
        let vertex = views[cur].legal.settlements[0];
        let act = ClientMsg::Catan(engine::Action::BuildSettlement { vertex });
        assert!(clients[oth].send(&state, act.clone()).is_some());
        assert_eq!(clients[cur].send(&state, act), None);
        for c in &mut clients {
            assert!(c.drain().iter().any(|m| matches!(m, ServerMsg::CatanState { .. })), "everyone sees the move");
        }
        assert!(matches!(state.lock().unwrap().map[&code].game, Some(Running::Catan(_))));
    }

    // ----- Hanabi through the shared rooms ---------------------------

    #[test]
    fn hanabi_seats_follow_join_order() {
        let state = new_state();
        let (_code, mut clients) = lobby(&state, GameKind::Hanabi, 3);
        clients[0].send(&state, ClientMsg::Start);
        for (i, c) in clients.iter_mut().enumerate() {
            let ServerMsg::HanabiState { view, names, connected } = c.drain().pop().unwrap() else {
                panic!("expected a Hanabi state");
            };
            assert_eq!(view.you, PlayerId(i as u8));
            assert_eq!(names, ["P0", "P1", "P2"]);
            assert_eq!(connected, [true, true, true]);
            assert_eq!(view.current_turn, PlayerId(0), "the host plays first");
            // Your own cards are hidden, everyone else's are shown.
            for (pid, hand) in &view.hands {
                assert_eq!(hand.iter().all(|c| c.card.is_none()), *pid == view.you);
            }
        }
    }

    #[test]
    fn picking_hanabii_locks_the_other_options_and_tells_everyone() {
        let state = new_state();
        let (code, mut clients) = lobby(&state, GameKind::Hanabi, 2);

        // A client that (buggy, or hand-built) sends every other option
        // alongside the mode.
        let greedy = GameRules {
            multicolor: true,
            black: true,
            extra_colors: 2,
            multicolor_short: true,
            black_short: true,
            extra_colors_short: true,
            six_cards: false,
            hanabii: true,
        };
        assert_eq!(set_rules(&state, &mut clients[0], greedy), None);

        let preset = GameRules { hanabii: true, ..Default::default() }.normalized();
        assert_eq!(hanabi_rules_of(&state, &code), preset);
        assert!(preset.hanabii && preset.six_cards && preset.extra_colors == 1);
        assert!(!preset.multicolor && !preset.black);

        // Everyone - the sender included - is shown the locked-in preset,
        // not what was asked for.
        for c in &mut clients {
            assert_eq!(lobby_rules(&c.drain()), Some(preset));
        }
    }

    #[test]
    fn ordinary_rules_pass_through_untouched() {
        let state = new_state();
        let (code, mut clients) = lobby(&state, GameKind::Hanabi, 2);
        let rules = GameRules { multicolor: true, extra_colors: 2, black_short: true, ..Default::default() };
        assert_eq!(set_rules(&state, &mut clients[0], rules), None);
        assert_eq!(hanabi_rules_of(&state, &code), rules);
        assert_eq!(lobby_rules(&clients[1].drain()), Some(rules));
    }

    #[test]
    fn unpicking_hanabii_frees_the_options_again() {
        let state = new_state();
        let (code, mut clients) = lobby(&state, GameKind::Hanabi, 2);
        set_rules(&state, &mut clients[0], GameRules { hanabii: true, ..Default::default() });
        // Back to a plain game, then a couple of ordinary options on top.
        let rules = GameRules { black: true, six_cards: true, ..Default::default() };
        set_rules(&state, &mut clients[0], rules);
        assert_eq!(hanabi_rules_of(&state, &code), rules);
    }

    #[test]
    fn only_the_host_picks_the_rules_and_only_in_a_hanabi_lobby() {
        let state = new_state();
        let (code, mut clients) = lobby(&state, GameKind::Hanabi, 2);
        let rules = GameRules { multicolor: true, ..Default::default() };
        assert_eq!(set_rules(&state, &mut clients[1], rules).as_deref(), Some("Only the host can change the rules."));
        assert_eq!(hanabi_rules_of(&state, &code), GameRules::default());
        assert!(clients[0].drain().is_empty(), "a refused change tells nobody");

        let (_code, mut catan) = lobby(&state, GameKind::Catan, 2);
        assert_eq!(
            set_rules(&state, &mut catan[0], rules).as_deref(),
            Some("This game has no options to set.")
        );
    }

    #[test]
    fn rules_cannot_change_once_the_game_has_started() {
        let state = new_state();
        let (code, mut clients) = lobby(&state, GameKind::Hanabi, 2);
        assert_eq!(clients[0].send(&state, ClientMsg::Start), None);
        assert!(state.lock().unwrap().map[&code].game.is_some());

        let r = set_rules(&state, &mut clients[0], GameRules { hanabii: true, ..Default::default() });
        assert_eq!(r.as_deref(), Some("The game has already started."));
        assert_eq!(hanabi_rules_of(&state, &code), GameRules::default());
    }

    #[test]
    fn a_hanabii_lobby_starts_a_hanabii_game_and_sends_everyone_a_view_of_it() {
        let state = new_state();
        let (code, mut clients) = lobby(&state, GameKind::Hanabi, 2);
        set_rules(&state, &mut clients[0], GameRules { hanabii: true, ..Default::default() });
        assert_eq!(clients[1].send(&state, ClientMsg::Start).as_deref(), Some("Only the host can start the game."));
        assert_eq!(clients[0].send(&state, ClientMsg::Start), None);

        {
            let rooms = state.lock().unwrap();
            let Some(Running::Hanabi(game)) = &rooms.map[&code].game else {
                panic!("the game should have started");
            };
            assert!(game.rules.hanabii);
            assert_eq!(game.rules.max_score(), 36);
            assert_eq!(game.fireworks.len(), 6);
            assert!(!game.fireworks.contains_key(&Color::White));
        }

        // The state push is what actually reaches the browser, which reads
        // it back with serde_json - so check it survives that trip, new
        // fields and all.
        for c in &mut clients {
            let mut saw_state = false;
            for msg in c.drain() {
                if let ServerMsg::HanabiState { view, .. } = msg {
                    let json = serde_json::to_string(&ServerMsg::HanabiState {
                        view: view.clone(),
                        names: vec![],
                        connected: vec![],
                    })
                    .unwrap();
                    let Ok(ServerMsg::HanabiState { view: back, .. }) = serde_json::from_str::<ServerMsg>(&json) else {
                        panic!("state update didn't round-trip: {json}");
                    };
                    assert_eq!(back.rules, view.rules);
                    assert!(back.rules.hanabii);
                    assert_eq!(back.fireworks, view.fireworks);
                    saw_state = true;
                }
            }
            assert!(saw_state, "every player should get a state update when the game starts");
        }
    }

    /// Starts a two-player Hanabi game and gives the second player a hand of
    /// `card`s, then forgets everything sent so far.
    fn hanabi_game_with_second_hand(state: &Shared, rules: GameRules, card: Card) -> (String, Vec<Client>) {
        let (code, mut clients) = lobby(state, GameKind::Hanabi, 2);
        if rules != GameRules::default() {
            assert_eq!(set_rules(state, &mut clients[0], rules), None);
        }
        assert_eq!(clients[0].send(state, ClientMsg::Start), None);
        {
            let mut rooms = state.lock().unwrap();
            let Some(Running::Hanabi(game)) = rooms.map.get_mut(&code).unwrap().game.as_mut() else {
                panic!("game started");
            };
            for hc in game.hands.get_mut(&PlayerId(1)).unwrap().iter_mut() {
                hc.card = card;
            }
        }
        for c in &mut clients {
            c.drain();
        }
        (code, clients)
    }

    #[test]
    fn a_hanabii_color_clue_that_touches_nothing_goes_through_the_server() {
        let state = new_state();
        // Deal the second player a hand with no red in it.
        let (_code, mut clients) = hanabi_game_with_second_hand(
            &state,
            GameRules { hanabii: true, ..Default::default() },
            Card { color: Color::Green, number: 1 },
        );
        let clue = ClientMsg::Hanabi(Action::Clue { target: PlayerId(1), clue: Clue::Color(Color::Red) });
        assert_eq!(clients[0].send(&state, clue), None);

        for c in &mut clients {
            let view = c
                .drain()
                .into_iter()
                .rev()
                .find_map(|m| match m {
                    ServerMsg::HanabiState { view, .. } => Some(view),
                    _ => None,
                })
                .expect("everyone gets the new state");
            assert_eq!(view.last_actor, Some(PlayerId(0)));
            match &view.last_moves[&PlayerId(0)] {
                LastMove::Clue { touched, .. } => assert!(touched.is_empty()),
                other => panic!("expected a clue, got {other:?}"),
            }
        }
    }

    #[test]
    fn the_same_clue_in_an_ordinary_game_is_still_turned_away() {
        let state = new_state();
        let (_code, mut clients) = hanabi_game_with_second_hand(
            &state,
            GameRules::default(),
            Card { color: Color::White, number: 1 },
        );
        let clue = ClientMsg::Hanabi(Action::Clue { target: PlayerId(1), clue: Clue::Color(Color::Red) });
        assert_eq!(
            clients[0].send(&state, clue).as_deref(),
            Some("That clue doesn't touch any card in their hand."),
            "an ordinary game must still refuse a clue that touches nothing"
        );
        assert!(clients[1].drain().is_empty(), "a refused move changes nothing for anyone");
    }

    #[test]
    fn hanabi_moves_are_checked_against_the_seat_not_the_message() {
        let state = new_state();
        let (_code, mut clients) = hanabi_game_with_second_hand(
            &state,
            GameRules::default(),
            Card { color: Color::Blue, number: 3 },
        );
        // The guest tries to act on the host's turn, and to clue themselves.
        let clue = ClientMsg::Hanabi(Action::Clue { target: PlayerId(0), clue: Clue::Number(1) });
        assert_eq!(clients[1].send(&state, clue.clone()).as_deref(), Some("It's not your turn."));
        assert_eq!(clients[0].send(&state, clue).as_deref(), Some("You can't give a clue to yourself."));
    }

    #[test]
    fn a_whole_hanabi_game_can_be_played_to_the_end_through_the_server() {
        let state = new_state();
        let (code, mut clients) = lobby(&state, GameKind::Hanabi, 2);
        clients[0].send(&state, ClientMsg::Start);
        for _ in 0..200 {
            let (status_over, current, card) = {
                let rooms = state.lock().unwrap();
                let Some(Running::Hanabi(g)) = &rooms.map[&code].game else { panic!() };
                let cur = g.current_player();
                (g.status != hanabi_core::GameStatus::InProgress, cur, g.hands[&cur][0].id)
            };
            if status_over {
                break;
            }
            // Always play the first card: fuses run out quickly, which is
            // exactly the game-over path we want to see the clients receive.
            let r = clients[current.0 as usize].send(&state, ClientMsg::Hanabi(Action::Play { card_id: card }));
            assert_eq!(r, None);
        }
        let rooms = state.lock().unwrap();
        let Some(Running::Hanabi(g)) = &rooms.map[&code].game else { panic!() };
        assert_ne!(g.status, hanabi_core::GameStatus::InProgress, "the game ended");
        drop(rooms);
        for c in &mut clients {
            let last = c.drain().into_iter().rev().find_map(|m| match m {
                ServerMsg::HanabiState { view, .. } => Some(view),
                _ => None,
            });
            assert_ne!(last.expect("final state").status, hanabi_core::GameStatus::InProgress);
        }
        // Once it is over, further moves are refused politely.
        let r = clients[0].send(&state, ClientMsg::Hanabi(Action::Play { card_id: hanabi_core::CardId(0) }));
        assert_eq!(r.as_deref(), Some("The game is over."));
    }

    // ----- Wonderful World through the shared rooms ------------------

    /// The latest Wonderful World view each client has been sent.
    fn latest_views(clients: &mut [Client], views: &mut [Option<Box<wonderful_core::View>>]) {
        for (c, v) in clients.iter_mut().zip(views.iter_mut()) {
            for m in c.drain() {
                if let ServerMsg::WonderfulState { view, .. } = m {
                    *v = Some(view);
                }
            }
        }
    }

    /// What a player who can only see their own view would do next.
    fn wonderful_move(v: &wonderful_core::View) -> Option<wonderful_core::Action> {
        use wonderful_core::{Action, Phase, Piece, Res, Target, Token};
        let me = &v.players[v.you];
        match v.phase {
            Phase::Draft => (v.picked.is_none() && !v.hand.is_empty()).then(|| Action::Draft { card: v.hand[0] }),
            Phase::Planning => match v.drafted.first() {
                Some(&card) if card.0 % 2 == 0 => Some(Action::Build { card }),
                Some(&card) => Some(Action::Recycle { card, to: Target::Empire }),
                None => (!me.ready).then_some(Action::Ready),
            },
            Phase::Production { step } if (step as usize) < Res::ALL.len() => {
                if me.choose {
                    return Some(Action::Choose { token: Token::General });
                }
                if me.ready {
                    return None;
                }
                let res = Res::ALL[step as usize];
                let target = me
                    .buildings
                    .iter()
                    .find(|b| b.remaining().res[res.index()] > 0)
                    .map_or(Target::Empire, |b| Target::Card(b.card));
                Some(Action::Place { piece: Piece::Cube(res), target })
            }
            Phase::Production { .. } => (!me.ready).then_some(Action::Ready),
            Phase::Over => None,
        }
    }

    #[test]
    fn wonderful_deals_every_seat_its_own_hand_in_a_shuffled_order() {
        let state = new_state();
        let (code, mut clients) = lobby(&state, GameKind::Wonderful, 4);
        assert_eq!(clients[0].send(&state, ClientMsg::Start), None);

        let mut hands: Vec<Vec<wonderful_core::CardId>> = Vec::new();
        let mut seen_names: Vec<String> = Vec::new();
        for (i, c) in clients.iter_mut().enumerate() {
            let ServerMsg::WonderfulState { view, names, connected } = c.drain().pop().unwrap() else {
                panic!("expected a Wonderful World state");
            };
            // The names are listed by seat, and this view is for this client's seat.
            assert_eq!(names[view.you], format!("P{i}"));
            assert_eq!(connected, [true; 4]);
            assert_eq!(view.hand.len(), 7);
            assert_eq!((view.round, view.phase), (1, wonderful_core::Phase::Draft));
            seen_names = names;
            hands.push(view.hand);
        }
        seen_names.sort();
        assert_eq!(seen_names, ["P0", "P1", "P2", "P3"]);
        // Nobody holds a card somebody else was dealt.
        let mut all: Vec<_> = hands.concat();
        all.sort_by_key(|c| c.0);
        all.dedup();
        assert_eq!(all.len(), 28);
        assert!(matches!(state.lock().unwrap().map[&code].game, Some(Running::Wonderful(_))));
    }

    #[test]
    fn wonderful_hides_picks_until_everybody_has_picked() {
        let state = new_state();
        let (_code, mut clients) = lobby(&state, GameKind::Wonderful, 3);
        clients[0].send(&state, ClientMsg::Start);
        let mut views = vec![None; 3];
        latest_views(&mut clients, &mut views);
        let first = views[0].clone().unwrap();
        let card = first.hand[2];

        assert_eq!(clients[0].send(&state, ClientMsg::Wonderful(wonderful_core::Action::Draft { card })), None);
        latest_views(&mut clients, &mut views);
        // The picker sees their pick; the others only learn that somebody has picked.
        assert_eq!(views[0].as_ref().unwrap().picked, Some(card));
        for other in [1, 2] {
            let v = views[other].as_ref().unwrap();
            assert_eq!(v.picked, None);
            assert!(v.players[first.you].picked);
            assert!(v.players.iter().filter(|p| p.picked).count() == 1);
            assert!(!v.hand.contains(&card));
        }
        // Nothing has been passed on yet.
        assert_eq!(views[1].as_ref().unwrap().hand.len(), 7);
    }

    #[test]
    fn wonderful_moves_are_checked_against_the_seat_not_the_message() {
        use wonderful_core::{Action, Target};
        let state = new_state();
        let (_code, mut clients) = lobby(&state, GameKind::Wonderful, 3);
        clients[0].send(&state, ClientMsg::Start);
        let mut views = vec![None; 3];
        latest_views(&mut clients, &mut views);
        let hand1 = views[1].clone().unwrap().hand;
        let hand0 = views[0].clone().unwrap().hand;

        // A card from somebody else's hand can't be taken, whatever the message says.
        let r = clients[0].send(&state, ClientMsg::Wonderful(Action::Draft { card: hand1[0] }));
        assert_eq!(r.as_deref(), Some("That card isn't in your hand."));
        // Planning moves during the draft.
        let r = clients[0].send(&state, ClientMsg::Wonderful(Action::Build { card: hand0[0] }));
        assert_eq!(r.as_deref(), Some("You can't do that right now."));
        let r = clients[0].send(&state, ClientMsg::Wonderful(Action::Recycle { card: hand0[0], to: Target::Empire }));
        assert_eq!(r.as_deref(), Some("You can't do that right now."));
        // One pick per step.
        assert_eq!(clients[0].send(&state, ClientMsg::Wonderful(Action::Draft { card: hand0[0] })), None);
        let r = clients[0].send(&state, ClientMsg::Wonderful(Action::Draft { card: hand0[1] }));
        assert_eq!(r.as_deref(), Some("You already picked a card: wait for the others."));
        // Refused moves change nothing for anyone.
        for c in &mut clients {
            c.drain();
        }
        assert!(clients[0]
            .send(&state, ClientMsg::Wonderful(Action::Draft { card: hand0[1] }))
            .is_some());
        for c in &mut clients {
            assert!(c.drain().is_empty());
        }
    }

    #[test]
    fn a_dropped_wonderful_player_comes_back_to_their_own_hand() {
        let state = new_state();
        let (code, mut clients) = lobby(&state, GameKind::Wonderful, 3);
        clients[0].send(&state, ClientMsg::Start);
        let mut views = vec![None; 3];
        latest_views(&mut clients, &mut views);
        let before = views[2].clone().unwrap();

        let mut dropped = clients.remove(2);
        let token = dropped.token();
        disconnect(&state, &dropped.tx, dropped.ident.take());

        let mut back = Client::new();
        let r = back.send(&state, ClientMsg::Join { room: code, name: "P2".into(), token: Some(token) });
        assert_eq!(r, None);
        let ServerMsg::WonderfulState { view, connected, .. } = back.drain().pop().unwrap() else {
            panic!("expected the game state on reconnecting");
        };
        assert_eq!((view.you, view.hand), (before.you, before.hand));
        assert_eq!(connected, [true; 3]);
    }

    #[test]
    fn wonderful_lobbies_have_no_options() {
        let state = new_state();
        let (_code, mut clients) = lobby(&state, GameKind::Wonderful, 2);
        let r = set_rules(&state, &mut clients[0], GameRules::default());
        assert_eq!(r.as_deref(), Some("This game has no options to set."));
        // The lobby message says so too.
        clients[0].send(&state, ClientMsg::Leave);
        let msgs = clients[1].drain();
        assert!(matches!(msgs.last(), Some(ServerMsg::Lobby { game: GameKind::Wonderful, rules: None, .. })));
    }

    #[test]
    fn a_whole_wonderful_game_can_be_played_to_the_end_through_the_server() {
        use wonderful_core::Phase;
        for n in [2, 3, 5] {
            let state = new_state();
            let (code, mut clients) = lobby(&state, GameKind::Wonderful, n);
            assert_eq!(clients[0].send(&state, ClientMsg::Start), None);
            let mut views: Vec<Option<Box<wonderful_core::View>>> = vec![None; n];
            latest_views(&mut clients, &mut views);

            for _ in 0..5000 {
                if views.iter().all(|v| v.as_ref().is_some_and(|v| v.phase == Phase::Over)) {
                    break;
                }
                let mut moved = false;
                for i in 0..n {
                    let view = views[i].clone().expect("every seat has a view");
                    if let Some(action) = wonderful_move(&view) {
                        let r = clients[i].send(&state, ClientMsg::Wonderful(action.clone()));
                        assert_eq!(r, None, "seat {i} {action:?} in {:?}", view.phase);
                        latest_views(&mut clients, &mut views);
                        moved = true;
                    }
                }
                assert!(moved, "the game got stuck");
            }

            for v in &views {
                let v = v.as_ref().unwrap();
                assert_eq!((v.phase, v.round), (Phase::Over, wonderful_core::ROUNDS));
                assert_eq!(v.scores.len(), n);
                assert!(!v.winners.is_empty());
            }
            // Once it is over, further moves are refused politely.
            let r = clients[0].send(&state, ClientMsg::Wonderful(wonderful_core::Action::Ready));
            assert_eq!(r.as_deref(), Some("The game is over."));
            assert!(matches!(state.lock().unwrap().map[&code].game, Some(Running::Wonderful(_))));
        }
    }
}
