//! What the Wonderful World screens show and what a click means, as plain
//! functions with native tests: the browser-only code stays thin.

use wonderful_core::{Action, Card, CardId, Cost, Event, Kind, Phase, Piece, Res, Score, Target, View, EMPIRES, ROUNDS};

// ----- names and colours ----------------------------------------------------

pub fn res_class(res: Res) -> &'static str {
    match res {
        Res::Materials => "res-materials",
        Res::Energy => "res-energy",
        Res::Science => "res-science",
        Res::Gold => "res-gold",
        Res::Exploration => "res-exploration",
    }
}

pub fn res_letter(res: Res) -> &'static str {
    match res {
        Res::Materials => "M",
        Res::Energy => "E",
        Res::Science => "S",
        Res::Gold => "G",
        Res::Exploration => "X",
    }
}

pub fn kind_class(kind: Kind) -> &'static str {
    match kind {
        Kind::Structure => "kind-structure",
        Kind::Vehicle => "kind-vehicle",
        Kind::Research => "kind-research",
        Kind::Project => "kind-project",
        Kind::Discovery => "kind-discovery",
    }
}

pub fn empire_name(seat: usize) -> &'static str {
    EMPIRES[seat % EMPIRES.len()].name
}

/// What the seat's Empire card scores at the end, e.g. "3 VP per Discovery".
pub fn empire_points(seat: usize) -> String {
    let empire = &EMPIRES[seat % EMPIRES.len()];
    let per = empire
        .combo
        .map(|(kind, points)| (points, kind.name()))
        .or(empire.per_token.map(|(token, points)| (points, token.name())));
    per.map(|(points, what)| format!("{points} VP per {what}")).unwrap_or_default()
}

/// The name shown for a seat: "You" for yourself.
pub fn who(seat: usize, names: &[String], you: usize) -> String {
    if seat == you {
        "You".into()
    } else {
        names.get(seat).cloned().unwrap_or_else(|| format!("Player {}", seat + 1))
    }
}

fn plain_name(seat: usize, names: &[String]) -> String {
    names.get(seat).cloned().unwrap_or_else(|| format!("Player {}", seat + 1))
}

/// "Ann", "Ann and Bob", "Ann, Bob and Cy".
pub fn join_names(seats: &[usize], names: &[String]) -> String {
    let list: Vec<String> = seats.iter().map(|&s| plain_name(s, names)).collect();
    match list.as_slice() {
        [] => String::new(),
        [one] => one.clone(),
        [init @ .., last] => format!("{} and {}", init.join(", "), last),
    }
}

// ----- the spaces of a building -----------------------------------------------

/// What a space on a building accepts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Space {
    Cube(Res),
    /// Only Krystallium fills it.
    Krystallium,
    General,
    Financier,
}

impl Space {
    pub fn class(self) -> &'static str {
        match self {
            Space::Cube(res) => res_class(res),
            Space::Krystallium => "krystallium",
            Space::General => "general",
            Space::Financier => "financier",
        }
    }

    pub fn letter(self) -> &'static str {
        match self {
            Space::Cube(res) => res_letter(res),
            Space::Krystallium => "K",
            Space::General => "Gen",
            Space::Financier => "Fin",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Space::Cube(res) => res.name(),
            Space::Krystallium => "Krystallium",
            Space::General => "General",
            Space::Financier => "Financier",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    pub space: Space,
    pub filled: bool,
}

/// Every space of a card with the given cost, filled ones first within each
/// kind of space: resources in production order, then Krystallium, then
/// Generals, then Financiers.
pub fn slots(cost: &Cost, filled: &Cost) -> Vec<Slot> {
    let mut out = Vec::new();
    let mut add = |space: Space, needed: u8, placed: u8| {
        for i in 0..needed {
            out.push(Slot { space, filled: i < placed });
        }
    };
    for res in Res::ALL {
        add(Space::Cube(res), cost.res[res.index()], filled.res[res.index()]);
    }
    add(Space::Krystallium, cost.krystallium, filled.krystallium);
    add(Space::General, cost.generals, filled.generals);
    add(Space::Financier, cost.financiers, filled.financiers);
    out
}

/// (placed, needed) over all spaces of a building.
pub fn progress(cost: &Cost, filled: &Cost) -> (u32, u32) {
    let placed = slots(cost, filled).iter().filter(|s| s.filled).count() as u32;
    (placed, cost.total())
}

// ----- what you hold and where it can go -----------------------------------------

/// Something you can pick up and put on a building.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Held {
    Cube(Res),
    Krystallium,
    General,
    Financier,
}

impl Held {
    pub fn class(self) -> &'static str {
        match self {
            Held::Cube(res) => res_class(res),
            Held::Krystallium => "krystallium",
            Held::General => "general",
            Held::Financier => "financier",
        }
    }

    pub fn letter(self) -> &'static str {
        match self {
            Held::Cube(res) => res_letter(res),
            Held::Krystallium => "K",
            Held::General => "Gen",
            Held::Financier => "Fin",
        }
    }

    pub fn name(self) -> String {
        match self {
            Held::Cube(res) => format!("{} cube", res.name()),
            Held::Krystallium => "Krystallium".into(),
            Held::General => "General".into(),
            Held::Financier => "Financier".into(),
        }
    }
}

/// What you can place right now, with how many of each: the cubes of the
/// production step first, then what you keep from round to round.
pub fn available(view: &View) -> Vec<(Held, u8)> {
    let me = &view.players[view.you];
    if !matches!(view.phase, Phase::Planning | Phase::Production { .. }) {
        return Vec::new();
    }
    let mut out = Vec::new();
    if let Phase::Production { step } = view.phase {
        if let Some(&res) = Res::ALL.get(step as usize) {
            if me.pool > 0 {
                out.push((Held::Cube(res), me.pool));
            }
        }
    }
    if me.krystallium > 0 {
        out.push((Held::Krystallium, me.krystallium));
    }
    if me.generals > 0 {
        out.push((Held::General, me.generals));
    }
    if me.financiers > 0 {
        out.push((Held::Financier, me.financiers));
    }
    out
}

/// What a click on a space would place: the piece the player picked if they
/// still have it, otherwise the first thing they hold.
pub fn effective_held(view: &View, chosen: Option<Held>) -> Option<Held> {
    let have = available(view);
    match chosen {
        Some(h) if have.iter().any(|(a, _)| *a == h) => Some(h),
        _ => have.first().map(|(h, _)| *h),
    }
}

/// What `held` becomes when it goes on `space`, if it fits there: a cube fits
/// its own resource's space, Krystallium any resource's and its own, a
/// character its own.
fn piece_for(held: Held, space: Space) -> Option<Piece> {
    match (held, space) {
        (Held::Cube(a), Space::Cube(b)) if a == b => Some(Piece::Cube(a)),
        (Held::Krystallium, Space::Cube(res)) => Some(Piece::Krystallium(Some(res))),
        (Held::Krystallium, Space::Krystallium) => Some(Piece::Krystallium(None)),
        (Held::General, Space::General) => Some(Piece::General),
        (Held::Financier, Space::Financier) => Some(Piece::Financier),
        _ => None,
    }
}

/// The move for putting `held` on a free `space` of the building `card`.
pub fn slot_action(held: Held, space: Space, card: CardId) -> Option<Action> {
    piece_for(held, space).map(|piece| Action::Place { piece, target: Target::Card(card) })
}

/// The move for putting `held` on the Empire (only cubes go there).
pub fn empire_action(held: Held) -> Option<Action> {
    match held {
        Held::Cube(res) => Some(Action::Place { piece: Piece::Cube(res), target: Target::Empire }),
        _ => None,
    }
}

/// What a click on a free `space` places. The piece picked in the tray if it
/// fits; otherwise the plain piece for that space (the cube of its resource,
/// the character, Krystallium on its own space) if you hold one. On a
/// resource's space Krystallium is only ever placed when it was picked, since
/// it can stand in for anything and is easy to waste.
pub fn pick_for_space(view: &View, chosen: Option<Held>, space: Space) -> Option<Held> {
    let have = available(view);
    let holds = |h: Held| have.iter().any(|(a, _)| *a == h);
    if let Some(h) = chosen {
        if holds(h) && piece_for(h, space).is_some() {
            return Some(h);
        }
    }
    let plain = match space {
        Space::Cube(res) => Held::Cube(res),
        Space::Krystallium => Held::Krystallium,
        Space::General => Held::General,
        Space::Financier => Held::Financier,
    };
    holds(plain).then_some(plain)
}

/// What a click on the Empire places: the cube picked in the tray, or else the
/// first cube you hold (the current step's resource comes first).
pub fn pick_for_empire(view: &View, chosen: Option<Held>) -> Option<Held> {
    let have = available(view);
    match chosen {
        Some(h @ Held::Cube(_)) if have.iter().any(|(a, _)| *a == h) => Some(h),
        _ => have.iter().map(|(h, _)| *h).find(|h| matches!(h, Held::Cube(_))),
    }
}

// ----- recycling and scrapping: where the cube goes ------------------------------------

/// A card whose recycling cube still needs a place to go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CubeFrom {
    /// A drafted card being recycled in the planning phase.
    Recycle(CardId),
    /// A building being given up. Its cube always goes to the Empire, so
    /// the choice is only a confirmation.
    Scrap(CardId),
}

impl CubeFrom {
    pub fn card(self) -> CardId {
        match self {
            CubeFrom::Recycle(card) | CubeFrom::Scrap(card) => card,
        }
    }

    /// The cube the card gives.
    pub fn res(self) -> Res {
        self.card().def().recycle
    }

    /// Whether the card is still where it was when the choice was started
    /// (the game may have moved on, e.g. to the next round).
    pub fn valid(self, view: &View) -> bool {
        match self {
            CubeFrom::Recycle(card) => view.phase == Phase::Planning && view.drafted.contains(&card),
            CubeFrom::Scrap(card) => {
                matches!(view.phase, Phase::Planning | Phase::Production { .. })
                    && view.players[view.you].buildings.iter().any(|b| b.card == card)
            }
        }
    }

    /// The move that gives the cube to `to` (one of `cube_targets`).
    pub fn action(self, to: Target) -> Action {
        match self {
            CubeFrom::Recycle(card) => Action::Recycle { card, to },
            CubeFrom::Scrap(card) => Action::Scrap { card },
        }
    }
}

/// Where the cube can go: when recycling, your buildings that still have a
/// space for it; always the Empire.
pub fn cube_targets(view: &View, from: CubeFrom) -> Vec<Target> {
    let res = from.res();
    let mut out: Vec<Target> = match from {
        CubeFrom::Recycle(_) => view.players[view.you]
            .buildings
            .iter()
            .filter(|b| b.remaining().res[res.index()] > 0)
            .map(|b| Target::Card(b.card))
            .collect(),
        CubeFrom::Scrap(_) => Vec::new(),
    };
    out.push(Target::Empire);
    out
}

pub fn target_name(target: Target) -> String {
    match target {
        Target::Card(card) => card.def().name.to_string(),
        Target::Empire => "your Empire".to_string(),
    }
}

/// The seat that produced strictly more than everyone else in the current
/// production step, i.e. the one who takes the character.
pub fn supremacy(view: &View) -> Option<usize> {
    let Phase::Production { step } = view.phase else { return None };
    if step as usize >= Res::ALL.len() {
        return None;
    }
    let most = view.players.iter().map(|p| p.produced).max()?;
    if most == 0 {
        return None;
    }
    let best: Vec<usize> = (0..view.players.len()).filter(|&i| view.players[i].produced == most).collect();
    (best.len() == 1).then(|| best[0])
}

// ----- card text -----------------------------------------------------------------

pub fn cost_text(cost: &Cost) -> String {
    let mut parts: Vec<String> = Res::ALL
        .iter()
        .filter(|r| cost.res[r.index()] > 0)
        .map(|r| format!("{} {}", cost.res[r.index()], r.name()))
        .collect();
    if cost.krystallium > 0 {
        parts.push(format!("{} Krystallium", cost.krystallium));
    }
    if cost.generals > 0 {
        parts.push(format!("{} General{}", cost.generals, if cost.generals > 1 { "s" } else { "" }));
    }
    if cost.financiers > 0 {
        parts.push(format!("{} Financier{}", cost.financiers, if cost.financiers > 1 { "s" } else { "" }));
    }
    parts.join(", ")
}

/// What the card produces every round once it is finished.
pub fn production_parts(card: &Card) -> Vec<(Res, String)> {
    let mut parts: Vec<(Res, String)> = Res::ALL
        .iter()
        .filter(|r| card.produces[r.index()] > 0)
        .map(|r| (*r, format!("+{} {}", card.produces[r.index()], r.name())))
        .collect();
    if let Some((res, kind)) = card.scaled {
        parts.push((res, format!("+1 {} per {}", res.name(), kind.name())));
    }
    parts
}

/// The points the card is worth, as short phrases.
pub fn points_parts(card: &Card) -> Vec<String> {
    let mut parts = Vec::new();
    if card.vp > 0 {
        parts.push(format!("{} VP", card.vp));
    }
    if let Some((kind, points)) = card.combo {
        parts.push(format!("{points} VP per {}", kind.name()));
    }
    if let Some((token, points)) = card.per_token {
        parts.push(format!("{points} VP per {}", token.name()));
    }
    parts
}

/// The whole card in one sentence, for a tooltip or a screen reader.
pub fn card_summary(card: &Card) -> String {
    let mut parts = vec![format!("{} ({})", card.name, card.kind.name()), format!("costs {}", cost_text(&card.cost))];
    let makes: Vec<String> = production_parts(card).into_iter().map(|(_, text)| text).collect();
    if !makes.is_empty() {
        parts.push(format!("makes {}", makes.join(", ")));
    }
    let worth = points_parts(card);
    if !worth.is_empty() {
        parts.push(format!("worth {}", worth.join(", ")));
    }
    if let Some(bonus) = bonus_text(card) {
        parts.push(format!("gives {bonus} when built"));
    }
    parts.push(format!("recycles into {} {} cube", article(card.recycle.name()), card.recycle.name()));
    parts.join("; ")
}

/// "a" or "an", for a word that starts with a capital: "an Energy cube".
fn article(word: &str) -> &'static str {
    if word.starts_with(['A', 'E', 'I', 'O', 'U']) {
        "an"
    } else {
        "a"
    }
}

/// What finishing the card hands over, e.g. "a General" or "2 Krystallium".
pub fn bonus_text(card: &Card) -> Option<String> {
    use wonderful_core::{Bonus, Token};
    let mut parts = Vec::new();
    for (bonus, name, plural) in [
        (Bonus::Krystallium, "Krystallium", "Krystallium"),
        (Bonus::Token(Token::General), "General", "Generals"),
        (Bonus::Token(Token::Financier), "Financier", "Financiers"),
    ] {
        match card.bonus.iter().filter(|&&b| b == bonus).count() {
            0 => {}
            1 => parts.push(format!("{} {name}", article(name))),
            n => parts.push(format!("{n} {plural}")),
        }
    }
    (!parts.is_empty()).then(|| parts.join(" and "))
}

// ----- the state of the game in words ---------------------------------------------

/// The step of the production phase.
pub fn step_name(step: u8) -> &'static str {
    Res::ALL.get(step as usize).map_or("Wrap-up", |r| r.name())
}

pub fn phase_title(view: &View) -> String {
    match view.phase {
        Phase::Draft => "Draft".into(),
        Phase::Planning => "Planning".into(),
        Phase::Production { step } => format!("Production: {}", step_name(step)),
        Phase::Over => "Game over".into(),
    }
}

/// The seats the game is waiting for right now.
pub fn waiting_on(view: &View) -> Vec<usize> {
    let waiting = |p: &wonderful_core::PlayerView| match view.phase {
        Phase::Draft => !p.picked,
        Phase::Planning | Phase::Production { .. } => !p.ready,
        Phase::Over => false,
    };
    view.players.iter().enumerate().filter(|(_, p)| waiting(p)).map(|(i, _)| i).collect()
}

/// Who your hand goes to this round.
pub fn pass_to(view: &View) -> usize {
    let n = view.players.len();
    if view.passes_to_next {
        (view.you + 1) % n
    } else {
        (view.you + n - 1) % n
    }
}

/// What the player should do now.
pub fn hint(view: &View, names: &[String]) -> String {
    let me = &view.players[view.you];
    let others: Vec<usize> = waiting_on(view).into_iter().filter(|&s| s != view.you).collect();
    let waiting = || {
        if others.is_empty() {
            "Waiting for the next step…".to_string()
        } else {
            format!("Waiting for {}…", join_names(&others, names))
        }
    };
    match view.phase {
        Phase::Draft if me.picked => waiting(),
        Phase::Draft => format!(
            "Pick a card ({} of {}). The others go to {}.",
            view.drafted.len() + 1,
            wonderful_core::DRAFT_PICKS,
            plain_name(pass_to(view), names)
        ),
        Phase::Planning if !view.drafted.is_empty() => format!(
            "Build or recycle each of your {} drafted cards. A recycled card's cube goes on a building or your Empire right away.",
            view.drafted.len()
        ),
        Phase::Planning if !me.ready => "Done with your cards? Press the button when you are.".into(),
        Phase::Production { .. } if me.choose => {
            "You produced the most Science: choose a General or a Financier.".to_string()
        }
        Phase::Production { step } if (step as usize) < Res::ALL.len() && !me.ready => {
            let res = Res::ALL[step as usize];
            format!(
                "Place your {} {} cube{}: pick a free space on a building, or put them on your Empire. Cubes you don't place are lost.",
                me.pool,
                res.name(),
                if me.pool == 1 { "" } else { "s" }
            )
        }
        Phase::Production { .. } if !me.ready => {
            "Last chance this round to place Krystallium or characters you hold. Finish when you're done.".into()
        }
        Phase::Production { .. } | Phase::Planning => waiting(),
        Phase::Over => "Four rounds played. These are the final scores.".into(),
    }
}

/// One line of the log.
pub fn describe_event(event: &Event, names: &[String], you: usize) -> String {
    match event {
        Event::RoundStarted(round) => format!("Round {round} of {ROUNDS} begins."),
        Event::Completed { seat, card } => {
            format!("{} finished {}.", who(*seat, names, you), card.def().name)
        }
        Event::Supremacy { res, seat: Some(seat) } => {
            let what = match res.supremacy_token() {
                Some(token) => format!("took a {}", token.name()),
                None => "may choose a character".into(),
            };
            format!("{} produced the most {} and {what}.", who(*seat, names, you), res.name())
        }
        Event::Supremacy { res, seat: None } => {
            let character = match res.supremacy_token() {
                Some(token) => token.name(),
                None => "character",
            };
            format!("{} tied: nobody takes a {character}.", res.name())
        }
        Event::Chose { seat, token } => format!("{} chose a {}.", who(*seat, names, you), token.name()),
    }
}

// ----- the end of the game ----------------------------------------------------------

/// "You", "You and Bob", "Ann, Bob and You" (yourself named as "You").
fn join_who(seats: &[usize], names: &[String], you: usize) -> String {
    let list: Vec<String> = seats.iter().map(|&s| who(s, names, you)).collect();
    match list.as_slice() {
        [] => String::new(),
        [one] => one.clone(),
        [init @ .., last] => format!("{} and {}", init.join(", "), last),
    }
}

/// Who won, as a sentence.
pub fn verdict(view: &View, names: &[String]) -> String {
    match view.winners.as_slice() {
        [] => "The game is over.".into(),
        [one] if *one == view.you => "You win!".into(),
        [one] => format!("{} wins.", who(*one, names, view.you)),
        many => {
            // Yourself first: "You and Bob share the win."
            let mut seats = many.to_vec();
            seats.sort_by_key(|&s| s != view.you);
            format!("{} share the win.", join_who(&seats, names, view.you))
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResultRow {
    pub seat: usize,
    pub name: String,
    pub score: Score,
    pub winner: bool,
}

/// The final standings, best first (same order as the tie-breaks).
pub fn results(view: &View, names: &[String]) -> Vec<ResultRow> {
    let mut rows: Vec<ResultRow> = view
        .scores
        .iter()
        .enumerate()
        .map(|(seat, score)| ResultRow {
            seat,
            name: plain_name(seat, names),
            score: *score,
            winner: view.winners.contains(&seat),
        })
        .collect();
    let tokens = |seat: usize| view.players[seat].generals as u32 + view.players[seat].financiers as u32;
    rows.sort_by_key(|r| std::cmp::Reverse((r.score.total, r.score.cards, tokens(r.seat))));
    rows
}

/// Finished cards of each type, in `Kind::ALL` order.
pub fn kind_counts(empire: &[CardId]) -> [u32; 5] {
    let mut counts = [0; 5];
    for id in empire {
        let kind = id.def().kind;
        if let Some(at) = Kind::ALL.iter().position(|k| *k == kind) {
            counts[at] += 1;
        }
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;
    use wonderful_core::{catalogue, Action as A, State, Token, WRAP_UP};

    fn id(name: &str) -> CardId {
        CardId(catalogue().iter().position(|c| c.name == name).unwrap() as u16)
    }

    fn names() -> Vec<String> {
        ["Ann", "Bob", "Cy"].iter().map(|s| s.to_string()).collect()
    }

    /// A game in planning in which everybody drafted their first card each time.
    fn planning() -> State {
        let mut s = State::new(3, 4);
        for _ in 0..wonderful_core::DRAFT_PICKS {
            for seat in 0..3 {
                let card = s.players[seat].hand[0];
                s.apply(seat, A::Draft { card }).unwrap();
            }
        }
        s
    }

    #[test]
    fn spaces_are_listed_in_order_with_the_filled_ones_first() {
        let cost = Cost { res: [2, 0, 1, 0, 0], krystallium: 1, generals: 1, financiers: 0 };
        let filled = Cost { res: [1, 5, 0, 0, 0], krystallium: 1, ..Cost::default() };
        let list = slots(&cost, &filled);
        assert_eq!(
            list,
            vec![
                Slot { space: Space::Cube(Res::Materials), filled: true },
                Slot { space: Space::Cube(Res::Materials), filled: false },
                Slot { space: Space::Cube(Res::Science), filled: false },
                Slot { space: Space::Krystallium, filled: true },
                Slot { space: Space::General, filled: false },
            ]
        );
        assert_eq!(progress(&cost, &filled), (2, 5));
        assert!(slots(&cost, &Cost::default()).iter().all(|s| !s.filled));
        assert!(slots(&cost, &cost).iter().all(|s| s.filled));
        // A real card: Juggernaut needs 3 Materials, 3 Energy and a Krystallium.
        let juggernaut = id("Juggernaut").def().cost;
        assert_eq!(slots(&juggernaut, &Cost::default()).last().map(|s| s.space), Some(Space::Krystallium));
    }

    #[test]
    fn a_click_on_a_space_places_the_held_piece_when_it_fits() {
        let plant = id("Recycling Plant");
        let to = Target::Card(plant);
        let mat = Space::Cube(Res::Materials);
        assert_eq!(
            slot_action(Held::Cube(Res::Materials), mat, plant),
            Some(Action::Place { piece: Piece::Cube(Res::Materials), target: to })
        );
        // A cube only fills its own resource's space; Krystallium fills any.
        assert_eq!(slot_action(Held::Cube(Res::Gold), mat, plant), None);
        assert_eq!(
            slot_action(Held::Krystallium, Space::Cube(Res::Gold), plant),
            Some(Action::Place { piece: Piece::Krystallium(Some(Res::Gold)), target: to })
        );
        // A Krystallium space takes Krystallium only.
        assert_eq!(
            slot_action(Held::Krystallium, Space::Krystallium, plant),
            Some(Action::Place { piece: Piece::Krystallium(None), target: to })
        );
        assert_eq!(slot_action(Held::Cube(Res::Gold), Space::Krystallium, plant), None);
        assert_eq!(slot_action(Held::General, Space::Krystallium, plant), None);
        // Characters only fill character spaces, and Krystallium can't.
        assert_eq!(
            slot_action(Held::General, Space::General, plant),
            Some(Action::Place { piece: Piece::General, target: to })
        );
        assert_eq!(
            slot_action(Held::Financier, Space::Financier, plant),
            Some(Action::Place { piece: Piece::Financier, target: to })
        );
        assert_eq!(slot_action(Held::General, Space::Financier, plant), None);
        assert_eq!(slot_action(Held::Krystallium, Space::General, plant), None);
        assert_eq!(slot_action(Held::Cube(Res::Materials), Space::General, plant), None);
    }

    #[test]
    fn only_cubes_go_on_the_empire() {
        assert_eq!(
            empire_action(Held::Cube(Res::Energy)),
            Some(Action::Place { piece: Piece::Cube(Res::Energy), target: Target::Empire })
        );
        assert_eq!(empire_action(Held::Krystallium), None);
        assert_eq!(empire_action(Held::General), None);
        assert_eq!(empire_action(Held::Financier), None);
    }

    #[test]
    fn what_you_hold_depends_on_the_phase() {
        let mut s = planning();
        assert!(available(&s.view_for(0)).is_empty(), "nothing yet in planning");
        s.players[0].krystallium = 1;
        s.players[0].generals = 3;
        s.players[0].financiers = 1;
        let kept = vec![(Held::Krystallium, 1), (Held::General, 3), (Held::Financier, 1)];
        assert_eq!(available(&s.view_for(0)), kept);
        // Others only see their own things.
        assert!(available(&s.view_for(1)).is_empty());

        // In production the current step's cubes come first.
        for seat in 0..3 {
            for card in s.players[seat].drafted.clone() {
                s.apply(seat, A::Build { card }).unwrap();
            }
            s.apply(seat, A::Ready).unwrap();
        }
        assert!(matches!(s.phase, Phase::Production { step: 0 }));
        s.players[0].pool = 2;
        s.players[0].financiers = 1; // whatever the Materials race gave
        let have = available(&s.view_for(0));
        assert_eq!(have[0], (Held::Cube(Res::Materials), 2));
        assert_eq!(have[1..], kept);
    }

    #[test]
    fn the_picked_piece_is_used_while_it_lasts_then_the_first_one() {
        let mut s = planning();
        s.players[0].krystallium = 2;
        s.players[0].financiers = 1;
        let view = s.view_for(0);
        assert_eq!(effective_held(&view, None), Some(Held::Krystallium));
        assert_eq!(effective_held(&view, Some(Held::Financier)), Some(Held::Financier));
        // Picked something you no longer have: fall back.
        assert_eq!(effective_held(&view, Some(Held::General)), Some(Held::Krystallium));
        assert_eq!(effective_held(&s.view_for(1), Some(Held::General)), None);
    }

    #[test]
    fn a_space_takes_the_picked_piece_or_else_the_plain_one() {
        let mut s = planning();
        s.phase = Phase::Production { step: 0 };
        s.players[0].pool = 1; // a Materials cube
        s.players[0].krystallium = 1;
        s.players[0].generals = 1;
        let view = s.view_for(0);
        let mat = Space::Cube(Res::Materials);
        let sci = Space::Cube(Res::Science);

        // Nothing picked: the plain cube for the space, a character for its own space.
        assert_eq!(pick_for_space(&view, None, mat), Some(Held::Cube(Res::Materials)));
        assert_eq!(pick_for_space(&view, None, Space::General), Some(Held::General));
        // No Science cube and no pick: Krystallium is never used on its own.
        assert_eq!(pick_for_space(&view, None, sci), None);
        assert_eq!(pick_for_space(&view, None, Space::Financier), None);
        // Picked Krystallium: it goes on any resource space, even where a cube would also fit.
        assert_eq!(pick_for_space(&view, Some(Held::Krystallium), sci), Some(Held::Krystallium));
        assert_eq!(pick_for_space(&view, Some(Held::Krystallium), mat), Some(Held::Krystallium));
        // ...but not on a character's space: fall back to what fits.
        assert_eq!(pick_for_space(&view, Some(Held::Krystallium), Space::General), Some(Held::General));
        // A picked piece that doesn't fit the space gives way to the plain one.
        assert_eq!(pick_for_space(&view, Some(Held::General), mat), Some(Held::Cube(Res::Materials)));
        // Krystallium is the plain piece for its own space.
        assert_eq!(pick_for_space(&view, None, Space::Krystallium), Some(Held::Krystallium));
        assert_eq!(pick_for_space(&view, Some(Held::General), Space::Krystallium), Some(Held::Krystallium));
        // A piece you no longer hold is ignored.
        assert_eq!(pick_for_space(&view, Some(Held::Cube(Res::Gold)), mat), Some(Held::Cube(Res::Materials)));
        // Someone who holds nothing can't place anything.
        assert_eq!(pick_for_space(&s.view_for(1), Some(Held::Krystallium), mat), None);

        assert!(piece_for(Held::Cube(Res::Gold), Space::Cube(Res::Gold)).is_some());
        assert!(piece_for(Held::Cube(Res::Gold), Space::Cube(Res::Energy)).is_none());
        assert!(piece_for(Held::Krystallium, Space::Cube(Res::Energy)).is_some());
        assert!(piece_for(Held::Krystallium, Space::General).is_none());
        assert!(piece_for(Held::Krystallium, Space::Krystallium).is_some());
    }

    #[test]
    fn the_empire_takes_cubes_only() {
        let mut s = planning();
        s.players[0].krystallium = 2;
        s.players[0].generals = 2;
        assert_eq!(pick_for_empire(&s.view_for(0), None), None, "no cube held");
        assert_eq!(pick_for_empire(&s.view_for(0), Some(Held::Krystallium)), None);
        s.phase = Phase::Production { step: 2 };
        s.players[0].pool = 2;
        let view = s.view_for(0);
        assert_eq!(pick_for_empire(&view, None), Some(Held::Cube(Res::Science)));
        assert_eq!(pick_for_empire(&view, Some(Held::Cube(Res::Science))), Some(Held::Cube(Res::Science)));
        // A pick that isn't a cube (or isn't held) falls back to the first cube.
        assert_eq!(pick_for_empire(&view, Some(Held::General)), Some(Held::Cube(Res::Science)));
        assert_eq!(pick_for_empire(&view, Some(Held::Cube(Res::Gold))), Some(Held::Cube(Res::Science)));
    }

    #[test]
    fn a_recycled_cube_can_go_to_the_empire_or_a_building_that_wants_it() {
        let mut s = planning();
        // Seat 0 builds one card needing Materials and recycles another.
        let plant = id("Recycling Plant");
        let nuclear = id("Nuclear Plant");
        s.players[0].drafted = vec![plant, nuclear, id("Zeppelin")];
        s.apply(0, A::Build { card: plant }).unwrap();
        let view = s.view_for(0);

        let recycle = CubeFrom::Recycle(nuclear);
        assert!(recycle.valid(&view));
        assert_eq!(recycle.card(), nuclear);
        let res = nuclear.def().recycle;
        let wants = plant.def().cost.res[res.index()] > 0;
        let targets = cube_targets(&view, recycle);
        assert_eq!(targets.contains(&Target::Card(plant)), wants, "{res:?}");
        assert_eq!(targets.last(), Some(&Target::Empire), "the Empire is always possible");
        assert_eq!(recycle.action(Target::Empire), Action::Recycle { card: nuclear, to: Target::Empire });

        // Scrapping a building always sends its cube to the Empire.
        let scrap = CubeFrom::Scrap(plant);
        assert!(scrap.valid(&view));
        assert_eq!(cube_targets(&view, scrap), vec![Target::Empire]);
        assert_eq!(scrap.action(Target::Empire), Action::Scrap { card: plant });

        // A card that isn't there (any more) can't be moved.
        assert!(!CubeFrom::Recycle(plant).valid(&view), "built cards can't be recycled");
        assert!(!CubeFrom::Scrap(nuclear).valid(&view), "only buildings can be scrapped");
        s.phase = Phase::Draft;
        assert!(!CubeFrom::Recycle(nuclear).valid(&s.view_for(0)), "the phase moved on");

        assert_eq!(target_name(Target::Card(plant)), "Recycling Plant");
        assert_eq!(target_name(Target::Empire), "your Empire");
    }

    #[test]
    fn the_cube_goes_where_the_server_would_accept_it() {
        // Whatever the helpers offer, the rules must accept: try every target
        // of every drafted card of every seat on a copy of the game.
        let s = planning();
        for seat in 0..3 {
            let mut s = s.clone();
            for card in s.players[seat].drafted.clone().into_iter().take(2) {
                s.apply(seat, A::Build { card }).unwrap();
            }
            let view = s.view_for(seat);
            for card in view.drafted.clone() {
                let from = CubeFrom::Recycle(card);
                for to in cube_targets(&view, from) {
                    let mut t = s.clone();
                    assert!(t.apply(seat, from.action(to)).is_ok(), "{} -> {to:?}", card.def().name);
                }
            }
            for b in &view.players[seat].buildings {
                let from = CubeFrom::Scrap(b.card);
                for to in cube_targets(&view, from) {
                    let mut t = s.clone();
                    assert!(t.apply(seat, from.action(to)).is_ok(), "scrap {} -> {to:?}", b.card.def().name);
                }
            }
        }
    }

    #[test]
    fn the_supremacy_goes_to_the_one_who_produced_most() {
        let mut s = planning();
        for seat in 0..3 {
            for card in s.players[seat].drafted.clone() {
                s.apply(seat, A::Build { card }).unwrap();
            }
            s.apply(seat, A::Ready).unwrap();
        }
        assert!(matches!(s.phase, Phase::Production { step: 0 }));
        s.players[0].produced = 3;
        s.players[1].produced = 1;
        s.players[2].produced = 0;
        assert_eq!(supremacy(&s.view_for(2)), Some(0));
        s.players[1].produced = 3;
        assert_eq!(supremacy(&s.view_for(0)), None, "tied");
        s.players.iter_mut().for_each(|p| p.produced = 0);
        assert_eq!(supremacy(&s.view_for(0)), None, "nobody produced anything");
        s.players[1].produced = 2;
        s.phase = Phase::Production { step: WRAP_UP };
        assert_eq!(supremacy(&s.view_for(0)), None, "no supremacy in the wrap-up");
        s.phase = Phase::Planning;
        assert_eq!(supremacy(&s.view_for(0)), None);
    }

    #[test]
    fn card_text_reads_like_the_cards() {
        let sonar = id("Super-Sonar").def();
        assert_eq!(production_parts(sonar), vec![(Res::Exploration, "+1 Exploration per Vehicle".to_string())]);
        let plant = id("Recycling Plant").def();
        assert_eq!(production_parts(plant), vec![(Res::Materials, "+2 Materials".to_string())]);
        assert_eq!(cost_text(&plant.cost), "2 Materials");
        assert_eq!(cost_text(&id("Juggernaut").def().cost), "3 Materials, 3 Energy, 1 Krystallium");
        assert_eq!(cost_text(&id("Center of the Earth").def().cost), "5 Exploration, 2 Generals");
        assert_eq!(cost_text(&id("Giant Tower").def().cost), "2 Materials, 3 Gold, 1 Financier");
        assert_eq!(points_parts(id("Cities of Gold").def()), vec!["3 VP".to_string()]);
        assert_eq!(points_parts(id("Transportation Network").def()), vec!["1 VP per Vehicle".to_string()]);
        assert_eq!(points_parts(id("Security Automatons").def()), vec!["1 VP per General".to_string()]);
        assert_eq!(bonus_text(id("Transmutation").def()).as_deref(), Some("a Krystallium"));
        assert_eq!(bonus_text(id("Industrial Complex").def()).as_deref(), Some("a Financier"));
        assert_eq!(bonus_text(id("Juggernaut").def()).as_deref(), Some("2 Generals"));
        assert_eq!(bonus_text(id("Fountain of Youth").def()).as_deref(), Some("3 Krystallium"));
        assert_eq!(article("Energy"), "an");
        assert_eq!(article("Exploration"), "an");
        assert_eq!(article("Gold"), "a");
        assert_eq!(article("General"), "a");
        assert_eq!(bonus_text(plant), None);
        assert!(points_parts(plant).is_empty());
    }

    #[test]
    fn every_card_has_text_for_every_part() {
        for card in catalogue() {
            assert!(!cost_text(&card.cost).is_empty(), "{}", card.name);
            assert!(!kind_class(card.kind).is_empty());
            // Every card does something: produces, scores or pays a bonus.
            let does = !production_parts(card).is_empty() || !points_parts(card).is_empty() || !card.bonus.is_empty();
            assert!(does, "{} does nothing", card.name);
            assert!(!slots(&card.cost, &Cost::default()).is_empty());
        }
    }

    #[test]
    fn names_read_naturally() {
        let n = names();
        assert_eq!(who(0, &n, 0), "You");
        assert_eq!(who(1, &n, 0), "Bob");
        assert_eq!(who(9, &n, 0), "Player 10");
        assert_eq!(join_names(&[], &n), "");
        assert_eq!(join_names(&[1], &n), "Bob");
        assert_eq!(join_names(&[0, 2], &n), "Ann and Cy");
        assert_eq!(join_names(&[0, 1, 2], &n), "Ann, Bob and Cy");
        assert_eq!(empire_name(0), "Noram States");
        assert_eq!(empire_name(5), "Noram States");
        assert_eq!(empire_points(0), "1 VP per Financier");
        assert_eq!(empire_points(4), "3 VP per Discovery");
    }

    #[test]
    fn the_verdict_names_the_winners() {
        let n = names();
        let mut s = planning();
        s.phase = Phase::Over;
        let over = |s: &State| {
            let mut s = s.clone();
            s.scores = (0..3).map(|i| s.score_of(i)).collect();
            s
        };
        // Nobody has anything: everybody ties.
        for p in &mut s.players {
            p.drafted.clear();
        }
        let s = over(&s);
        assert_eq!(verdict(&s.view_for(0), &n), "You, Bob and Cy share the win.");
        assert_eq!(verdict(&s.view_for(1), &n), "You, Ann and Cy share the win.");

        let mut one = s.clone();
        one.players[2].empire = vec![id("Time Travel")];
        let one = over(&one);
        assert_eq!(verdict(&one.view_for(2), &n), "You win!");
        assert_eq!(verdict(&one.view_for(0), &n), "Cy wins.");

        let mut two = s.clone();
        two.players[0].empire = vec![id("Time Travel")];
        two.players[1].empire = vec![id("Time Travel")];
        let two = over(&two);
        assert_eq!(verdict(&two.view_for(1), &n), "You and Ann share the win.");
        assert_eq!(verdict(&two.view_for(2), &n), "Ann and Bob share the win.");
    }

    #[test]
    fn a_card_reads_as_one_sentence() {
        let text = card_summary(id("Super-Sonar").def());
        assert!(text.starts_with("Super-Sonar (Research); costs 4 Science; "), "{text}");
        assert!(text.contains("makes +1 Exploration per Vehicle"), "{text}");
        assert!(text.ends_with("recycles into an Exploration cube"), "{text}");
        let text = card_summary(id("Teleportation").def());
        assert!(text.contains("gives 2 Krystallium when built"), "{text}");
        for card in catalogue() {
            let text = card_summary(card);
            assert!(text.contains(card.name) && text.contains("recycles into"), "{text}");
        }
    }

    #[test]
    fn the_hint_tells_each_player_what_to_do() {
        let n = names();
        let mut s = State::new(3, 4);
        let h = hint(&s.view_for(0), &n);
        assert!(h.contains("Pick a card (1 of 7)") && h.contains("Bob"), "{h}"); // round 1: to the next seat
        s.round = 2;
        let h = hint(&s.view_for(0), &n);
        assert!(h.contains("Cy"), "round 2 goes the other way: {h}");
        s.round = 1;

        let card = s.players[0].hand[0];
        s.apply(0, A::Draft { card }).unwrap();
        assert_eq!(hint(&s.view_for(0), &n), "Waiting for Bob and Cy…");
        assert_eq!(waiting_on(&s.view_for(2)), vec![1, 2]);
        assert!(hint(&s.view_for(1), &n).contains("Pick a card (1 of 7)"));

        let mut s = planning();
        let h = hint(&s.view_for(0), &n);
        assert!(h.contains("Build or recycle each of your 7"), "{h}");
        for card in s.players[0].drafted.clone() {
            s.apply(0, A::Build { card }).unwrap();
        }
        assert!(hint(&s.view_for(0), &n).contains("Done with your cards"));
        s.apply(0, A::Ready).unwrap();
        assert_eq!(hint(&s.view_for(0), &n), "Waiting for Bob and Cy…");
    }

    #[test]
    fn the_hint_covers_production() {
        let n = names();
        let mut s = planning();
        for seat in 0..3 {
            for card in s.players[seat].drafted.clone() {
                s.apply(seat, A::Build { card }).unwrap();
            }
            s.apply(seat, A::Ready).unwrap();
        }
        assert_eq!(phase_title(&s.view_for(0)), "Production: Materials");
        // Seat 0's Empire (Noram States) makes three Materials.
        let h = hint(&s.view_for(0), &n);
        assert!(h.contains("Place your 3 Materials cubes:"), "{h}");
        assert_eq!(s.view_for(0).players[0].pool, 3);

        s.players[0].choose = true;
        assert!(hint(&s.view_for(0), &n).contains("choose a General or a Financier"));
        s.players[0].choose = false;

        s.phase = Phase::Production { step: WRAP_UP };
        s.players[0].ready = false;
        assert_eq!(phase_title(&s.view_for(0)), "Production: Wrap-up");
        assert!(hint(&s.view_for(0), &n).contains("Last chance"));
    }

    #[test]
    fn events_read_as_sentences() {
        let n = names();
        let one = |e: Event| describe_event(&e, &n, 0);
        assert_eq!(one(Event::RoundStarted(2)), "Round 2 of 4 begins.");
        assert_eq!(one(Event::Completed { seat: 0, card: id("Zeppelin") }), "You finished Zeppelin.");
        assert_eq!(one(Event::Completed { seat: 1, card: id("Zeppelin") }), "Bob finished Zeppelin.");
        assert_eq!(
            one(Event::Supremacy { res: Res::Materials, seat: Some(2) }),
            "Cy produced the most Materials and took a Financier."
        );
        assert_eq!(
            one(Event::Supremacy { res: Res::Gold, seat: Some(0) }),
            "You produced the most Gold and took a Financier."
        );
        assert_eq!(
            one(Event::Supremacy { res: Res::Science, seat: Some(1) }),
            "Bob produced the most Science and may choose a character."
        );
        assert_eq!(
            one(Event::Supremacy { res: Res::Energy, seat: None }),
            "Energy tied: nobody takes a General."
        );
        assert_eq!(one(Event::Supremacy { res: Res::Exploration, seat: None }), "Exploration tied: nobody takes a General.");
        assert_eq!(one(Event::Supremacy { res: Res::Science, seat: None }), "Science tied: nobody takes a character.");
        assert_eq!(one(Event::Chose { seat: 1, token: Token::Financier }), "Bob chose a Financier.");
    }

    #[test]
    fn results_are_ranked_like_the_tie_breaks() {
        let n = names();
        let mut s = planning();
        for p in &mut s.players {
            p.drafted.clear();
        }
        s.players[0].empire = vec![id("Human Cloning")]; // 1 point
        s.players[1].empire = vec![id("Giant Tower")]; // 10 points
        s.players[2].empire = vec![id("Island of Avalon")]; // 7 points
        s.phase = Phase::Over;
        s.scores = (0..3).map(|i| s.score_of(i)).collect();
        let rows = results(&s.view_for(0), &n);
        let order: Vec<usize> = rows.iter().map(|r| r.seat).collect();
        assert_eq!(order, vec![1, 2, 0]);
        assert_eq!(rows[0].name, "Bob");
        assert!(rows[0].winner && !rows[1].winner && !rows[2].winner);
        assert_eq!(rows[0].score.total, 10);
    }

    #[test]
    fn finished_cards_are_counted_by_type() {
        let empire = [
            id("Recycling Plant"),
            id("Nuclear Plant"),
            id("Zeppelin"),
            id("Wind Turbines"),
            id("Island of Avalon"),
            id("Neuroscience"),
        ];
        assert_eq!(kind_counts(&empire), [3, 1, 1, 0, 1]);
        assert_eq!(kind_counts(&[]), [0; 5]);
    }

    #[test]
    fn the_hand_goes_to_the_neighbour_the_round_decides() {
        let mut s = State::new(4, 1);
        assert_eq!((pass_to(&s.view_for(0)), pass_to(&s.view_for(3))), (1, 0));
        s.round = 2;
        assert_eq!((pass_to(&s.view_for(0)), pass_to(&s.view_for(3))), (3, 2));
    }
}
