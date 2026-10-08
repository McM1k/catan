//! Rules engine for "Colonists", a Catan-style board game.
//!
//! The engine is pure, deterministic given an RNG, and shared by the server
//! (authoritative state) and the client (rendering types).

mod board;
mod game;

pub use board::*;
pub use game::*;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Resource {
    Wood,
    Brick,
    Sheep,
    Wheat,
    Ore,
}

impl Resource {
    pub const ALL: [Resource; 5] = [
        Resource::Wood,
        Resource::Brick,
        Resource::Sheep,
        Resource::Wheat,
        Resource::Ore,
    ];

    pub fn idx(self) -> usize {
        self as usize
    }

    pub fn name(self) -> &'static str {
        match self {
            Resource::Wood => "Wood",
            Resource::Brick => "Brick",
            Resource::Sheep => "Sheep",
            Resource::Wheat => "Wheat",
            Resource::Ore => "Ore",
        }
    }
}

/// A multiset of resource cards.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hand(pub [u8; 5]);

impl Hand {
    pub fn of(items: &[(Resource, u8)]) -> Hand {
        let mut h = Hand::default();
        for (r, n) in items {
            h.0[r.idx()] += n;
        }
        h
    }

    pub fn get(&self, r: Resource) -> u8 {
        self.0[r.idx()]
    }

    pub fn add(&mut self, r: Resource, n: u8) {
        self.0[r.idx()] += n;
    }

    pub fn total(&self) -> u32 {
        self.0.iter().map(|&n| n as u32).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.total() == 0
    }

    pub fn contains(&self, other: &Hand) -> bool {
        (0..5).all(|i| self.0[i] >= other.0[i])
    }

    /// Removes `other` from `self`. Caller must check `contains` first.
    pub fn remove(&mut self, other: &Hand) {
        for i in 0..5 {
            self.0[i] -= other.0[i];
        }
    }

    pub fn insert(&mut self, other: &Hand) {
        for i in 0..5 {
            self.0[i] += other.0[i];
        }
    }

    pub fn overlaps(&self, other: &Hand) -> bool {
        (0..5).any(|i| self.0[i] > 0 && other.0[i] > 0)
    }
}
