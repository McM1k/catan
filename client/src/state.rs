use crate::{hanabi, wonderful};
use engine::{Building, GameView};
use leptos::prelude::*;
use protocol::{ClientMsg, GameKind, LobbyPlayer, ServerMsg};
use std::time::Duration;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{MessageEvent, WebSocket};

const KEY_ROOM: &str = "games.room";
const KEY_TOKEN: &str = "games.token";
pub const KEY_NAME: &str = "games.name";

#[derive(Clone)]
pub enum Screen {
    /// Pick a game.
    Menu,
    /// Name + create/join a room for the chosen game.
    Home(GameKind),
    /// The waiting room. Which game it is for comes with the room: the code
    /// decides, whatever was picked in the menu.
    Lobby {
        room: String,
        game: GameKind,
        players: Vec<LobbyPlayer>,
        /// Our own seat, i.e. our index in `players`.
        you: usize,
        is_host: bool,
    },
    /// A running Catan game.
    Game {
        view: Box<GameView>,
        connected: Vec<bool>,
        room: String,
    },
    /// A running Hanabi game. Carries no data on purpose: the state lives in
    /// `App::hanabi`, so updates don't rebuild the board (see `hanabi::Signals`).
    HanabiGame,
    /// A running It's a Wonderful World game. Like Catan it carries the
    /// state and is rebuilt on every message; what has to outlive that (the
    /// piece picked in the tray) is in `App::wonderful`.
    WonderfulGame {
        view: Box<wonderful_core::View>,
        names: Vec<String>,
        connected: Vec<bool>,
        room: String,
    },
}

/// How long a changed piece keeps its ember contour.
pub const EMBER_MS: f64 = 2400.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FreshKind {
    Road(usize),
    /// A new settlement, or a settlement upgraded to a city.
    Piece(usize),
    /// The robber's new tile.
    Robber(usize),
}

#[derive(Clone, Copy)]
pub struct Fresh {
    pub kind: FreshKind,
    /// `now_ms()` when the change arrived.
    pub at: f64,
}

/// The parts of the previous game state needed to spot what changed.
#[derive(Clone)]
pub struct Snap {
    buildings: Vec<Option<Building>>,
    roads: Vec<Option<usize>>,
    robber: usize,
}

impl Snap {
    pub fn of(v: &GameView) -> Snap {
        Snap {
            buildings: v.buildings.clone(),
            roads: v.roads.clone(),
            robber: v.robber,
        }
    }
}

/// Everything on the board that differs between `prev` and `v`.
pub fn diff(prev: &Snap, v: &GameView) -> Vec<FreshKind> {
    if prev.buildings.len() != v.buildings.len() || prev.roads.len() != v.roads.len() {
        return vec![]; // a different board: nothing to compare
    }
    let mut out = Vec::new();
    for (e, r) in v.roads.iter().enumerate() {
        if r.is_some() && prev.roads[e] != *r {
            out.push(FreshKind::Road(e));
        }
    }
    for (i, b) in v.buildings.iter().enumerate() {
        if b.is_some() && prev.buildings[i] != *b {
            out.push(FreshKind::Piece(i));
        }
    }
    if prev.robber != v.robber {
        out.push(FreshKind::Robber(v.robber));
    }
    out
}

#[cfg(target_arch = "wasm32")]
pub fn now_ms() -> f64 {
    js_sys::Date::now()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn now_ms() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64() * 1000.0)
        .unwrap_or(0.0)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    None,
    Road,
    Settlement,
    City,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Picker {
    YearOfPlenty,
    Monopoly,
}

/// Purely client-side interface state. Lives at the top level so it
/// survives the game view being re-rendered on every server update.
#[derive(Clone, Copy)]
pub struct Ui {
    pub mode: RwSignal<Mode>,
    pub robber_tile: RwSignal<Option<usize>>,
    pub give: RwSignal<[u8; 5]>,
    pub want: RwSignal<[u8; 5]>,
    pub bank_give: RwSignal<Option<usize>>,
    pub bank_get: RwSignal<Option<usize>>,
    pub discard: RwSignal<[u8; 5]>,
    pub picker: RwSignal<Option<Picker>>,
    pub picks: RwSignal<Vec<usize>>,
}

#[derive(Clone, Copy)]
pub struct App {
    pub ws: StoredValue<Option<WebSocket>, LocalStorage>,
    pub online: RwSignal<bool>,
    pub screen: RwSignal<Screen>,
    pub error: RwSignal<Option<String>>,
    pub name: RwSignal<String>,
    pub room_input: RwSignal<String>,
    pub ui: Ui,
    /// Board changes still showing their ember contour.
    pub fresh: RwSignal<Vec<Fresh>>,
    snap: StoredValue<Option<Snap>>,
    /// Everything about the Hanabi game and lobby.
    pub hanabi: hanabi::Signals,
    /// The interface state of the It's a Wonderful World board.
    pub wonderful: wonderful::Ui,
}

impl App {
    pub fn new() -> App {
        App {
            ws: StoredValue::new_local(None),
            online: RwSignal::new(false),
            screen: RwSignal::new(Screen::Menu),
            error: RwSignal::new(None),
            name: RwSignal::new(load(KEY_NAME).unwrap_or_default()),
            room_input: RwSignal::new(String::new()),
            fresh: RwSignal::new(vec![]),
            snap: StoredValue::new(None),
            hanabi: hanabi::Signals::new(),
            wonderful: wonderful::Ui::new(),
            ui: Ui {
                mode: RwSignal::new(Mode::None),
                robber_tile: RwSignal::new(None),
                give: RwSignal::new([0; 5]),
                want: RwSignal::new([0; 5]),
                bank_give: RwSignal::new(None),
                bank_get: RwSignal::new(None),
                discard: RwSignal::new([0; 5]),
                picker: RwSignal::new(None),
                picks: RwSignal::new(vec![]),
            },
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

// Native builds only exist for `cargo check` and render tests.
#[cfg(not(target_arch = "wasm32"))]
fn storage() -> Option<web_sys::Storage> {
    None
}

pub fn load(key: &str) -> Option<String> {
    storage()?.get_item(key).ok()?
}

pub fn save(key: &str, value: &str) {
    if let Some(s) = storage() {
        let _ = s.set_item(key, value);
    }
}

fn clear_session() {
    if let Some(s) = storage() {
        let _ = s.remove_item(KEY_ROOM);
        let _ = s.remove_item(KEY_TOKEN);
    }
}

fn ws_url() -> String {
    let loc = web_sys::window().unwrap().location();
    let secure = loc.protocol().unwrap_or_default() == "https:";
    let host = loc.host().unwrap_or_default();
    format!("{}://{}/ws", if secure { "wss" } else { "ws" }, host)
}

pub fn send(app: App, msg: &ClientMsg) {
    let Ok(text) = serde_json::to_string(msg) else { return };
    app.ws.with_value(|ws| {
        if let Some(ws) = ws {
            let _ = ws.send_with_str(&text);
        }
    });
}

pub fn show_error(app: App, msg: String) {
    app.error.set(Some(msg));
    set_timeout(move || app.error.set(None), Duration::from_secs(5));
}

/// Opens the WebSocket and keeps it alive: on close it retries and, if a
/// saved session exists, rejoins the same seat.
pub fn connect(app: App) {
    let ws = match WebSocket::new(&ws_url()) {
        Ok(ws) => ws,
        Err(_) => {
            set_timeout(move || connect(app), Duration::from_secs(2));
            return;
        }
    };

    let onopen = Closure::<dyn FnMut()>::new(move || {
        app.online.set(true);
        if let (Some(room), Some(token)) = (load(KEY_ROOM), load(KEY_TOKEN)) {
            let name = load(KEY_NAME).unwrap_or_default();
            send(
                app,
                &ClientMsg::Join {
                    room,
                    name,
                    token: Some(token),
                },
            );
        }
    });
    ws.set_onopen(Some(onopen.as_ref().unchecked_ref()));
    onopen.forget();

    let onmessage = Closure::<dyn FnMut(MessageEvent)>::new(move |e: MessageEvent| {
        let Some(text) = e.data().as_string() else { return };
        match serde_json::from_str::<ServerMsg>(&text) {
            Ok(msg) => handle(app, msg),
            Err(err) => show_error(app, format!("Bad message from server: {err}")),
        }
    });
    ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
    onmessage.forget();

    let onclose = Closure::<dyn FnMut()>::new(move || {
        app.online.set(false);
        set_timeout(move || connect(app), Duration::from_secs(2));
    });
    ws.set_onclose(Some(onclose.as_ref().unchecked_ref()));
    onclose.forget();

    app.ws.set_value(Some(ws));
}

fn handle(app: App, msg: ServerMsg) {
    match msg {
        ServerMsg::Joined { room, token } => {
            save(KEY_ROOM, &room);
            save(KEY_TOKEN, &token);
            app.hanabi.room.set(room);
            app.error.set(None);
        }
        ServerMsg::Lobby {
            room,
            game,
            players,
            you,
            is_host,
            rules,
        } => {
            app.snap.set_value(None);
            app.hanabi.room.set(room.clone());
            if let Some(rules) = rules {
                app.hanabi.rules.set(rules);
            }
            // The lobby is sent again whenever somebody joins, leaves or the
            // host changes a rule. Only rebuild the screen when what it shows
            // changed (the rules have their own signal), so the controls the
            // host is using aren't torn down under their cursor.
            let unchanged = app.screen.with_untracked(|s| {
                matches!(s, Screen::Lobby { room: r, game: g, players: p, you: y, is_host: h }
                    if *r == room && *g == game && *p == players && *y == you && *h == is_host)
            });
            if !unchanged {
                app.screen.set(Screen::Lobby {
                    room,
                    game,
                    players,
                    you,
                    is_host,
                });
            }
        }
        ServerMsg::CatanState { view, connected } => {
            note_changes(app, &view);
            let room = load(KEY_ROOM).unwrap_or_default();
            app.screen.set(Screen::Game {
                view,
                connected,
                room,
            })
        }
        ServerMsg::HanabiState {
            view,
            names,
            connected,
        } => {
            app.hanabi.names.set(names);
            app.hanabi.connected.set(connected);
            app.hanabi.view.set(Some(*view));
            // The board is built once; later states only change its signals.
            if !matches!(app.screen.get_untracked(), Screen::HanabiGame) {
                app.screen.set(Screen::HanabiGame);
            }
        }
        ServerMsg::WonderfulState {
            view,
            names,
            connected,
        } => {
            let room = load(KEY_ROOM).unwrap_or_default();
            app.screen.set(Screen::WonderfulGame {
                view,
                names,
                connected,
                room,
            })
        }
        ServerMsg::Error(e) => {
            // A failed automatic rejoin means the saved session is dead.
            if matches!(app.screen.get_untracked(), Screen::Menu | Screen::Home(_)) {
                clear_session();
            }
            show_error(app, e);
        }
    }
}

/// Compares the new state with the previous one and starts an ember
/// contour on everything that changed. The first state after joining or
/// reconnecting is only remembered, not highlighted.
fn note_changes(app: App, view: &GameView) {
    let changes = app
        .snap
        .with_value(|prev| prev.as_ref().map(|p| diff(p, view)))
        .unwrap_or_default();
    app.snap.set_value(Some(Snap::of(view)));
    if changes.is_empty() {
        return;
    }
    let now = now_ms();
    app.fresh.update(|list| {
        list.retain(|f| now - f.at < EMBER_MS);
        for kind in changes {
            // One robber ring at a time; re-flag anything that changed again.
            list.retain(|f| {
                f.kind != kind && !(matches!(f.kind, FreshKind::Robber(_)) && matches!(kind, FreshKind::Robber(_)))
            });
            list.push(Fresh { kind, at: now });
        }
    });
    set_timeout(
        move || {
            let now = now_ms();
            app.fresh.update(|list| list.retain(|f| now - f.at < EMBER_MS));
        },
        Duration::from_millis(EMBER_MS as u64 + 50),
    );
}

/// Opens a room for `game` under the name typed on the home screen.
pub fn create_room(app: App, game: GameKind) {
    let name = app.name.get_untracked();
    save(KEY_NAME, &name);
    send(app, &ClientMsg::Create { game, name });
}

/// Joins the room whose code is typed on the home screen. The code decides
/// which game you land in.
pub fn join_room(app: App) {
    let name = app.name.get_untracked();
    save(KEY_NAME, &name);
    send(
        app,
        &ClientMsg::Join {
            room: app.room_input.get_untracked(),
            name,
            token: None,
        },
    );
}

pub fn leave(app: App) {
    send(app, &ClientMsg::Leave);
    clear_session();
    app.snap.set_value(None);
    app.fresh.set(vec![]);
    // Leave the screen first, so the Hanabi board is gone before its state is.
    app.screen.set(Screen::Menu);
    app.hanabi.reset();
    app.wonderful.reset();
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine::{Action, Game, Hand, Phase, Rules, SetupExpect};
    use rand::{rngs::StdRng, SeedableRng};

    fn game_after_setup() -> (Game, StdRng) {
        let mut rng = StdRng::seed_from_u64(7);
        let names = (0..3).map(|i| format!("P{i}")).collect();
        let mut g = Game::new(Rules::default(), names, &mut rng);
        while let Phase::Setup { expect, .. } = g.phase.clone() {
            let s = g.current;
            let a = match expect {
                SetupExpect::Settlement => Action::BuildSettlement { vertex: g.legal_settlements(s, true)[0] },
                SetupExpect::Road => Action::BuildRoad { edge: g.legal_roads(s, g.setup_vertex)[0] },
            };
            g.apply(s, a, &mut rng).unwrap();
        }
        (g, rng)
    }

    #[test]
    fn diff_flags_exactly_what_changed() {
        let (mut g, mut rng) = game_after_setup();
        g.do_roll(2, 3, &mut rng).unwrap();
        let snap = |g: &Game| Snap::of(&g.view_for(Some(1)));

        // Nothing changed -> nothing flagged (also for another viewer).
        let before = snap(&g);
        assert!(diff(&before, &g.view_for(Some(2))).is_empty());

        // A new road.
        g.players[0].hand = Hand([1, 1, 0, 0, 0]);
        let e = g.legal_roads(0, None)[0];
        g.apply(0, Action::BuildRoad { edge: e }, &mut rng).unwrap();
        assert_eq!(diff(&before, &g.view_for(Some(1))), vec![FreshKind::Road(e)]);

        // A settlement upgraded to a city counts as a new piece.
        let before = snap(&g);
        let v = g.legal_cities(0)[0];
        g.players[0].hand = Hand([0, 0, 0, 2, 3]);
        g.apply(0, Action::BuildCity { vertex: v }, &mut rng).unwrap();
        assert_eq!(diff(&before, &g.view_for(Some(1))), vec![FreshKind::Piece(v)]);

        // The robber moving to a new tile.
        let before = snap(&g);
        g.phase = Phase::MoveRobber { back_to_roll: false };
        let tile = (0..19)
            .find(|&t| t != g.robber && g.board.tiles[t].vertices.iter().all(|&v| g.buildings[v].is_none()))
            .unwrap();
        g.apply(0, Action::MoveRobber { tile, victim: None }, &mut rng).unwrap();
        assert_eq!(diff(&before, &g.view_for(Some(1))), vec![FreshKind::Robber(tile)]);
    }
}
