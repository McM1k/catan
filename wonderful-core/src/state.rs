//! The game state and its rules.
//!
//! A game is four rounds. Every round has three phases in which **all
//! players act at the same time**:
//!
//! 1. **Draft.** Everybody is dealt a hand (7 cards, 10 with two players),
//!    secretly picks one card, then passes the rest on: to the next seat in
//!    rounds 1 and 3, to the previous one in rounds 2 and 4. Once every pick
//!    is in, the new hands are looked at and the next pick starts, until
//!    each player holds 7 cards. With two players the 3 cards left over are
//!    discarded.
//! 2. **Planning.** Each drafted card is either put under construction or
//!    recycled: it is discarded and its recycling bonus, a cube, goes onto a
//!    building or onto the Empire straight away. A building whose spaces are
//!    all filled is finished at once.
//! 3. **Production.** Five steps, one per resource (Materials, Energy,
//!    Science, Gold, Exploration). In each, everybody produces as many cubes
//!    as they have icons for that resource, the player who produced the most
//!    (and strictly more than everyone else) takes the matching character,
//!    and the cubes are placed on buildings or the Empire; cubes that are
//!    not placed are lost. A sixth, **wrap-up** step follows, in which
//!    Krystallium and characters that are still held can be placed.
//!
//! After the fourth round the victory points are counted.
//!
//! The rules are pure: no networking, no randomness except the seed that
//! shuffles the deck.

use crate::cards::{Bonus, CardId, Cost, Kind, Res, Token, DECK_SIZE, EMPIRES};
use serde::{Deserialize, Serialize};

pub const MIN_PLAYERS: usize = 2;
pub const MAX_PLAYERS: usize = 5;
pub const ROUNDS: u8 = 4;
/// Cards each player ends the draft with.
pub const DRAFT_PICKS: usize = 7;
/// The production step after Exploration, for held Krystallium and characters.
pub const WRAP_UP: u8 = 5;
/// How many events [`State::log`] keeps.
const LOG_LEN: usize = 24;
/// Empire cubes that turn into one Krystallium.
const CUBES_PER_KRYSTALLIUM: u8 = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    Draft,
    Planning,
    /// `step` 0 to 4 are the resources in [`Res::ALL`] order, [`WRAP_UP`] the last one.
    Production { step: u8 },
    Over,
}

/// Where a cube goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Target {
    /// A building: a card under construction.
    Card(CardId),
    /// The Empire: five cubes there make a Krystallium.
    Empire,
}

/// Something a player holds and can place on a building.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Piece {
    Cube(Res),
    /// Krystallium standing in for a cube of this resource.
    Krystallium(Res),
    General,
    Financier,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    /// Draft: take this card from the hand you hold.
    Draft { card: CardId },
    /// Planning: put a drafted card under construction.
    Build { card: CardId },
    /// Planning: recycle a drafted card; its cube goes to `to`.
    Recycle { card: CardId, to: Target },
    /// Planning or production: give up a building. What was placed on it is
    /// lost; its recycling cube goes to `to`.
    Scrap { card: CardId, to: Target },
    /// Planning or production: place something you hold.
    Place { piece: Piece, target: Target },
    /// Production, Science: pick the character the supremacy bonus gives.
    Choose { token: Token },
    /// Planning: no more to decide. Production: done with this step.
    Ready,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    GameOver,
    UnknownSeat,
    WrongPhase,
    AlreadyPicked,
    NotInHand,
    NotDrafted,
    NotUnderConstruction,
    InvalidTarget,
    NothingToPlace,
    CardsLeftToPlan,
    MustChoose,
    NothingToChoose,
}

impl Error {
    /// What the error means, in words for the player.
    pub fn message(self) -> &'static str {
        match self {
            Error::GameOver => "The game is over.",
            Error::UnknownSeat => "You aren't seated in this game.",
            Error::WrongPhase => "You can't do that right now.",
            Error::AlreadyPicked => "You already picked a card: wait for the others.",
            Error::NotInHand => "That card isn't in your hand.",
            Error::NotDrafted => "That card isn't one of your drafted cards.",
            Error::NotUnderConstruction => "That card isn't under construction.",
            Error::InvalidTarget => "It can't go there: that building has no free space for it.",
            Error::NothingToPlace => "You don't have that to place.",
            Error::CardsLeftToPlan => "Decide what to do with all your drafted cards first.",
            Error::MustChoose => "Choose your General or Financier first.",
            Error::NothingToChoose => "There's nothing to choose.",
        }
    }
}

/// Something that happened, kept (briefly) for the log. Seats are indices.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Event {
    RoundStarted(u8),
    /// A building was finished.
    Completed { seat: usize, card: CardId },
    /// Somebody produced the most of `res` (`Some`), or the best were tied (`None`).
    Supremacy { res: Res, seat: Option<usize> },
    Chose { seat: usize, token: Token },
}

/// A card under construction, with what has been placed on it so far.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Building {
    pub card: CardId,
    pub filled: Cost,
}

impl Building {
    /// What is still missing.
    pub fn remaining(&self) -> Cost {
        self.card.def().cost.minus(&self.filled)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerState {
    /// The hand being drafted from (what you can pick from right now).
    pub hand: Vec<CardId>,
    /// The card picked in this draft step, not yet revealed to the others.
    pub picked: Option<CardId>,
    /// The draft area: cards kept, waiting for the planning phase.
    pub drafted: Vec<CardId>,
    pub buildings: Vec<Building>,
    /// Finished cards.
    pub empire: Vec<CardId>,
    /// Cubes on the Empire card, fewer than five.
    pub empire_cubes: u8,
    pub krystallium: u8,
    pub generals: u8,
    pub financiers: u8,
    /// Cubes from construction bonuses, to be placed this round. Cubes still
    /// here when the round ends go to the Empire.
    pub pending: [u8; 5],
    /// Cubes produced in the current step and not placed yet.
    pub pool: u8,
    /// What was produced in the current step.
    pub produced: u8,
    /// Planning: done planning. Production: done with this step.
    pub ready: bool,
    /// Won the Science supremacy and has to pick a character.
    pub choose: bool,
}

/// How a player's points add up.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Score {
    /// Points printed on cards, whatever else you own.
    pub gross: u32,
    /// Points that depend on card types or characters.
    pub combo: u32,
    /// One point per General held.
    pub generals: u32,
    /// One point per Financier held.
    pub financiers: u32,
    pub total: u32,
    /// Finished cards, the first tie-break.
    pub cards: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct State {
    pub players: Vec<PlayerState>,
    pub deck: Vec<CardId>,
    pub discard: Vec<CardId>,
    /// 1 to [`ROUNDS`].
    pub round: u8,
    pub phase: Phase,
    /// The latest events, oldest first.
    pub log: Vec<Event>,
    /// Filled in when the game is over: one entry per seat.
    pub scores: Vec<Score>,
}

/// Cards in each hand at the start of a round.
pub fn hand_size(players: usize) -> usize {
    if players == 2 {
        10
    } else {
        DRAFT_PICKS
    }
}

/// SplitMix64: a tiny seeded generator, so the rules need no `rand`.
struct SplitMix(u64);

impl SplitMix {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

fn shuffle(cards: &mut [CardId], seed: u64) {
    let mut rng = SplitMix(seed);
    for i in (1..cards.len()).rev() {
        let j = (rng.next() % (i as u64 + 1)) as usize;
        cards.swap(i, j);
    }
}

impl State {
    /// A new game for 2 to 5 players. The same seed gives the same deck.
    pub fn new(players: usize, seed: u64) -> State {
        assert!(
            (MIN_PLAYERS..=MAX_PLAYERS).contains(&players),
            "the game is for {MIN_PLAYERS} to {MAX_PLAYERS} players"
        );
        let mut deck: Vec<CardId> = (0..DECK_SIZE as u16).map(CardId).collect();
        shuffle(&mut deck, seed);
        let mut state = State {
            players: vec![PlayerState::default(); players],
            deck,
            discard: Vec::new(),
            round: 1,
            phase: Phase::Draft,
            log: Vec::new(),
            scores: Vec::new(),
        };
        state.start_round();
        state
    }

    pub fn seats(&self) -> usize {
        self.players.len()
    }

    /// Whether this round's hands go to the next seat (rounds 1 and 3) or
    /// the previous one (rounds 2 and 4).
    pub fn passes_to_next(&self) -> bool {
        self.round % 2 == 1
    }

    /// How many cubes of `res` the seat produces right now: the Empire's own
    /// icons plus those of every finished card.
    pub fn production(&self, seat: usize, res: Res) -> u8 {
        let p = &self.players[seat];
        let mut total = EMPIRES[seat % EMPIRES.len()].base[res.index()] as u32;
        for id in &p.empire {
            let card = id.def();
            total += card.produces[res.index()] as u32;
            if let Some((r, kind)) = card.scaled {
                if r == res {
                    total += count_kind(&p.empire, kind);
                }
            }
        }
        total.min(u8::MAX as u32) as u8
    }

    /// The seat's points as things stand.
    pub fn score_of(&self, seat: usize) -> Score {
        let p = &self.players[seat];
        let mut score = Score { cards: p.empire.len() as u32, ..Score::default() };
        for id in &p.empire {
            let card = id.def();
            score.gross += card.vp as u32;
            if let Some((kind, points)) = card.combo {
                score.combo += points as u32 * count_kind(&p.empire, kind);
            }
            if let Some((token, points)) = card.per_token {
                let held = match token {
                    Token::General => p.generals,
                    Token::Financier => p.financiers,
                };
                score.combo += points as u32 * held as u32;
            }
        }
        score.generals = p.generals as u32;
        score.financiers = p.financiers as u32;
        score.total = score.gross + score.combo + score.generals + score.financiers;
        score
    }

    /// The winners once the game is over: most points, then most finished
    /// cards, then most characters. Whoever is still level shares the win.
    pub fn winners(&self) -> Vec<usize> {
        if self.phase != Phase::Over || self.scores.is_empty() {
            return Vec::new();
        }
        let key = |seat: usize| {
            let s = &self.scores[seat];
            let p = &self.players[seat];
            (s.total, s.cards, p.generals as u32 + p.financiers as u32)
        };
        let best = (0..self.seats()).map(key).max().unwrap_or_default();
        (0..self.seats()).filter(|&s| key(s) == best).collect()
    }

    /// Applies one move by `seat`. Moves are checked and applied right away,
    /// whoever else is still deciding; when the last player of a step is
    /// done, the game moves on by itself.
    pub fn apply(&mut self, seat: usize, action: Action) -> Result<(), Error> {
        if seat >= self.seats() {
            return Err(Error::UnknownSeat);
        }
        if self.phase == Phase::Over {
            return Err(Error::GameOver);
        }
        match action {
            Action::Draft { card } => self.draft(seat, card)?,
            Action::Build { card } => self.build(seat, card)?,
            Action::Recycle { card, to } => self.recycle(seat, card, to)?,
            Action::Scrap { card, to } => self.scrap(seat, card, to)?,
            Action::Place { piece, target } => self.place(seat, piece, target)?,
            Action::Choose { token } => self.choose(seat, token)?,
            Action::Ready => self.ready(seat)?,
        }
        self.settle();
        Ok(())
    }

    // ----- draft -----------------------------------------------------

    fn start_round(&mut self) {
        let size = hand_size(self.seats());
        self.phase = Phase::Draft;
        for p in &mut self.players {
            let at = self.deck.len() - size;
            p.hand = self.deck.split_off(at);
            p.picked = None;
            p.drafted.clear();
            p.pool = 0;
            p.produced = 0;
            p.ready = false;
            p.choose = false;
        }
        self.push_event(Event::RoundStarted(self.round));
    }

    fn draft(&mut self, seat: usize, card: CardId) -> Result<(), Error> {
        if self.phase != Phase::Draft {
            return Err(Error::WrongPhase);
        }
        let p = &mut self.players[seat];
        if p.picked.is_some() {
            return Err(Error::AlreadyPicked);
        }
        let at = p.hand.iter().position(|&c| c == card).ok_or(Error::NotInHand)?;
        p.hand.remove(at);
        p.picked = Some(card);
        Ok(())
    }

    /// Everybody has picked: reveal the picks and pass the hands on.
    fn finish_draft_step(&mut self) {
        let n = self.seats();
        for p in &mut self.players {
            if let Some(card) = p.picked.take() {
                p.drafted.push(card);
            }
        }
        let next = self.passes_to_next();
        let mut passed: Vec<Vec<CardId>> = vec![Vec::new(); n];
        for (i, p) in self.players.iter_mut().enumerate() {
            let to = if next { (i + 1) % n } else { (i + n - 1) % n };
            passed[to] = std::mem::take(&mut p.hand);
        }
        for (p, hand) in self.players.iter_mut().zip(passed) {
            p.hand = hand;
        }
        if self.players.iter().all(|p| p.drafted.len() >= DRAFT_PICKS) {
            // Whatever is left (3 cards each with two players) is discarded
            // without its recycling bonus.
            for p in &mut self.players {
                self.discard.append(&mut p.hand);
                p.ready = false;
            }
            self.phase = Phase::Planning;
        }
    }

    // ----- planning and construction ---------------------------------

    fn build(&mut self, seat: usize, card: CardId) -> Result<(), Error> {
        if self.phase != Phase::Planning {
            return Err(Error::WrongPhase);
        }
        let p = &mut self.players[seat];
        let at = p.drafted.iter().position(|&c| c == card).ok_or(Error::NotDrafted)?;
        p.drafted.remove(at);
        p.buildings.push(Building { card, filled: Cost::default() });
        Ok(())
    }

    fn recycle(&mut self, seat: usize, card: CardId, to: Target) -> Result<(), Error> {
        if self.phase != Phase::Planning {
            return Err(Error::WrongPhase);
        }
        let at = self.players[seat]
            .drafted
            .iter()
            .position(|&c| c == card)
            .ok_or(Error::NotDrafted)?;
        let res = card.def().recycle;
        self.check_cube_target(seat, res, to, None)?;
        self.players[seat].drafted.remove(at);
        self.discard.push(card);
        self.place_cube(seat, res, to);
        Ok(())
    }

    fn scrap(&mut self, seat: usize, card: CardId, to: Target) -> Result<(), Error> {
        if !matches!(self.phase, Phase::Planning | Phase::Production { .. }) {
            return Err(Error::WrongPhase);
        }
        let at = self.players[seat]
            .buildings
            .iter()
            .position(|b| b.card == card)
            .ok_or(Error::NotUnderConstruction)?;
        let res = card.def().recycle;
        self.check_cube_target(seat, res, to, Some(card))?;
        self.players[seat].buildings.remove(at);
        self.discard.push(card);
        self.place_cube(seat, res, to);
        Ok(())
    }

    /// Whether a cube of `res` can go to `to`. `exclude` is a building that
    /// is about to disappear.
    fn check_cube_target(&self, seat: usize, res: Res, to: Target, exclude: Option<CardId>) -> Result<(), Error> {
        match to {
            Target::Empire => Ok(()),
            Target::Card(id) => {
                if exclude == Some(id) {
                    return Err(Error::InvalidTarget);
                }
                let b = self.players[seat]
                    .buildings
                    .iter()
                    .find(|b| b.card == id)
                    .ok_or(Error::InvalidTarget)?;
                if b.remaining().res[res.index()] > 0 {
                    Ok(())
                } else {
                    Err(Error::InvalidTarget)
                }
            }
        }
    }

    /// Puts a cube where `check_cube_target` said it can go.
    fn place_cube(&mut self, seat: usize, res: Res, to: Target) {
        match to {
            Target::Empire => self.add_empire_cube(seat),
            Target::Card(id) => {
                if let Some(b) = self.players[seat].buildings.iter_mut().find(|b| b.card == id) {
                    b.filled.res[res.index()] += 1;
                }
                self.complete_if_done(seat, id);
            }
        }
    }

    fn add_empire_cube(&mut self, seat: usize) {
        let p = &mut self.players[seat];
        p.empire_cubes += 1;
        if p.empire_cubes >= CUBES_PER_KRYSTALLIUM {
            p.empire_cubes -= CUBES_PER_KRYSTALLIUM;
            p.krystallium += 1;
        }
    }

    /// Finishes the building if every space is filled: the card joins the
    /// Empire and its construction bonus is paid out.
    fn complete_if_done(&mut self, seat: usize, id: CardId) {
        let p = &mut self.players[seat];
        let Some(at) = p.buildings.iter().position(|b| b.card == id) else {
            return;
        };
        if !p.buildings[at].remaining().is_zero() {
            return;
        }
        p.buildings.remove(at);
        p.empire.push(id);
        match id.def().bonus {
            Some(Bonus::Cube(res)) => p.pending[res.index()] += 1,
            Some(Bonus::Krystallium) => p.krystallium += 1,
            Some(Bonus::Token(Token::General)) => p.generals += 1,
            Some(Bonus::Token(Token::Financier)) => p.financiers += 1,
            None => {}
        }
        self.push_event(Event::Completed { seat, card: id });
    }

    fn place(&mut self, seat: usize, piece: Piece, target: Target) -> Result<(), Error> {
        match self.phase {
            Phase::Planning | Phase::Production { .. } => {}
            _ => return Err(Error::WrongPhase),
        }
        match piece {
            Piece::Cube(res) => {
                // Cubes produced in this step come first, then bonus cubes.
                let from_pool = matches!(self.phase, Phase::Production { step }
                    if (step as usize) < Res::ALL.len() && Res::ALL[step as usize] == res)
                    && self.players[seat].pool > 0;
                if !from_pool && self.players[seat].pending[res.index()] == 0 {
                    return Err(Error::NothingToPlace);
                }
                self.check_cube_target(seat, res, target, None)?;
                let p = &mut self.players[seat];
                if from_pool {
                    p.pool -= 1;
                    // Everything placed: nothing left to wait for in this step.
                    if p.pool == 0 && !p.choose {
                        p.ready = true;
                    }
                } else {
                    p.pending[res.index()] -= 1;
                }
                self.place_cube(seat, res, target);
                Ok(())
            }
            Piece::Krystallium(res) => {
                let id = self.building_with_room(seat, target, |c| c.res[res.index()] > 0)?;
                let p = &mut self.players[seat];
                if p.krystallium == 0 {
                    return Err(Error::NothingToPlace);
                }
                p.krystallium -= 1;
                self.fill(seat, id, |f| f.res[res.index()] += 1);
                Ok(())
            }
            Piece::General => {
                let id = self.building_with_room(seat, target, |c| c.generals > 0)?;
                let p = &mut self.players[seat];
                if p.generals == 0 {
                    return Err(Error::NothingToPlace);
                }
                p.generals -= 1;
                self.fill(seat, id, |f| f.generals += 1);
                Ok(())
            }
            Piece::Financier => {
                let id = self.building_with_room(seat, target, |c| c.financiers > 0)?;
                let p = &mut self.players[seat];
                if p.financiers == 0 {
                    return Err(Error::NothingToPlace);
                }
                p.financiers -= 1;
                self.fill(seat, id, |f| f.financiers += 1);
                Ok(())
            }
        }
    }

    /// The building `target` names, if it still has a space `wanted` accepts.
    fn building_with_room(&self, seat: usize, target: Target, wanted: impl Fn(&Cost) -> bool) -> Result<CardId, Error> {
        let Target::Card(id) = target else {
            return Err(Error::InvalidTarget);
        };
        let b = self.players[seat]
            .buildings
            .iter()
            .find(|b| b.card == id)
            .ok_or(Error::InvalidTarget)?;
        if wanted(&b.remaining()) {
            Ok(id)
        } else {
            Err(Error::InvalidTarget)
        }
    }

    fn fill(&mut self, seat: usize, id: CardId, add: impl FnOnce(&mut Cost)) {
        if let Some(b) = self.players[seat].buildings.iter_mut().find(|b| b.card == id) {
            add(&mut b.filled);
        }
        self.complete_if_done(seat, id);
    }

    // ----- production --------------------------------------------------

    fn choose(&mut self, seat: usize, token: Token) -> Result<(), Error> {
        if !matches!(self.phase, Phase::Production { .. }) {
            return Err(Error::WrongPhase);
        }
        let p = &mut self.players[seat];
        if !p.choose {
            return Err(Error::NothingToChoose);
        }
        p.choose = false;
        match token {
            Token::General => p.generals += 1,
            Token::Financier => p.financiers += 1,
        }
        if p.pool == 0 {
            p.ready = true;
        }
        self.push_event(Event::Chose { seat, token });
        Ok(())
    }

    fn ready(&mut self, seat: usize) -> Result<(), Error> {
        match self.phase {
            Phase::Planning => {
                if !self.players[seat].drafted.is_empty() {
                    return Err(Error::CardsLeftToPlan);
                }
                self.players[seat].ready = true;
                Ok(())
            }
            Phase::Production { .. } => {
                if self.players[seat].choose {
                    return Err(Error::MustChoose);
                }
                self.players[seat].ready = true;
                Ok(())
            }
            _ => Err(Error::WrongPhase),
        }
    }

    /// Moves the game on for as long as every player is done with a step.
    fn settle(&mut self) {
        loop {
            let everyone_done = self.players.iter().all(|p| p.ready);
            let everyone_picked = self.players.iter().all(|p| p.picked.is_some());
            match self.phase {
                Phase::Draft if everyone_picked => self.finish_draft_step(),
                Phase::Planning if everyone_done => self.start_step(0),
                Phase::Production { step } if everyone_done => {
                    if step < WRAP_UP {
                        self.start_step(step + 1);
                    } else {
                        self.end_round();
                    }
                }
                _ => break,
            }
        }
    }

    fn start_step(&mut self, step: u8) {
        self.phase = Phase::Production { step };
        let n = self.seats();
        if (step as usize) < Res::ALL.len() {
            let res = Res::ALL[step as usize];
            let made: Vec<u8> = (0..n).map(|i| self.production(i, res)).collect();
            let most = made.iter().copied().max().unwrap_or(0);
            let best: Vec<usize> = (0..n).filter(|&i| made[i] == most).collect();
            for (p, &amount) in self.players.iter_mut().zip(&made) {
                p.produced = amount;
                p.pool = amount;
                p.choose = false;
            }
            if most > 0 {
                // Tied for the most: nobody gets the character.
                let winner = (best.len() == 1).then(|| best[0]);
                if let Some(w) = winner {
                    match res.supremacy_token() {
                        Some(Token::General) => self.players[w].generals += 1,
                        Some(Token::Financier) => self.players[w].financiers += 1,
                        None => self.players[w].choose = true,
                    }
                }
                self.push_event(Event::Supremacy { res, seat: winner });
            }
            for p in &mut self.players {
                p.ready = p.pool == 0 && !p.choose;
            }
        } else {
            for i in 0..n {
                let can_act = self.can_wrap_up(i);
                let p = &mut self.players[i];
                p.pool = 0;
                p.produced = 0;
                p.choose = false;
                p.ready = !can_act;
            }
        }
    }

    /// Whether the seat holds Krystallium or a character that fits a space
    /// of one of its buildings.
    fn can_wrap_up(&self, seat: usize) -> bool {
        let p = &self.players[seat];
        p.buildings.iter().any(|b| {
            let left = b.remaining();
            (p.krystallium > 0 && left.res.iter().any(|&n| n > 0))
                || (p.generals > 0 && left.generals > 0)
                || (p.financiers > 0 && left.financiers > 0)
        })
    }

    fn end_round(&mut self) {
        for seat in 0..self.seats() {
            // Bonus cubes nobody placed go to the Empire.
            for res in 0..Res::ALL.len() {
                while self.players[seat].pending[res] > 0 {
                    self.players[seat].pending[res] -= 1;
                    self.add_empire_cube(seat);
                }
            }
            let p = &mut self.players[seat];
            p.pool = 0;
            p.produced = 0;
            p.ready = false;
        }
        if self.round >= ROUNDS {
            self.scores = (0..self.seats()).map(|s| self.score_of(s)).collect();
            self.phase = Phase::Over;
        } else {
            self.round += 1;
            self.start_round();
        }
    }

    fn push_event(&mut self, event: Event) {
        self.log.push(event);
        if self.log.len() > LOG_LEN {
            let extra = self.log.len() - LOG_LEN;
            self.log.drain(..extra);
        }
    }
}

fn count_kind(cards: &[CardId], kind: Kind) -> u32 {
    cards.iter().filter(|id| id.def().kind == kind).count() as u32
}
