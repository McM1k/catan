//! The Hanabi screens: join, lobby with the rules picker, and the board.
//!
//! Ported from the stand-alone Hanabi client (McM1k/hanabii, Leptos 0.6).
//! The rooms are now the shared ones (see `crate::state`), so the old
//! name-and-code join screen became the same create/join flow Catan
//! uses, and the host starts the game.

pub mod board;
pub mod look;
pub mod screens;

use hanabi_core::{GameRules, PlayerView};
use leptos::prelude::*;

/// The Hanabi state shared by its screens. Kept apart from `Screen` so the
/// board's DOM survives state updates: it is built once per game and only
/// its signals change, which is what lets hands slide to their new spot,
/// drags survive and highlights time out undisturbed.
#[derive(Clone, Copy)]
pub struct Signals {
    /// The code of the room we're in, for the bar above the board.
    pub room: RwSignal<String>,
    /// What the server last sent for the running game, seen from our seat.
    pub view: RwSignal<Option<PlayerView>>,
    /// Seat names, indexed by `PlayerId`.
    pub names: RwSignal<Vec<String>>,
    /// Who is currently connected, indexed by `PlayerId`.
    pub connected: RwSignal<Vec<bool>>,
    /// The variant rules picked in the lobby (the host changes them, everyone
    /// sees them).
    pub rules: RwSignal<GameRules>,
}

impl Signals {
    pub fn new() -> Signals {
        Signals {
            room: RwSignal::new(String::new()),
            view: RwSignal::new(None),
            names: RwSignal::new(Vec::new()),
            connected: RwSignal::new(Vec::new()),
            rules: RwSignal::new(GameRules::default()),
        }
    }

    /// Forget the game, e.g. when leaving the room.
    pub fn reset(&self) {
        self.room.set(String::new());
        self.view.set(None);
        self.names.set(Vec::new());
        self.connected.set(Vec::new());
        self.rules.set(GameRules::default());
    }

    /// Whether the "hanabii" mode is on: what the lobby has picked until the
    /// game starts, then what the game is actually being played with.
    pub fn hanabii(&self) -> bool {
        self.view.with(|view| match view {
            Some(view) => view.rules.hanabii,
            None => self.rules.with(|rules| rules.hanabii),
        })
    }
}

impl Default for Signals {
    fn default() -> Self {
        Signals::new()
    }
}
