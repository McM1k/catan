//! Hanabi rules engine: pure game logic, no networking and no UI.
//!
//! Imported from `McM1k/hanabii` (`crates/game-core`, commit c2d5fa6). The
//! only changes are that its wire messages moved to the shared `protocol`
//! crate (so both games use one room system) and that the redacted
//! per-player view lives in `view.rs` instead of `protocol.rs`.

pub mod card;
pub mod deck;
pub mod knowledge;
pub mod player;
pub mod rules;
pub mod state;
pub mod view;

pub use card::*;
pub use deck::*;
pub use knowledge::*;
pub use player::*;
pub use rules::*;
pub use state::*;
pub use view::*;
