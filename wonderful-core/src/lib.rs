//! Rules for a draft-and-build engine game in the style of *It's a Wonderful
//! World*: pure game logic, no networking and no UI.
//!
//! Everybody drafts cards, plans which to build and which to recycle, then
//! produces resources that are placed on the buildings under construction.
//! Four rounds, all players acting at the same time. See [`state`] for the
//! rules in full.
//!
//! **The cards are an original placeholder set** (see [`cards`]): the game
//! works end to end, but the catalogue is not the published one. It is a
//! single table, so swapping in other cards doesn't touch the rules.

pub mod cards;
pub mod state;
pub mod view;

pub use cards::*;
pub use state::*;
pub use view::*;

#[cfg(test)]
mod tests;
