//! What one seat is allowed to see.
//!
//! Hidden from everyone but its owner: the hand being drafted from, the card
//! picked in the current draft step, and the draft area. Everything else
//! (buildings, finished cards, cubes, characters, the log) is on the table.
//! The view has no maps, so it survives JSON.

use crate::cards::{CardId, Res};
use crate::state::{Building, Event, Phase, Score, State};
use serde::{Deserialize, Serialize};

/// What everybody can see about one player.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerView {
    /// Cards in the hand being drafted from.
    pub hand_size: usize,
    /// Has picked a card in this draft step (which one stays secret).
    pub picked: bool,
    /// Cards kept in the draft area.
    pub drafted: usize,
    pub buildings: Vec<Building>,
    pub empire: Vec<CardId>,
    pub empire_cubes: u8,
    pub krystallium: u8,
    pub generals: u8,
    pub financiers: u8,
    pub pending: [u8; 5],
    pub pool: u8,
    pub produced: u8,
    pub ready: bool,
    pub choose: bool,
    /// What the player produces for each resource, in `Res::ALL` order.
    pub production: [u8; 5],
    /// The points the player has right now.
    pub points: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct View {
    pub you: usize,
    pub round: u8,
    pub phase: Phase,
    /// Whether hands go to the next seat this round (otherwise the previous one).
    pub passes_to_next: bool,
    pub players: Vec<PlayerView>,
    /// Your hand: what you can pick from right now.
    pub hand: Vec<CardId>,
    /// The card you picked in this step.
    pub picked: Option<CardId>,
    /// Your draft area.
    pub drafted: Vec<CardId>,
    pub deck: usize,
    pub discard: usize,
    pub log: Vec<Event>,
    /// Once the game is over: every seat's points.
    pub scores: Vec<Score>,
    pub winners: Vec<usize>,
}

impl State {
    pub fn view_for(&self, seat: usize) -> View {
        let players = self
            .players
            .iter()
            .enumerate()
            .map(|(i, p)| PlayerView {
                hand_size: p.hand.len(),
                picked: p.picked.is_some(),
                drafted: p.drafted.len(),
                buildings: p.buildings.clone(),
                empire: p.empire.clone(),
                empire_cubes: p.empire_cubes,
                krystallium: p.krystallium,
                generals: p.generals,
                financiers: p.financiers,
                pending: p.pending,
                pool: p.pool,
                produced: p.produced,
                ready: p.ready,
                choose: p.choose,
                production: Res::ALL.map(|r| self.production(i, r)),
                points: self.score_of(i).total,
            })
            .collect();
        let me = &self.players[seat];
        View {
            you: seat,
            round: self.round,
            phase: self.phase,
            passes_to_next: self.passes_to_next(),
            players,
            hand: me.hand.clone(),
            picked: me.picked,
            drafted: me.drafted.clone(),
            deck: self.deck.len(),
            discard: self.discard.len(),
            log: self.log.clone(),
            scores: self.scores.clone(),
            winners: self.winners(),
        }
    }
}
