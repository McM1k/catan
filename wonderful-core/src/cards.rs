//! The pieces of the game and the card catalogue.
//!
//! **The catalogue is an original, placeholder set.** It follows the shape
//! of the real game (five resources, five card types, construction costs,
//! production icons, recycling and construction bonuses, victory points
//! from cards, card types and characters) but none of the numbers are the
//! real cards'. The whole catalogue is the one table in [`designs`]; replace
//! its rows (and keep [`DESIGNS`] and [`COPIES`] in step) to change the cards.

use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

/// The five resources, in production order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Res {
    Materials,
    Energy,
    Science,
    Gold,
    Exploration,
}

impl Res {
    pub const ALL: [Res; 5] = [Res::Materials, Res::Energy, Res::Science, Res::Gold, Res::Exploration];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn name(self) -> &'static str {
        match self {
            Res::Materials => "Materials",
            Res::Energy => "Energy",
            Res::Science => "Science",
            Res::Gold => "Gold",
            Res::Exploration => "Exploration",
        }
    }

    /// The character the player with the most of this resource receives.
    /// `None` for Science, where the winner chooses.
    pub fn supremacy_token(self) -> Option<Token> {
        match self {
            Res::Materials | Res::Energy => Some(Token::General),
            Res::Gold | Res::Exploration => Some(Token::Financier),
            Res::Science => None,
        }
    }
}

/// The five card types.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Kind {
    Structure,
    Vehicle,
    Research,
    Project,
    Discovery,
}

impl Kind {
    pub const ALL: [Kind; 5] = [Kind::Structure, Kind::Vehicle, Kind::Research, Kind::Project, Kind::Discovery];

    pub fn name(self) -> &'static str {
        match self {
            Kind::Structure => "Structure",
            Kind::Vehicle => "Vehicle",
            Kind::Research => "Research",
            Kind::Project => "Project",
            Kind::Discovery => "Discovery",
        }
    }
}

/// The two kinds of character token. Each is worth a point at the end.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Token {
    General,
    Financier,
}

impl Token {
    pub fn name(self) -> &'static str {
        match self {
            Token::General => "General",
            Token::Financier => "Financier",
        }
    }
}

/// What a card needs to be built, or (in a building) what has been placed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cost {
    /// Spaces for resource cubes, in [`Res::ALL`] order.
    pub res: [u8; 5],
    pub generals: u8,
    pub financiers: u8,
}

impl Cost {
    pub fn total(&self) -> u32 {
        self.res.iter().map(|&n| n as u32).sum::<u32>() + self.generals as u32 + self.financiers as u32
    }

    /// What is still missing when `filled` has been placed.
    pub fn minus(&self, filled: &Cost) -> Cost {
        let mut res = [0u8; 5];
        for (i, r) in res.iter_mut().enumerate() {
            *r = self.res[i].saturating_sub(filled.res[i]);
        }
        Cost {
            res,
            generals: self.generals.saturating_sub(filled.generals),
            financiers: self.financiers.saturating_sub(filled.financiers),
        }
    }

    pub fn is_zero(&self) -> bool {
        self.total() == 0
    }
}

/// What a card hands over the moment it is finished.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Bonus {
    /// A cube of this resource, to place on a card or on your Empire.
    Cube(Res),
    Krystallium,
    Token(Token),
}

/// Identifies one physical card of the deck. The deck holds [`COPIES`]
/// copies of every design, told apart by their number.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CardId(pub u16);

impl CardId {
    pub fn def(self) -> &'static Card {
        let all = catalogue();
        &all[self.0 as usize % all.len()]
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Card {
    pub name: &'static str,
    pub kind: Kind,
    pub cost: Cost,
    /// Resource icons produced every round once finished.
    pub produces: [u8; 5],
    /// One more icon of this resource for every card of that type you own.
    pub scaled: Option<(Res, Kind)>,
    /// The cube you take when you recycle the card.
    pub recycle: Res,
    pub bonus: Option<Bonus>,
    /// Points that depend on nothing.
    pub vp: u8,
    /// Points for each card of this type you own (this one included if it
    /// is of that type).
    pub combo: Option<(Kind, u8)>,
    /// Extra points for each character token of this type you hold.
    pub per_token: Option<(Token, u8)>,
}

impl Card {
    fn new(name: &'static str, kind: Kind, cost: [u8; 5], produces: [u8; 5], recycle: Res, vp: u8) -> Card {
        Card {
            name,
            kind,
            cost: Cost { res: cost, generals: 0, financiers: 0 },
            produces,
            scaled: None,
            recycle,
            bonus: None,
            vp,
            combo: None,
            per_token: None,
        }
    }

    /// Character spaces in the cost: `g` Generals and `f` Financiers.
    fn chars(mut self, g: u8, f: u8) -> Card {
        self.cost.generals = g;
        self.cost.financiers = f;
        self
    }

    fn scaled(mut self, res: Res, kind: Kind) -> Card {
        self.scaled = Some((res, kind));
        self
    }

    fn bonus(mut self, bonus: Bonus) -> Card {
        self.bonus = Some(bonus);
        self
    }

    fn combo(mut self, kind: Kind, points: u8) -> Card {
        self.combo = Some((kind, points));
        self
    }

    fn per_token(mut self, token: Token, points: u8) -> Card {
        self.per_token = Some((token, points));
        self
    }
}

/// How many designs the catalogue has, and how many copies of each are in
/// the deck: 75 × 2 = 150 cards, enough for five players.
pub const DESIGNS: usize = 75;
pub const COPIES: usize = 2;
pub const DECK_SIZE: usize = DESIGNS * COPIES;

static CATALOGUE: OnceLock<Vec<Card>> = OnceLock::new();

/// Every design, indexed by `CardId.0 % DESIGNS`.
pub fn catalogue() -> &'static [Card] {
    CATALOGUE.get_or_init(designs)
}

/// The starting production of an Empire. Seat `i` plays Empire `i`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Empire {
    pub name: &'static str,
    pub base: [u8; 5],
}

pub const EMPIRES: [Empire; 5] = [
    Empire { name: "Aurelian Union", base: [1, 1, 0, 1, 0] },
    Empire { name: "Meridian League", base: [0, 1, 1, 0, 1] },
    Empire { name: "Solar Concord", base: [1, 0, 1, 1, 0] },
    Empire { name: "Verdant Pact", base: [0, 1, 0, 1, 1] },
    Empire { name: "Obsidian Court", base: [1, 0, 1, 0, 1] },
];

#[allow(clippy::vec_init_then_push)]
fn designs() -> Vec<Card> {
    use Bonus::{Cube, Krystallium};
    use Kind::{Discovery, Project, Research, Structure, Vehicle};
    use Res::{Energy as E, Exploration as X, Gold as G, Materials as M, Science as S};
    let general = Bonus::Token(Token::General);
    let financier = Bonus::Token(Token::Financier);
    let c = Card::new;

    let mut v = Vec::with_capacity(DESIGNS);

    // Structures: materials, energy and gold; solid points.
    v.push(c("Quarry", Structure, [2, 0, 0, 0, 0], [1, 0, 0, 0, 0], M, 1));
    v.push(c("Smelter", Structure, [3, 1, 0, 0, 0], [2, 0, 0, 0, 0], E, 1));
    v.push(c("Wind Farm", Structure, [1, 2, 0, 0, 0], [0, 1, 0, 0, 0], E, 1));
    v.push(c("Reactor Hall", Structure, [2, 3, 0, 0, 0], [0, 2, 0, 0, 0], E, 2));
    v.push(c("Granary", Structure, [2, 0, 0, 1, 0], [0, 0, 0, 1, 0], G, 2));
    v.push(c("Bazaar", Structure, [1, 0, 0, 3, 0], [0, 0, 0, 2, 0], G, 1).bonus(financier));
    v.push(c("Bastion", Structure, [3, 0, 0, 0, 0], [1, 0, 0, 0, 0], M, 3).chars(1, 0));
    v.push(c("Skyline Tower", Structure, [4, 1, 0, 2, 0], [0; 5], M, 3).combo(Structure, 1));
    v.push(c("Dome Habitat", Structure, [2, 1, 1, 0, 0], [0, 0, 1, 0, 0], S, 2));
    v.push(c("Harbor Works", Structure, [2, 0, 0, 1, 1], [0, 0, 0, 0, 1], X, 1));
    v.push(c("Great Wall", Structure, [5, 0, 0, 0, 0], [2, 0, 0, 0, 0], M, 4).chars(1, 0));
    v.push(c("Civic Forum", Structure, [3, 0, 1, 2, 0], [0; 5], G, 2).chars(0, 1).combo(Project, 1));
    v.push(c("Mega Foundry", Structure, [4, 3, 0, 0, 0], [0; 5], E, 1).scaled(M, Structure));
    v.push(c("Archive Vault", Structure, [2, 0, 3, 1, 0], [0; 5], S, 2).per_token(Token::Financier, 1));
    v.push(c("Monument", Structure, [4, 0, 0, 3, 0], [0; 5], G, 6));

    // Vehicles: energy and exploration; they move things along.
    v.push(c("Hover Cart", Vehicle, [1, 1, 0, 0, 0], [0, 1, 0, 0, 0], E, 1));
    v.push(c("Rail Line", Vehicle, [2, 2, 0, 0, 0], [0, 0, 0, 0, 1], M, 1));
    v.push(c("Skiff", Vehicle, [1, 0, 0, 0, 1], [0, 0, 0, 0, 1], X, 1));
    v.push(c("Cargo Hauler", Vehicle, [2, 2, 0, 1, 0], [1, 0, 0, 1, 0], G, 1));
    v.push(c("Starfreighter", Vehicle, [3, 3, 0, 2, 0], [0, 1, 0, 1, 1], E, 3));
    v.push(c("Armored Convoy", Vehicle, [3, 1, 0, 0, 0], [0, 1, 0, 0, 0], M, 2).chars(1, 0));
    v.push(c("Airship", Vehicle, [2, 2, 0, 0, 2], [0, 0, 0, 0, 2], X, 2));
    v.push(c("Fleet Command", Vehicle, [3, 2, 0, 2, 0], [0; 5], E, 2).combo(Vehicle, 1));
    v.push(c("Deep Diver", Vehicle, [2, 1, 1, 0, 2], [0, 0, 1, 0, 1], S, 3));
    v.push(c("Tram Network", Vehicle, [4, 2, 0, 0, 0], [0; 5], M, 1).scaled(X, Vehicle));
    v.push(c("Ion Courier", Vehicle, [1, 3, 1, 0, 0], [0, 2, 0, 0, 0], S, 1));
    v.push(c("Caravan", Vehicle, [1, 1, 0, 1, 0], [0, 0, 0, 1, 0], G, 1));
    v.push(c("Frontier Rover", Vehicle, [2, 1, 0, 0, 2], [0, 0, 0, 0, 2], X, 2).chars(0, 1));
    v.push(c("Orbital Shuttle", Vehicle, [3, 4, 2, 0, 0], [0, 1, 0, 0, 0], E, 4));
    v.push(c("Sky Armada", Vehicle, [4, 3, 0, 3, 0], [0; 5], E, 4).chars(1, 0).combo(Vehicle, 1));

    // Research: science, and the points that come from knowing things.
    v.push(c("Lab Bench", Research, [1, 0, 1, 0, 0], [0, 0, 1, 0, 0], S, 1));
    v.push(c("Observatory", Research, [1, 1, 2, 0, 0], [0, 0, 2, 0, 0], S, 1));
    v.push(c("Think Tank", Research, [0, 1, 3, 0, 0], [0, 0, 2, 0, 0], S, 2));
    v.push(c("Particle Ring", Research, [2, 3, 2, 0, 0], [0, 1, 1, 0, 0], E, 2));
    v.push(c("Library Annex", Research, [1, 0, 2, 1, 0], [0, 0, 1, 1, 0], G, 1));
    v.push(c("Field Station", Research, [1, 0, 1, 0, 2], [0, 0, 1, 0, 1], X, 1));
    v.push(c("Quantum Core", Research, [1, 2, 4, 0, 0], [0, 0, 3, 0, 0], S, 2));
    v.push(c("Academy", Research, [2, 0, 3, 2, 0], [0; 5], G, 2).chars(0, 1).combo(Research, 1));
    v.push(c("Genome Bank", Research, [0, 0, 4, 1, 0], [0; 5], S, 1).combo(Discovery, 2));
    v.push(c("Material Lab", Research, [3, 0, 2, 0, 0], [1, 0, 1, 0, 0], M, 1));
    v.push(c("Prototype Works", Research, [2, 1, 2, 0, 0], [0; 5], S, 1).scaled(S, Research));
    v.push(c("Scholar Guild", Research, [0, 0, 3, 2, 0], [0, 0, 1, 0, 0], G, 2).chars(0, 1));
    v.push(c("Fusion Study", Research, [1, 4, 3, 0, 0], [0, 2, 1, 0, 0], E, 2).bonus(Krystallium));
    v.push(c("Grand Theory", Research, [0, 1, 5, 2, 0], [0; 5], S, 5));
    v.push(c("Mind Engine", Research, [2, 2, 4, 0, 0], [0, 0, 1, 0, 0], S, 3).chars(1, 0).combo(Research, 1));

    // Projects: little production, lots of points.
    v.push(c("Aqueduct", Project, [2, 0, 0, 1, 0], [1, 0, 0, 0, 0], M, 2));
    v.push(c("Opera House", Project, [1, 0, 1, 3, 0], [0; 5], G, 4));
    v.push(c("Sky Garden", Project, [2, 1, 1, 1, 0], [0, 0, 0, 1, 0], S, 3));
    v.push(c("Peace Treaty", Project, [0, 0, 2, 2, 0], [0; 5], S, 4).chars(1, 1));
    v.push(c("Grand Canal", Project, [4, 1, 0, 1, 1], [1, 0, 0, 0, 0], M, 5));
    v.push(c("World Fair", Project, [1, 1, 1, 2, 1], [0; 5], X, 3).combo(Discovery, 1));
    v.push(c("Trade Pact", Project, [0, 0, 1, 3, 0], [0; 5], G, 2).chars(0, 1).per_token(Token::Financier, 1));
    v.push(c("Defense Grid", Project, [2, 2, 0, 0, 0], [0; 5], E, 3).chars(2, 0).per_token(Token::General, 1));
    v.push(c("Cultural Wave", Project, [0, 0, 2, 2, 2], [0; 5], X, 3).combo(Structure, 1));
    v.push(c("Megacity", Project, [5, 2, 1, 3, 0], [0; 5], M, 8));
    v.push(c("Space Elevator", Project, [3, 3, 2, 0, 3], [0, 0, 0, 0, 1], E, 7));
    v.push(c("Founding Charter", Project, [1, 1, 1, 1, 1], [0; 5], G, 3).bonus(general));
    v.push(c("Utopia Plan", Project, [3, 2, 3, 3, 2], [0; 5], S, 10));
    v.push(c("Global Network", Project, [2, 2, 2, 2, 0], [0; 5], E, 2).combo(Vehicle, 1));
    v.push(c("Legacy Hall", Project, [2, 0, 2, 2, 0], [0; 5], G, 2).combo(Project, 2));

    // Discoveries: exploration, characters and surprises.
    v.push(c("Lost Ruins", Discovery, [0, 0, 0, 0, 2], [0, 0, 0, 0, 1], X, 1));
    v.push(c("Spice Route", Discovery, [0, 0, 0, 1, 2], [0, 0, 0, 1, 1], G, 1));
    v.push(c("Crystal Cave", Discovery, [0, 0, 1, 0, 3], [0, 0, 0, 0, 1], X, 1).bonus(Krystallium));
    v.push(c("Sunken Temple", Discovery, [1, 0, 2, 0, 3], [0; 5], S, 4));
    v.push(c("Alien Relic", Discovery, [0, 1, 3, 0, 2], [0, 0, 1, 0, 1], S, 3));
    v.push(c("Jungle Outpost", Discovery, [1, 0, 0, 0, 2], [1, 0, 0, 0, 1], M, 1));
    v.push(c("Polar Station", Discovery, [1, 2, 1, 0, 2], [0, 1, 0, 0, 1], E, 2).chars(1, 0));
    v.push(c("Cartographers' Guild", Discovery, [0, 0, 1, 1, 3], [0; 5], X, 2).chars(0, 1).combo(Discovery, 1));
    v.push(c("Meteor Mine", Discovery, [2, 1, 0, 0, 3], [2, 0, 0, 0, 0], M, 2).bonus(Cube(M)));
    v.push(c("Star Map", Discovery, [0, 0, 2, 0, 2], [0, 0, 0, 0, 2], X, 1));
    v.push(c("Lunar Colony", Discovery, [3, 2, 1, 0, 4], [0, 0, 0, 1, 0], M, 5).chars(1, 0));
    v.push(c("Ancient Archive", Discovery, [0, 0, 3, 0, 3], [0, 0, 1, 0, 0], S, 2).bonus(financier));
    v.push(c("Gold Rush", Discovery, [0, 1, 0, 1, 3], [0, 0, 0, 2, 0], G, 1));
    v.push(c("Frontier Beacon", Discovery, [1, 1, 1, 1, 3], [0; 5], X, 3).scaled(X, Discovery));
    v.push(c("First Contact", Discovery, [0, 2, 3, 1, 4], [0; 5], X, 6).chars(0, 1));

    debug_assert_eq!(v.len(), DESIGNS);
    v
}
