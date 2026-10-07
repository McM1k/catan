use crate::{Board, Hand, Resource};
use rand::seq::SliceRandom;
use rand::Rng;
use serde::{Deserialize, Serialize};

pub const MAX_ROADS: usize = 15;
pub const MAX_SETTLEMENTS: usize = 5;
pub const MAX_CITIES: usize = 4;

/// All the numbers that can be tweaked without touching game logic.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Rules {
    pub victory_points: u8,
    /// A player holding more than this many cards must discard half on a 7.
    pub discard_above: u32,
    /// Default bank trade ratio (ports improve it).
    pub bank_ratio: u8,
    pub longest_road_min: u8,
    pub largest_army_min: u8,
}

impl Default for Rules {
    fn default() -> Self {
        Rules {
            victory_points: 10,
            discard_above: 7,
            bank_ratio: 4,
            longest_road_min: 5,
            largest_army_min: 3,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DevCard {
    Knight,
    VictoryPoint,
    RoadBuilding,
    YearOfPlenty,
    Monopoly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Building {
    pub owner: usize,
    pub city: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SetupExpect {
    Settlement,
    Road,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    Setup { step: usize, expect: SetupExpect },
    Roll,
    Main,
    Discard { pending: Vec<(usize, u8)> },
    MoveRobber { back_to_roll: bool },
    RoadBuilding { left: u8 },
    Finished { winner: usize },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Player {
    pub name: String,
    pub hand: Hand,
    pub dev: Vec<DevCard>,
    /// Bought this turn: cannot be played until next turn.
    pub new_dev: Vec<DevCard>,
    pub knights: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Trade {
    pub from: usize,
    /// What `from` hands over.
    pub give: Hand,
    /// What `from` wants in return.
    pub want: Hand,
    pub accepted: Vec<usize>,
    pub declined: Vec<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Action {
    Roll,
    EndTurn,
    BuildRoad { edge: usize },
    BuildSettlement { vertex: usize },
    BuildCity { vertex: usize },
    BankTrade { give: Resource, get: Resource },
    ProposeTrade { give: Hand, want: Hand },
    RespondTrade { accept: bool },
    ConfirmTrade { with: usize },
    CancelTrade,
    BuyDevCard,
    PlayKnight,
    PlayRoadBuilding,
    PlayYearOfPlenty(Resource, Resource),
    PlayMonopoly(Resource),
    Discard(Hand),
    MoveRobber { tile: usize, victim: Option<usize> },
}

#[derive(Clone, Debug)]
pub struct Game {
    pub rules: Rules,
    pub board: Board,
    pub players: Vec<Player>,
    pub phase: Phase,
    pub current: usize,
    pub robber: usize,
    pub dice: Option<(u8, u8)>,
    pub buildings: Vec<Option<Building>>,
    pub roads: Vec<Option<usize>>,
    pub bank: Hand,
    pub deck: Vec<DevCard>,
    pub trade: Option<Trade>,
    pub longest_road: Option<usize>,
    pub largest_army: Option<usize>,
    pub dev_played: bool,
    pub setup_vertex: Option<usize>,
    pub log: Vec<String>,
}

type Res = Result<(), String>;

fn err<T>(msg: &str) -> Result<T, String> {
    Err(msg.to_string())
}

impl Game {
    pub fn new(rules: Rules, names: Vec<String>, rng: &mut impl Rng) -> Game {
        assert!((2..=4).contains(&names.len()), "2 to 4 players");
        let board = Board::generate(rng);
        let mut deck = Vec::new();
        for (card, n) in [
            (DevCard::Knight, 14),
            (DevCard::VictoryPoint, 5),
            (DevCard::RoadBuilding, 2),
            (DevCard::YearOfPlenty, 2),
            (DevCard::Monopoly, 2),
        ] {
            for _ in 0..n {
                deck.push(card);
            }
        }
        deck.shuffle(rng);
        let robber = board.desert();
        Game {
            rules,
            players: names
                .into_iter()
                .map(|name| Player {
                    name,
                    hand: Hand::default(),
                    dev: vec![],
                    new_dev: vec![],
                    knights: 0,
                })
                .collect(),
            phase: Phase::Setup {
                step: 0,
                expect: SetupExpect::Settlement,
            },
            current: 0,
            robber,
            dice: None,
            buildings: vec![None; board.vertices.len()],
            roads: vec![None; board.edges.len()],
            bank: Hand([19; 5]),
            deck,
            trade: None,
            longest_road: None,
            largest_army: None,
            dev_played: false,
            setup_vertex: None,
            log: vec!["The game begins. Place your first settlement.".into()],
            board,
        }
    }

    // ---------------------------------------------------------------- helpers

    fn say(&mut self, s: String) {
        self.log.push(s);
        if self.log.len() > 80 {
            self.log.remove(0);
        }
    }

    fn name(&self, seat: usize) -> String {
        self.players[seat].name.clone()
    }

    fn setup_seat(&self, step: usize) -> usize {
        let n = self.players.len();
        if step < n {
            step
        } else {
            2 * n - 1 - step
        }
    }

    pub fn count_settlements(&self, seat: usize) -> usize {
        self.buildings
            .iter()
            .filter(|b| matches!(b, Some(b) if b.owner == seat && !b.city))
            .count()
    }

    pub fn count_cities(&self, seat: usize) -> usize {
        self.buildings
            .iter()
            .filter(|b| matches!(b, Some(b) if b.owner == seat && b.city))
            .count()
    }

    pub fn count_roads(&self, seat: usize) -> usize {
        self.roads.iter().filter(|r| **r == Some(seat)).count()
    }

    pub fn public_vp(&self, seat: usize) -> u8 {
        let mut vp = self.count_settlements(seat) + 2 * self.count_cities(seat);
        if self.longest_road == Some(seat) {
            vp += 2;
        }
        if self.largest_army == Some(seat) {
            vp += 2;
        }
        vp as u8
    }

    pub fn vp(&self, seat: usize) -> u8 {
        let p = &self.players[seat];
        let hidden = p
            .dev
            .iter()
            .chain(p.new_dev.iter())
            .filter(|c| **c == DevCard::VictoryPoint)
            .count();
        self.public_vp(seat) + hidden as u8
    }

    /// Bank trade ratio `seat` gets when giving `r`.
    pub fn bank_ratio_for(&self, seat: usize, r: Resource) -> u8 {
        let ports = self.board.vertex_ports();
        let mut best = self.rules.bank_ratio;
        for (v, b) in self.buildings.iter().enumerate() {
            if matches!(b, Some(b) if b.owner == seat) {
                match ports[v] {
                    Some(None) => best = best.min(3),
                    Some(Some(pr)) if pr == r => best = best.min(2),
                    _ => {}
                }
            }
        }
        best
    }

    fn pay(&mut self, seat: usize, cost: &Hand) -> Res {
        if !self.players[seat].hand.contains(cost) {
            return err("You can't afford that.");
        }
        self.players[seat].hand.remove(cost);
        self.bank.insert(cost);
        Ok(())
    }

    // ------------------------------------------------------------- legality

    pub fn legal_settlements(&self, seat: usize, setup: bool) -> Vec<usize> {
        (0..self.board.vertices.len())
            .filter(|&v| {
                self.buildings[v].is_none()
                    && self.board.vertices[v]
                        .neighbors
                        .iter()
                        .all(|&n| self.buildings[n].is_none())
                    && (setup
                        || self.board.vertices[v]
                            .edges
                            .iter()
                            .any(|&e| self.roads[e] == Some(seat)))
            })
            .collect()
    }

    pub fn legal_cities(&self, seat: usize) -> Vec<usize> {
        (0..self.buildings.len())
            .filter(|&v| matches!(self.buildings[v], Some(b) if b.owner == seat && !b.city))
            .collect()
    }

    pub fn legal_roads(&self, seat: usize, only_at: Option<usize>) -> Vec<usize> {
        (0..self.board.edges.len())
            .filter(|&e| {
                if self.roads[e].is_some() {
                    return false;
                }
                let ed = &self.board.edges[e];
                if let Some(v) = only_at {
                    return ed.a == v || ed.b == v;
                }
                [ed.a, ed.b].iter().any(|&v| {
                    match self.buildings[v] {
                        Some(b) if b.owner == seat => true,
                        Some(_) => false, // blocked by an opponent
                        None => self.board.vertices[v]
                            .edges
                            .iter()
                            .any(|&o| self.roads[o] == Some(seat)),
                    }
                })
            })
            .collect()
    }

    // --------------------------------------------------------------- actions

    /// Applies `action` on behalf of `seat`. On error the game is unchanged.
    pub fn apply(&mut self, seat: usize, action: Action, rng: &mut impl Rng) -> Res {
        if seat >= self.players.len() {
            return err("Unknown player.");
        }
        if matches!(self.phase, Phase::Finished { .. }) {
            return err("The game is over.");
        }
        match action {
            Action::Discard(h) => self.discard(seat, h),
            Action::RespondTrade { accept } => self.respond_trade(seat, accept),
            a => {
                if seat != self.current {
                    return err("It's not your turn.");
                }
                let r = self.apply_current(seat, a, rng);
                if r.is_ok() {
                    self.after_action(seat);
                }
                r
            }
        }
    }

    fn apply_current(&mut self, seat: usize, action: Action, rng: &mut impl Rng) -> Res {
        match action {
            Action::Roll => {
                let d1 = rng.gen_range(1..=6);
                let d2 = rng.gen_range(1..=6);
                self.do_roll(d1, d2, rng)
            }
            Action::EndTurn => self.end_turn(),
            Action::BuildRoad { edge } => self.build_road(seat, edge),
            Action::BuildSettlement { vertex } => self.build_settlement(seat, vertex),
            Action::BuildCity { vertex } => self.build_city(seat, vertex),
            Action::BankTrade { give, get } => self.bank_trade(seat, give, get),
            Action::ProposeTrade { give, want } => self.propose_trade(seat, give, want),
            Action::ConfirmTrade { with } => self.confirm_trade(seat, with),
            Action::CancelTrade => {
                self.trade = None;
                Ok(())
            }
            Action::BuyDevCard => self.buy_dev(seat, rng),
            Action::PlayKnight => self.play_knight(seat),
            Action::PlayRoadBuilding => self.play_road_building(seat),
            Action::PlayYearOfPlenty(a, b) => self.play_year_of_plenty(seat, a, b),
            Action::PlayMonopoly(r) => self.play_monopoly(seat, r),
            Action::MoveRobber { tile, victim } => self.move_robber(seat, tile, victim, rng),
            Action::Discard(_) | Action::RespondTrade { .. } => unreachable!(),
        }
    }

    fn after_action(&mut self, actor: usize) {
        self.update_longest_road();
        self.update_largest_army();
        if !matches!(self.phase, Phase::Finished { .. }) && self.vp(actor) >= self.rules.victory_points {
            let n = self.name(actor);
            self.say(format!("{n} wins the game with {} points!", self.vp(actor)));
            self.phase = Phase::Finished { winner: actor };
        }
    }

    /// Rolls the given dice (exposed for tests and deterministic replays).
    pub fn do_roll(&mut self, d1: u8, d2: u8, rng: &mut impl Rng) -> Res {
        let _ = rng;
        if self.phase != Phase::Roll {
            return err("You can't roll right now.");
        }
        self.dice = Some((d1, d2));
        let total = d1 + d2;
        let n = self.name(self.current);
        self.say(format!("{n} rolled {total} ({d1}+{d2})."));
        if total == 7 {
            let pending: Vec<(usize, u8)> = self
                .players
                .iter()
                .enumerate()
                .filter(|(_, p)| p.hand.total() > self.rules.discard_above)
                .map(|(i, p)| (i, (p.hand.total() / 2) as u8))
                .collect();
            self.phase = if pending.is_empty() {
                Phase::MoveRobber { back_to_roll: false }
            } else {
                Phase::Discard { pending }
            };
        } else {
            self.produce(total);
            self.phase = Phase::Main;
        }
        Ok(())
    }

    fn produce(&mut self, number: u8) {
        let n = self.players.len();
        let mut gains = vec![Hand::default(); n];
        for (ti, tile) in self.board.tiles.iter().enumerate() {
            if tile.number != Some(number) || ti == self.robber {
                continue;
            }
            let Some(res) = tile.terrain.resource() else { continue };
            for &v in &tile.vertices {
                if let Some(b) = self.buildings[v] {
                    gains[b.owner].add(res, if b.city { 2 } else { 1 });
                }
            }
        }
        for res in Resource::ALL {
            let demand: u32 = gains.iter().map(|g| g.get(res) as u32).sum();
            let have = self.bank.get(res) as u32;
            let takers = gains.iter().filter(|g| g.get(res) > 0).count();
            if demand > have {
                if takers == 1 {
                    let p = gains.iter().position(|g| g.get(res) > 0).unwrap();
                    gains[p].0[res.idx()] = have as u8;
                } else {
                    for g in gains.iter_mut() {
                        g.0[res.idx()] = 0;
                    }
                    self.say(format!("The bank doesn't have enough {} for everyone.", res.name()));
                }
            }
        }
        for (seat, g) in gains.iter().enumerate() {
            if g.is_empty() {
                continue;
            }
            self.bank.remove(g);
            self.players[seat].hand.insert(g);
            let n = self.name(seat);
            self.say(format!("{n} collects {}.", describe(g)));
        }
    }

    fn discard(&mut self, seat: usize, h: Hand) -> Res {
        let Phase::Discard { pending } = &self.phase else {
            return err("Nobody needs to discard right now.");
        };
        let Some(&(_, need)) = pending.iter().find(|(s, _)| *s == seat) else {
            return err("You don't need to discard.");
        };
        if h.total() != need as u32 {
            return Err(format!("You must discard exactly {need} cards."));
        }
        if !self.players[seat].hand.contains(&h) {
            return err("You don't have those cards.");
        }
        self.players[seat].hand.remove(&h);
        self.bank.insert(&h);
        let n = self.name(seat);
        self.say(format!("{n} discards {need} cards."));
        if let Phase::Discard { pending } = &mut self.phase {
            pending.retain(|(s, _)| *s != seat);
            if pending.is_empty() {
                self.phase = Phase::MoveRobber { back_to_roll: false };
            }
        }
        Ok(())
    }

    fn move_robber(&mut self, seat: usize, tile: usize, victim: Option<usize>, rng: &mut impl Rng) -> Res {
        let Phase::MoveRobber { back_to_roll } = self.phase else {
            return err("You can't move the robber right now.");
        };
        if tile >= self.board.tiles.len() || tile == self.robber {
            return err("The robber must move to a different tile.");
        }
        let mut adjacent: Vec<usize> = Vec::new();
        for &v in &self.board.tiles[tile].vertices {
            if let Some(b) = self.buildings[v] {
                if b.owner != seat && !adjacent.contains(&b.owner) && self.players[b.owner].hand.total() > 0 {
                    adjacent.push(b.owner);
                }
            }
        }
        let target = match (victim, adjacent.len()) {
            (_, 0) => {
                if victim.is_some() {
                    return err("There is nobody to steal from on that tile.");
                }
                None
            }
            (None, 1) => Some(adjacent[0]),
            (None, _) => return err("Choose a player to steal from."),
            (Some(v), _) if adjacent.contains(&v) => Some(v),
            _ => return err("You can't steal from that player."),
        };
        self.robber = tile;
        let n = self.name(seat);
        match target {
            Some(v) => {
                let total = self.players[v].hand.total();
                let mut k = rng.gen_range(0..total);
                let mut stolen = Resource::Wood;
                for r in Resource::ALL {
                    let c = self.players[v].hand.get(r) as u32;
                    if k < c {
                        stolen = r;
                        break;
                    }
                    k -= c;
                }
                self.players[v].hand.0[stolen.idx()] -= 1;
                self.players[seat].hand.add(stolen, 1);
                let vn = self.name(v);
                self.say(format!("{n} moves the robber and steals a card from {vn}."));
            }
            None => self.say(format!("{n} moves the robber.")),
        }
        self.phase = if back_to_roll { Phase::Roll } else { Phase::Main };
        Ok(())
    }

    fn end_turn(&mut self) -> Res {
        if self.phase != Phase::Main {
            return err("You can't end your turn right now.");
        }
        let cur = self.current;
        let fresh = std::mem::take(&mut self.players[cur].new_dev);
        self.players[cur].dev.extend(fresh);
        self.dev_played = false;
        self.trade = None;
        self.dice = None;
        self.current = (cur + 1) % self.players.len();
        self.phase = Phase::Roll;
        let n = self.name(self.current);
        self.say(format!("It's {n}'s turn."));
        Ok(())
    }

    // -------------------------------------------------------------- building

    fn advance_setup(&mut self, step: usize) {
        let n = self.players.len();
        self.setup_vertex = None;
        if step + 1 >= 2 * n {
            self.current = 0;
            self.phase = Phase::Roll;
            let name = self.name(0);
            self.say(format!("Setup complete. {name} goes first."));
        } else {
            self.current = self.setup_seat(step + 1);
            self.phase = Phase::Setup {
                step: step + 1,
                expect: SetupExpect::Settlement,
            };
        }
    }

    fn build_road(&mut self, seat: usize, edge: usize) -> Res {
        if edge >= self.roads.len() {
            return err("No such edge.");
        }
        match self.phase.clone() {
            Phase::Setup { step, expect: SetupExpect::Road } => {
                if !self.legal_roads(seat, self.setup_vertex).contains(&edge) {
                    return err("Place the road next to the settlement you just built.");
                }
                self.roads[edge] = Some(seat);
                self.advance_setup(step);
                Ok(())
            }
            Phase::RoadBuilding { left } => {
                if !self.legal_roads(seat, None).contains(&edge) {
                    return err("You can't build a road there.");
                }
                self.roads[edge] = Some(seat);
                let left = left - 1;
                let more = left > 0
                    && self.count_roads(seat) < MAX_ROADS
                    && !self.legal_roads(seat, None).is_empty();
                self.phase = if more { Phase::RoadBuilding { left } } else { Phase::Main };
                Ok(())
            }
            Phase::Main => {
                if self.count_roads(seat) >= MAX_ROADS {
                    return err("You have no roads left.");
                }
                if !self.legal_roads(seat, None).contains(&edge) {
                    return err("You can't build a road there.");
                }
                self.pay(seat, &Hand::of(&[(Resource::Wood, 1), (Resource::Brick, 1)]))?;
                self.roads[edge] = Some(seat);
                let n = self.name(seat);
                self.say(format!("{n} builds a road."));
                Ok(())
            }
            _ => err("You can't build a road right now."),
        }
    }

    fn build_settlement(&mut self, seat: usize, vertex: usize) -> Res {
        if vertex >= self.buildings.len() {
            return err("No such vertex.");
        }
        match self.phase.clone() {
            Phase::Setup { step, expect: SetupExpect::Settlement } => {
                if !self.legal_settlements(seat, true).contains(&vertex) {
                    return err("You can't settle there (too close to another settlement).");
                }
                self.buildings[vertex] = Some(Building { owner: seat, city: false });
                self.setup_vertex = Some(vertex);
                if step >= self.players.len() {
                    // Second settlement pays out its neighbouring tiles.
                    let tiles = self.board.vertices[vertex].tiles.clone();
                    for ti in tiles {
                        if let Some(r) = self.board.tiles[ti].terrain.resource() {
                            if self.bank.get(r) > 0 {
                                self.bank.0[r.idx()] -= 1;
                                self.players[seat].hand.add(r, 1);
                            }
                        }
                    }
                }
                self.phase = Phase::Setup { step, expect: SetupExpect::Road };
                Ok(())
            }
            Phase::Main => {
                if self.count_settlements(seat) >= MAX_SETTLEMENTS {
                    return err("You have no settlements left.");
                }
                if !self.legal_settlements(seat, false).contains(&vertex) {
                    return err("You can't settle there.");
                }
                self.pay(
                    seat,
                    &Hand::of(&[
                        (Resource::Wood, 1),
                        (Resource::Brick, 1),
                        (Resource::Sheep, 1),
                        (Resource::Wheat, 1),
                    ]),
                )?;
                self.buildings[vertex] = Some(Building { owner: seat, city: false });
                let n = self.name(seat);
                self.say(format!("{n} builds a settlement."));
                Ok(())
            }
            _ => err("You can't build a settlement right now."),
        }
    }

    fn build_city(&mut self, seat: usize, vertex: usize) -> Res {
        if self.phase != Phase::Main {
            return err("You can't build a city right now.");
        }
        if vertex >= self.buildings.len() || !self.legal_cities(seat).contains(&vertex) {
            return err("You can only upgrade your own settlements.");
        }
        if self.count_cities(seat) >= MAX_CITIES {
            return err("You have no cities left.");
        }
        self.pay(seat, &Hand::of(&[(Resource::Wheat, 2), (Resource::Ore, 3)]))?;
        self.buildings[vertex] = Some(Building { owner: seat, city: true });
        let n = self.name(seat);
        self.say(format!("{n} upgrades a settlement to a city."));
        Ok(())
    }

    // --------------------------------------------------------------- trading

    fn bank_trade(&mut self, seat: usize, give: Resource, get: Resource) -> Res {
        if self.phase != Phase::Main {
            return err("You can only trade after rolling.");
        }
        if give == get {
            return err("Pick two different resources.");
        }
        let ratio = self.bank_ratio_for(seat, give);
        if self.players[seat].hand.get(give) < ratio {
            return Err(format!("You need {ratio} {} for that trade.", give.name()));
        }
        if self.bank.get(get) == 0 {
            return err("The bank is out of that resource.");
        }
        self.players[seat].hand.0[give.idx()] -= ratio;
        self.bank.0[give.idx()] += ratio;
        self.bank.0[get.idx()] -= 1;
        self.players[seat].hand.add(get, 1);
        let n = self.name(seat);
        self.say(format!("{n} trades {ratio} {} for 1 {} with the bank.", give.name(), get.name()));
        Ok(())
    }

    fn propose_trade(&mut self, seat: usize, give: Hand, want: Hand) -> Res {
        if self.phase != Phase::Main {
            return err("You can only trade after rolling.");
        }
        if give.is_empty() || want.is_empty() {
            return err("A trade needs cards on both sides.");
        }
        if give.overlaps(&want) {
            return err("You can't offer and request the same resource.");
        }
        if !self.players[seat].hand.contains(&give) {
            return err("You don't have those cards.");
        }
        let n = self.name(seat);
        self.say(format!("{n} offers {} for {}.", describe(&give), describe(&want)));
        self.trade = Some(Trade {
            from: seat,
            give,
            want,
            accepted: vec![],
            declined: vec![],
        });
        Ok(())
    }

    fn respond_trade(&mut self, seat: usize, accept: bool) -> Res {
        if self.phase != Phase::Main {
            return err("There is no trade to answer.");
        }
        let Some(t) = &mut self.trade else {
            return err("There is no trade to answer.");
        };
        if t.from == seat {
            return err("You can't answer your own offer.");
        }
        if accept {
            if !self.players[seat].hand.contains(&t.want) {
                return err("You don't have the cards requested.");
            }
            t.declined.retain(|&s| s != seat);
            if !t.accepted.contains(&seat) {
                t.accepted.push(seat);
            }
        } else {
            t.accepted.retain(|&s| s != seat);
            if !t.declined.contains(&seat) {
                t.declined.push(seat);
            }
        }
        Ok(())
    }

    fn confirm_trade(&mut self, seat: usize, with: usize) -> Res {
        let Some(t) = self.trade.clone() else {
            return err("There is no open trade.");
        };
        if t.from != seat || !t.accepted.contains(&with) {
            return err("That player hasn't accepted your offer.");
        }
        if !self.players[seat].hand.contains(&t.give) || !self.players[with].hand.contains(&t.want) {
            return err("One side no longer has the cards for this trade.");
        }
        self.players[seat].hand.remove(&t.give);
        self.players[with].hand.remove(&t.want);
        self.players[seat].hand.insert(&t.want);
        self.players[with].hand.insert(&t.give);
        self.trade = None;
        let (a, b) = (self.name(seat), self.name(with));
        self.say(format!("{a} and {b} complete a trade."));
        Ok(())
    }

    // ------------------------------------------------------- development cards

    fn buy_dev(&mut self, seat: usize, _rng: &mut impl Rng) -> Res {
        if self.phase != Phase::Main {
            return err("You can only buy after rolling.");
        }
        if self.deck.is_empty() {
            return err("The development deck is empty.");
        }
        self.pay(
            seat,
            &Hand::of(&[(Resource::Sheep, 1), (Resource::Wheat, 1), (Resource::Ore, 1)]),
        )?;
        let card = self.deck.pop().unwrap();
        self.players[seat].new_dev.push(card);
        let n = self.name(seat);
        self.say(format!("{n} buys a development card."));
        Ok(())
    }

    fn take_card(&mut self, seat: usize, card: DevCard) -> Res {
        if self.dev_played {
            return err("You can only play one development card per turn.");
        }
        let Some(pos) = self.players[seat].dev.iter().position(|c| *c == card) else {
            return err("You don't have a playable card of that kind.");
        };
        self.players[seat].dev.remove(pos);
        self.dev_played = true;
        Ok(())
    }

    fn play_knight(&mut self, seat: usize) -> Res {
        let back_to_roll = match self.phase {
            Phase::Roll => true,
            Phase::Main => false,
            _ => return err("You can't play a card right now."),
        };
        self.take_card(seat, DevCard::Knight)?;
        self.players[seat].knights += 1;
        self.phase = Phase::MoveRobber { back_to_roll };
        let n = self.name(seat);
        self.say(format!("{n} plays a Knight."));
        Ok(())
    }

    fn play_road_building(&mut self, seat: usize) -> Res {
        if self.phase != Phase::Main {
            return err("You can only play that after rolling.");
        }
        let avail = MAX_ROADS.saturating_sub(self.count_roads(seat));
        if avail == 0 || self.legal_roads(seat, None).is_empty() {
            return err("You have nowhere to build roads.");
        }
        self.take_card(seat, DevCard::RoadBuilding)?;
        self.phase = Phase::RoadBuilding { left: avail.min(2) as u8 };
        let n = self.name(seat);
        self.say(format!("{n} plays Road Building."));
        Ok(())
    }

    fn play_year_of_plenty(&mut self, seat: usize, a: Resource, b: Resource) -> Res {
        if self.phase != Phase::Main {
            return err("You can only play that after rolling.");
        }
        let mut want = Hand::default();
        want.add(a, 1);
        want.add(b, 1);
        if !self.bank.contains(&want) {
            return err("The bank doesn't have those resources.");
        }
        self.take_card(seat, DevCard::YearOfPlenty)?;
        self.bank.remove(&want);
        self.players[seat].hand.insert(&want);
        let n = self.name(seat);
        self.say(format!("{n} plays Year of Plenty and takes {}.", describe(&want)));
        Ok(())
    }

    fn play_monopoly(&mut self, seat: usize, r: Resource) -> Res {
        if self.phase != Phase::Main {
            return err("You can only play that after rolling.");
        }
        self.take_card(seat, DevCard::Monopoly)?;
        let mut total = 0u8;
        for s in 0..self.players.len() {
            if s != seat {
                let c = self.players[s].hand.get(r);
                self.players[s].hand.0[r.idx()] = 0;
                total += c;
            }
        }
        self.players[seat].hand.add(r, total);
        let n = self.name(seat);
        self.say(format!("{n} plays Monopoly and collects {total} {}.", r.name()));
        Ok(())
    }

    // ----------------------------------------------------------- achievements

    pub fn road_length(&self, seat: usize) -> u32 {
        let mut used = vec![false; self.board.edges.len()];
        let mut best = 0;
        for v in 0..self.board.vertices.len() {
            best = best.max(self.road_dfs(seat, v, &mut used, 0));
        }
        best
    }

    fn road_dfs(&self, seat: usize, v: usize, used: &mut Vec<bool>, depth: u32) -> u32 {
        let mut best = depth;
        for &e in &self.board.vertices[v].edges {
            if used[e] || self.roads[e] != Some(seat) {
                continue;
            }
            let ed = &self.board.edges[e];
            let w = if ed.a == v { ed.b } else { ed.a };
            used[e] = true;
            let blocked = matches!(self.buildings[w], Some(b) if b.owner != seat);
            let len = if blocked { depth + 1 } else { self.road_dfs(seat, w, used, depth + 1) };
            best = best.max(len);
            used[e] = false;
        }
        best
    }

    fn update_longest_road(&mut self) {
        let lens: Vec<u32> = (0..self.players.len()).map(|s| self.road_length(s)).collect();
        let best = *lens.iter().max().unwrap();
        let min = self.rules.longest_road_min as u32;
        let new = if best < min {
            None
        } else if let Some(h) = self.longest_road.filter(|&h| lens[h] == best) {
            Some(h)
        } else {
            let leaders: Vec<usize> = (0..lens.len()).filter(|&i| lens[i] == best).collect();
            if leaders.len() == 1 { Some(leaders[0]) } else { None }
        };
        if new != self.longest_road {
            if let Some(s) = new {
                let n = self.name(s);
                self.say(format!("{n} now has the Longest Road ({best})."));
            }
            self.longest_road = new;
        }
    }

    fn update_largest_army(&mut self) {
        let best = self.players.iter().map(|p| p.knights).max().unwrap();
        if best < self.rules.largest_army_min {
            return;
        }
        if let Some(h) = self.largest_army {
            if self.players[h].knights >= best {
                return;
            }
        }
        let s = self.players.iter().position(|p| p.knights == best).unwrap();
        if self.largest_army != Some(s) {
            let n = self.name(s);
            self.say(format!("{n} now has the Largest Army."));
            self.largest_army = Some(s);
        }
    }

    // ------------------------------------------------------------------ views

    /// The game as seen by `seat` (or a spectator when `None`): other
    /// players' hands and development cards are hidden.
    pub fn view_for(&self, seat: Option<usize>) -> GameView {
        let mut legal = Legal::default();
        if let Some(s) = seat.filter(|&s| s == self.current) {
            match &self.phase {
                Phase::Setup { expect: SetupExpect::Settlement, .. } => {
                    legal.settlements = self.legal_settlements(s, true)
                }
                Phase::Setup { expect: SetupExpect::Road, .. } => {
                    legal.roads = self.legal_roads(s, self.setup_vertex)
                }
                Phase::Main => {
                    legal.settlements = self.legal_settlements(s, false);
                    legal.cities = self.legal_cities(s);
                    legal.roads = self.legal_roads(s, None);
                }
                Phase::RoadBuilding { .. } => legal.roads = self.legal_roads(s, None),
                Phase::MoveRobber { .. } => {
                    legal.robber_tiles = (0..self.board.tiles.len()).filter(|&t| t != self.robber).collect()
                }
                _ => {}
            }
        }
        GameView {
            rules: self.rules.clone(),
            board: self.board.clone(),
            seat,
            players: (0..self.players.len())
                .map(|i| {
                    let p = &self.players[i];
                    PublicPlayer {
                        name: p.name.clone(),
                        hand_count: p.hand.total() as u8,
                        dev_count: (p.dev.len() + p.new_dev.len()) as u8,
                        knights: p.knights,
                        road_length: self.road_length(i) as u8,
                        public_vp: self.public_vp(i),
                        roads_left: MAX_ROADS.saturating_sub(self.count_roads(i)) as u8,
                        settlements_left: MAX_SETTLEMENTS.saturating_sub(self.count_settlements(i)) as u8,
                        cities_left: MAX_CITIES.saturating_sub(self.count_cities(i)) as u8,
                    }
                })
                .collect(),
            me: seat.map(|s| PrivateView {
                hand: self.players[s].hand,
                dev: self.players[s].dev.clone(),
                new_dev: self.players[s].new_dev.clone(),
                vp: self.vp(s),
                ratios: Resource::ALL.map(|r| self.bank_ratio_for(s, r)),
            }),
            phase: self.phase.clone(),
            current: self.current,
            robber: self.robber,
            dice: self.dice,
            buildings: self.buildings.clone(),
            roads: self.roads.clone(),
            bank: self.bank,
            deck_left: self.deck.len(),
            trade: self.trade.clone(),
            longest_road: self.longest_road,
            largest_army: self.largest_army,
            dev_played: self.dev_played,
            legal,
            log: self.log.clone(),
        }
    }
}

fn describe(h: &Hand) -> String {
    let parts: Vec<String> = Resource::ALL
        .iter()
        .filter(|r| h.get(**r) > 0)
        .map(|r| format!("{} {}", h.get(*r), r.name()))
        .collect();
    parts.join(", ")
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Legal {
    pub settlements: Vec<usize>,
    pub cities: Vec<usize>,
    pub roads: Vec<usize>,
    pub robber_tiles: Vec<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PublicPlayer {
    pub name: String,
    pub hand_count: u8,
    pub dev_count: u8,
    pub knights: u8,
    pub road_length: u8,
    pub public_vp: u8,
    pub roads_left: u8,
    pub settlements_left: u8,
    pub cities_left: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PrivateView {
    pub hand: Hand,
    pub dev: Vec<DevCard>,
    pub new_dev: Vec<DevCard>,
    pub vp: u8,
    /// Bank trade ratio per resource (indexed like `Resource::ALL`).
    pub ratios: [u8; 5],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameView {
    pub rules: Rules,
    pub board: Board,
    pub seat: Option<usize>,
    pub players: Vec<PublicPlayer>,
    pub me: Option<PrivateView>,
    pub phase: Phase,
    pub current: usize,
    pub robber: usize,
    pub dice: Option<(u8, u8)>,
    pub buildings: Vec<Option<Building>>,
    pub roads: Vec<Option<usize>>,
    pub bank: Hand,
    pub deck_left: usize,
    pub trade: Option<Trade>,
    pub longest_road: Option<usize>,
    pub largest_army: Option<usize>,
    pub dev_played: bool,
    pub legal: Legal,
    pub log: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    fn new_game(n: usize, seed: u64) -> (Game, StdRng) {
        let mut rng = StdRng::seed_from_u64(seed);
        let names = (0..n).map(|i| format!("P{i}")).collect();
        (Game::new(Rules::default(), names, &mut rng), rng)
    }

    fn auto_setup(g: &mut Game, rng: &mut StdRng) {
        while let Phase::Setup { expect, .. } = g.phase.clone() {
            let seat = g.current;
            let action = match expect {
                SetupExpect::Settlement => Action::BuildSettlement {
                    vertex: g.legal_settlements(seat, true)[0],
                },
                SetupExpect::Road => Action::BuildRoad {
                    edge: g.legal_roads(seat, g.setup_vertex)[0],
                },
            };
            g.apply(seat, action, rng).unwrap();
        }
    }

    #[test]
    fn board_has_standard_shape() {
        for seed in 0..20 {
            let (g, _) = new_game(3, seed);
            let b = &g.board;
            assert_eq!(b.tiles.len(), 19);
            assert_eq!(b.vertices.len(), 54);
            assert_eq!(b.edges.len(), 72);
            assert_eq!(b.edges.iter().filter(|e| e.coastal).count(), 30);
            assert_eq!(b.ports.len(), 9);
            assert_eq!(b.tiles.iter().filter(|t| t.number.is_some()).count(), 18);
            let d = &b.tiles[b.desert()];
            assert!(d.number.is_none());
            // No two hot numbers touch.
            for i in 0..19 {
                for j in b.tile_neighbors(i) {
                    let hot = |t: &crate::Tile| matches!(t.number, Some(6) | Some(8));
                    assert!(!(hot(&b.tiles[i]) && hot(&b.tiles[j])));
                }
            }
        }
    }

    #[test]
    fn setup_flow_completes() {
        let (mut g, mut rng) = new_game(3, 1);
        // Wrong player cannot act.
        assert!(g.apply(1, Action::BuildSettlement { vertex: 0 }, &mut rng).is_err());
        auto_setup(&mut g, &mut rng);
        assert_eq!(g.phase, Phase::Roll);
        assert_eq!(g.current, 0);
        for s in 0..3 {
            assert_eq!(g.count_settlements(s), 2);
            assert_eq!(g.count_roads(s), 2);
        }
    }

    #[test]
    fn setup_is_snake_order() {
        let (g, _) = new_game(4, 1);
        let order: Vec<usize> = (0..8).map(|s| g.setup_seat(s)).collect();
        assert_eq!(order, vec![0, 1, 2, 3, 3, 2, 1, 0]);
    }

    #[test]
    fn settlements_respect_distance_rule() {
        let (mut g, mut rng) = new_game(2, 2);
        let v = 0;
        g.apply(0, Action::BuildSettlement { vertex: v }, &mut rng).unwrap();
        let e = g.legal_roads(0, g.setup_vertex)[0];
        g.apply(0, Action::BuildRoad { edge: e }, &mut rng).unwrap();
        let n = g.board.vertices[v].neighbors[0];
        assert!(g.apply(1, Action::BuildSettlement { vertex: n }, &mut rng).is_err());
    }

    #[test]
    fn production_pays_out_and_robber_blocks() {
        let (mut g, mut rng) = new_game(2, 3);
        auto_setup(&mut g, &mut rng);
        // Find a producing tile next to player 0 and make it roll.
        let (ti, num) = g
            .board
            .tiles
            .iter()
            .enumerate()
            .find(|(i, t)| {
                t.number.is_some()
                    && *i != g.robber
                    && t.vertices.iter().any(|&v| matches!(g.buildings[v], Some(b) if b.owner == 0))
            })
            .map(|(i, t)| (i, t.number.unwrap()))
            .unwrap();
        let before: u32 = g.players.iter().map(|p| p.hand.total()).sum();
        let mut expected = 0u32;
        for (i, t) in g.board.tiles.iter().enumerate() {
            if t.number == Some(num) && i != g.robber {
                for &v in &t.vertices {
                    if let Some(b) = g.buildings[v] {
                        expected += if b.city { 2 } else { 1 };
                    }
                }
            }
        }
        let (d1, d2) = if num > 6 { (num - 6, 6) } else { (num - 1, 1) };
        g.do_roll(d1, d2, &mut rng).unwrap();
        let after: u32 = g.players.iter().map(|p| p.hand.total()).sum();
        assert_eq!(after - before, expected);
        assert!(expected > 0);

        // With the robber on that tile, nothing is produced from it.
        let (mut g2, mut rng2) = new_game(2, 3);
        auto_setup(&mut g2, &mut rng2);
        g2.robber = ti;
        let before: u32 = g2.players.iter().map(|p| p.hand.total()).sum();
        g2.do_roll(d1, d2, &mut rng2).unwrap();
        let after: u32 = g2.players.iter().map(|p| p.hand.total()).sum();
        assert!(after - before < expected);
    }

    #[test]
    fn seven_forces_discard_then_robber() {
        let (mut g, mut rng) = new_game(3, 4);
        auto_setup(&mut g, &mut rng);
        g.players[1].hand = Hand([3, 2, 2, 1, 1]); // 9 cards
        g.players[2].hand = Hand([1, 0, 0, 0, 0]);
        g.do_roll(3, 4, &mut rng).unwrap();
        assert_eq!(g.phase, Phase::Discard { pending: vec![(1, 4)] });
        // Wrong amount / cards not owned.
        assert!(g.apply(1, Action::Discard(Hand([1, 0, 0, 0, 0])), &mut rng).is_err());
        assert!(g.apply(1, Action::Discard(Hand([0, 0, 0, 0, 4])), &mut rng).is_err());
        // Nobody can move the robber before discards are done.
        assert!(g
            .apply(0, Action::MoveRobber { tile: 0, victim: None }, &mut rng)
            .is_err());
        g.apply(1, Action::Discard(Hand([2, 1, 1, 0, 0])), &mut rng).unwrap();
        assert_eq!(g.players[1].hand.total(), 5);
        assert_eq!(g.phase, Phase::MoveRobber { back_to_roll: false });
        // Must pick a different tile.
        let cur = g.robber;
        assert!(g.apply(0, Action::MoveRobber { tile: cur, victim: None }, &mut rng).is_err());
        // A tile with nobody on it, so no victim has to be chosen.
        let tile = (0..19)
            .filter(|&t| t != cur)
            .find(|&t| g.board.tiles[t].vertices.iter().all(|&v| g.buildings[v].is_none()))
            .unwrap();
        g.apply(0, Action::MoveRobber { tile, victim: None }, &mut rng).unwrap();
        assert_eq!(g.phase, Phase::Main);
        assert_eq!(g.robber, tile);
    }

    #[test]
    fn robber_steals_from_adjacent_player() {
        let (mut g, mut rng) = new_game(2, 5);
        auto_setup(&mut g, &mut rng);
        g.players[1].hand = Hand([0, 0, 0, 0, 1]);
        let v = g.buildings.iter().position(|b| matches!(b, Some(b) if b.owner == 1)).unwrap();
        let tile = g.board.vertices[v].tiles.iter().copied().find(|&t| t != g.robber).unwrap();
        let before = g.players[0].hand.total();
        g.phase = Phase::MoveRobber { back_to_roll: false };
        g.apply(0, Action::MoveRobber { tile, victim: None }, &mut rng).unwrap();
        assert_eq!(g.players[0].hand.total(), before + 1);
        assert_eq!(g.players[1].hand.total(), 0);
    }

    #[test]
    fn bank_trade_uses_ratio() {
        let (mut g, mut rng) = new_game(2, 6);
        auto_setup(&mut g, &mut rng);
        g.do_roll(1, 2, &mut rng).unwrap();
        g.players[0].hand = Hand([4, 0, 0, 0, 0]);
        let ratio = g.bank_ratio_for(0, Resource::Wood);
        assert!((2..=4).contains(&ratio));
        g.players[0].hand = Hand([ratio, 0, 0, 0, 0]);
        g.apply(0, Action::BankTrade { give: Resource::Wood, get: Resource::Ore }, &mut rng)
            .unwrap();
        assert_eq!(g.players[0].hand.get(Resource::Ore), 1);
        assert_eq!(g.players[0].hand.get(Resource::Wood), 0);
        assert!(g
            .apply(0, Action::BankTrade { give: Resource::Wood, get: Resource::Ore }, &mut rng)
            .is_err());
    }

    #[test]
    fn player_trade_requires_acceptance_and_confirmation() {
        let (mut g, mut rng) = new_game(3, 7);
        auto_setup(&mut g, &mut rng);
        g.do_roll(1, 2, &mut rng).unwrap();
        g.players[0].hand = Hand([2, 0, 0, 0, 0]);
        g.players[1].hand = Hand([0, 0, 1, 0, 0]);
        g.players[2].hand = Hand::default();
        g.apply(
            0,
            Action::ProposeTrade {
                give: Hand([2, 0, 0, 0, 0]),
                want: Hand([0, 0, 1, 0, 0]),
            },
            &mut rng,
        )
        .unwrap();
        // Player 2 lacks the sheep, so cannot accept.
        assert!(g.apply(2, Action::RespondTrade { accept: true }, &mut rng).is_err());
        // Confirming before anyone accepts fails.
        assert!(g.apply(0, Action::ConfirmTrade { with: 1 }, &mut rng).is_err());
        g.apply(1, Action::RespondTrade { accept: true }, &mut rng).unwrap();
        g.apply(0, Action::ConfirmTrade { with: 1 }, &mut rng).unwrap();
        assert_eq!(g.players[0].hand, Hand([0, 0, 1, 0, 0]));
        assert_eq!(g.players[1].hand, Hand([2, 0, 0, 0, 0]));
        assert!(g.trade.is_none());
    }

    fn walk_path(g: &Game, start: usize, len: usize) -> (Vec<usize>, Vec<usize>) {
        let mut verts = vec![start];
        let mut edges = vec![];
        while edges.len() < len {
            let v = *verts.last().unwrap();
            let e = g.board.vertices[v]
                .edges
                .iter()
                .copied()
                .find(|&e| {
                    let ed = &g.board.edges[e];
                    let w = if ed.a == v { ed.b } else { ed.a };
                    !verts.contains(&w)
                })
                .unwrap();
            let ed = &g.board.edges[e];
            verts.push(if ed.a == v { ed.b } else { ed.a });
            edges.push(e);
        }
        (verts, edges)
    }

    #[test]
    fn longest_road_awarded_and_broken() {
        let (mut g, _) = new_game(2, 8);
        let (verts, edges) = walk_path(&g, 0, 5);
        for &e in &edges {
            g.roads[e] = Some(0);
        }
        assert_eq!(g.road_length(0), 5);
        g.update_longest_road();
        assert_eq!(g.longest_road, Some(0));
        assert_eq!(g.public_vp(0), 2);

        // An opponent settlement in the middle splits the road.
        g.buildings[verts[2]] = Some(Building { owner: 1, city: false });
        assert_eq!(g.road_length(0), 3);
        g.update_longest_road();
        assert_eq!(g.longest_road, None);
    }

    #[test]
    fn road_length_counts_longest_branch_not_total() {
        let (mut g, _) = new_game(2, 9);
        let (verts, edges) = walk_path(&g, 0, 4);
        for &e in &edges {
            g.roads[e] = Some(0);
        }
        // Add a spur off the middle vertex that isn't already used.
        let mid = verts[2];
        let spur = g.board.vertices[mid].edges.iter().copied().find(|e| !edges.contains(e));
        if let Some(e) = spur {
            g.roads[e] = Some(0);
        }
        assert_eq!(g.road_length(0), 4);
    }

    #[test]
    fn dev_cards_cannot_be_played_the_turn_they_are_bought() {
        let (mut g, mut rng) = new_game(2, 10);
        auto_setup(&mut g, &mut rng);
        g.do_roll(1, 2, &mut rng).unwrap();
        g.players[0].hand = Hand([0, 0, 1, 1, 1]);
        g.deck.push(DevCard::Knight);
        g.apply(0, Action::BuyDevCard, &mut rng).unwrap();
        assert!(g.apply(0, Action::PlayKnight, &mut rng).is_err());
        g.apply(0, Action::EndTurn, &mut rng).unwrap();
        assert_eq!(g.players[0].dev, vec![DevCard::Knight]);
    }

    #[test]
    fn knights_award_largest_army() {
        let (mut g, mut rng) = new_game(2, 11);
        auto_setup(&mut g, &mut rng);
        g.players[0].knights = 2;
        g.players[0].dev = vec![DevCard::Knight];
        g.do_roll(1, 2, &mut rng).unwrap();
        g.apply(0, Action::PlayKnight, &mut rng).unwrap();
        assert_eq!(g.phase, Phase::MoveRobber { back_to_roll: false });
        assert_eq!(g.largest_army, Some(0));
    }

    #[test]
    fn monopoly_and_year_of_plenty() {
        let (mut g, mut rng) = new_game(3, 12);
        auto_setup(&mut g, &mut rng);
        g.do_roll(1, 2, &mut rng).unwrap();
        for p in g.players.iter_mut() {
            p.hand = Hand::default();
        }
        g.players[1].hand = Hand([0, 0, 0, 3, 0]);
        g.players[2].hand = Hand([0, 0, 0, 2, 0]);
        g.players[0].dev = vec![DevCard::Monopoly];
        g.apply(0, Action::PlayMonopoly(Resource::Wheat), &mut rng).unwrap();
        assert_eq!(g.players[0].hand.get(Resource::Wheat), 5);
        assert_eq!(g.players[1].hand.total() + g.players[2].hand.total(), 0);
        // Only one card per turn.
        g.players[0].dev = vec![DevCard::YearOfPlenty];
        assert!(g
            .apply(0, Action::PlayYearOfPlenty(Resource::Ore, Resource::Ore), &mut rng)
            .is_err());
    }

    #[test]
    fn cities_cost_and_replace_settlements() {
        let (mut g, mut rng) = new_game(2, 13);
        auto_setup(&mut g, &mut rng);
        g.do_roll(1, 2, &mut rng).unwrap();
        let v = g.legal_cities(0)[0];
        assert!(g.apply(0, Action::BuildCity { vertex: v }, &mut rng).is_err());
        g.players[0].hand = Hand([0, 0, 0, 2, 3]);
        g.apply(0, Action::BuildCity { vertex: v }, &mut rng).unwrap();
        assert_eq!(g.count_cities(0), 1);
        assert_eq!(g.count_settlements(0), 1);
        assert_eq!(g.players[0].hand.total(), 0);
    }

    #[test]
    fn reaching_ten_points_wins() {
        let (mut g, mut rng) = new_game(2, 14);
        auto_setup(&mut g, &mut rng);
        g.do_roll(1, 2, &mut rng).unwrap();
        g.players[0].dev = vec![DevCard::VictoryPoint; 8];
        g.apply(0, Action::EndTurn, &mut rng).unwrap();
        assert_eq!(g.phase, Phase::Finished { winner: 0 });
        assert!(g.apply(1, Action::Roll, &mut rng).is_err());
    }

    #[test]
    fn views_hide_other_players_cards() {
        let (mut g, mut rng) = new_game(2, 15);
        auto_setup(&mut g, &mut rng);
        g.players[1].hand = Hand([5, 5, 5, 5, 5]);
        let v = g.view_for(Some(0));
        assert_eq!(v.players[1].hand_count, 25);
        assert_eq!(v.me.as_ref().unwrap().hand, g.players[0].hand);
        let spectator = g.view_for(None);
        assert!(spectator.me.is_none());
    }
}
