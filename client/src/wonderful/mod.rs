//! It's a Wonderful World: the screens (create/join, the waiting room and the
//! board) and what a click means.
//!
//! The game screen carries the state the server last sent (see
//! `Screen::WonderfulGame`) and is rebuilt on every message, as Catan is.
//! What only the browser knows (the piece picked in the tray, a cube waiting
//! for its place) lives in [`Ui`], which outlives those rebuilds.

pub mod board;
pub mod look;
pub mod screens;

use leptos::prelude::*;

/// Interface state that survives the screen being rebuilt.
#[derive(Clone, Copy)]
pub struct Ui {
    /// The piece picked in the tray, if any (see `look::effective_held`).
    pub held: RwSignal<Option<look::Held>>,
    /// A recycled card or a scrapped building whose cube is waiting to be
    /// given a place. Shown as a dialog until a place is picked or it's
    /// cancelled.
    pub cube_from: RwSignal<Option<look::CubeFrom>>,
}

impl Ui {
    pub fn new() -> Ui {
        Ui { held: RwSignal::new(None), cube_from: RwSignal::new(None) }
    }

    /// Forget everything, e.g. when leaving the room.
    pub fn reset(&self) {
        self.held.set(None);
        self.cube_from.set(None);
    }
}

impl Default for Ui {
    fn default() -> Self {
        Ui::new()
    }
}
