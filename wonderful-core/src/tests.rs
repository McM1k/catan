use super::*;

// ----- helpers ---------------------------------------------------------

/// The first copy of the design called `name`.
fn id(name: &str) -> CardId {
    let at = catalogue().iter().position(|c| c.name == name).unwrap_or_else(|| panic!("no card {name}"));
    CardId(at as u16)
}

/// The second copy of the design called `name`.
fn twin(name: &str) -> CardId {
    CardId(id(name).0 + DESIGNS as u16)
}

/// A game in which everybody has drafted their first card each time.
fn to_planning(n: usize, seed: u64) -> State {
    let mut s = State::new(n, seed);
    for _ in 0..DRAFT_PICKS {
        for seat in 0..n {
            let card = s.players[seat].hand[0];
            s.apply(seat, Action::Draft { card }).unwrap();
        }
    }
    assert_eq!(s.phase, Phase::Planning);
    s
}

/// Planning with nothing drafted, built or finished: a blank slate for
/// setting up a production scenario by hand.
fn blank(n: usize) -> State {
    let mut s = to_planning(n, 7);
    for p in &mut s.players {
        p.drafted.clear();
        p.buildings.clear();
        p.empire.clear();
    }
    s
}

fn start_production(s: &mut State) {
    for seat in 0..s.seats() {
        s.apply(seat, Action::Ready).unwrap();
    }
}

/// Everybody gives up what is left of the current production step.
fn skip_step(s: &mut State) {
    for seat in 0..s.seats() {
        if !matches!(s.phase, Phase::Production { .. }) {
            return; // the last "ready" ended the round
        }
        if s.players[seat].choose {
            s.apply(seat, Action::Choose { token: Token::General }).unwrap();
        }
        s.apply(seat, Action::Ready).unwrap();
    }
}

fn skip_to(s: &mut State, step: u8) {
    while s.phase != (Phase::Production { step }) {
        assert!(matches!(s.phase, Phase::Production { .. }), "left production: {:?}", s.phase);
        skip_step(s);
    }
}

/// Everybody passes until the round is over. (The wrap-up step is skipped
/// by the game itself when nobody has anything to place.)
fn finish_round(s: &mut State) {
    let round = s.round;
    while s.round == round && s.phase != Phase::Over {
        skip_step(s);
    }
}

fn all_cards(s: &State) -> Vec<CardId> {
    let mut v = s.deck.clone();
    v.extend(&s.discard);
    for p in &s.players {
        v.extend(&p.hand);
        v.extend(p.picked);
        v.extend(&p.drafted);
        v.extend(p.buildings.iter().map(|b| b.card));
        v.extend(&p.empire);
    }
    v
}

/// No card is ever lost or duplicated.
fn check_cards(s: &State) {
    let mut v = all_cards(s);
    assert_eq!(v.len(), DECK_SIZE);
    v.sort_by_key(|c| c.0);
    v.dedup();
    assert_eq!(v.len(), DECK_SIZE, "a card is in two places");
}

// ----- the catalogue -----------------------------------------------------

#[test]
fn the_catalogue_has_the_expected_shape() {
    let all = catalogue();
    assert_eq!(all.len(), DESIGNS);
    assert_eq!(DECK_SIZE, 150);
    for kind in Kind::ALL {
        let of_kind = all.iter().filter(|c| c.kind == kind).count();
        assert_eq!(of_kind, DESIGNS / 5, "{kind:?}");
    }
    let mut names: Vec<_> = all.iter().map(|c| c.name).collect();
    names.sort();
    names.dedup();
    assert_eq!(names.len(), DESIGNS, "names must be unique");
    for c in all {
        let total = c.cost.total();
        assert!((1..=20).contains(&total), "{} costs {total}", c.name);
        assert!(c.vp <= 12, "{}", c.name);
    }
}

#[test]
fn every_resource_and_every_character_can_be_had() {
    let all = catalogue();
    for res in Res::ALL {
        assert!(all.iter().any(|c| c.produces[res.index()] > 0), "nothing produces {res:?}");
        assert!(all.iter().any(|c| c.recycle == res), "nothing recycles into {res:?}");
        assert!(all.iter().any(|c| c.cost.res[res.index()] > 0), "nothing costs {res:?}");
    }
    assert!(all.iter().any(|c| c.cost.generals > 0));
    assert!(all.iter().any(|c| c.cost.financiers > 0));
    assert!(all.iter().any(|c| c.bonus == Some(Bonus::Krystallium)));
    assert!(all.iter().any(|c| c.bonus == Some(Bonus::Token(Token::General))));
    assert!(all.iter().any(|c| c.bonus == Some(Bonus::Token(Token::Financier))));
    assert!(all.iter().any(|c| matches!(c.bonus, Some(Bonus::Cube(_)))));
}

#[test]
fn copies_share_a_design() {
    assert_eq!(CardId(0).def().name, CardId(DESIGNS as u16).def().name);
    assert_eq!(id("Quarry").def().name, "Quarry");
    assert_eq!(twin("Quarry").def().name, "Quarry");
    assert_ne!(id("Quarry"), twin("Quarry"));
    // The last card of the deck is a real design too.
    assert!(!CardId(DECK_SIZE as u16 - 1).def().name.is_empty());
}

#[test]
fn cost_arithmetic() {
    let cost = Cost { res: [3, 0, 1, 0, 0], generals: 1, financiers: 0 };
    assert_eq!(cost.total(), 5);
    let half = Cost { res: [1, 5, 1, 0, 0], generals: 0, financiers: 3 };
    let left = cost.minus(&half);
    assert_eq!(left, Cost { res: [2, 0, 0, 0, 0], generals: 1, financiers: 0 });
    assert!(!left.is_zero());
    assert!(cost.minus(&cost).is_zero());
}

#[test]
fn supremacy_characters() {
    assert_eq!(Res::Materials.supremacy_token(), Some(Token::General));
    assert_eq!(Res::Energy.supremacy_token(), Some(Token::General));
    assert_eq!(Res::Gold.supremacy_token(), Some(Token::Financier));
    assert_eq!(Res::Exploration.supremacy_token(), Some(Token::Financier));
    assert_eq!(Res::Science.supremacy_token(), None);
}

// ----- dealing -----------------------------------------------------------

#[test]
fn a_new_game_deals_a_hand_to_everybody() {
    for n in MIN_PLAYERS..=MAX_PLAYERS {
        let s = State::new(n, 5);
        let size = if n == 2 { 10 } else { 7 };
        assert_eq!(hand_size(n), size);
        assert_eq!((s.round, s.phase), (1, Phase::Draft));
        assert_eq!(s.deck.len(), DECK_SIZE - n * size);
        for p in &s.players {
            assert_eq!(p.hand.len(), size);
            assert!(p.picked.is_none() && p.drafted.is_empty());
        }
        assert_eq!(s.log, vec![Event::RoundStarted(1)]);
        check_cards(&s);
    }
}

#[test]
fn the_seed_decides_the_deal() {
    let a = State::new(3, 1);
    let b = State::new(3, 1);
    let c = State::new(3, 2);
    assert_eq!(a, b);
    assert_ne!(a.players[0].hand, c.players[0].hand);
}

#[test]
#[should_panic]
fn one_player_is_not_enough() {
    State::new(1, 0);
}

#[test]
#[should_panic]
fn six_players_do_not_fit() {
    State::new(6, 0);
}

// ----- the draft -----------------------------------------------------------

#[test]
fn hands_go_to_the_next_seat_in_round_one() {
    let mut s = State::new(4, 11);
    let before: Vec<Vec<CardId>> = s.players.iter().map(|p| p.hand.clone()).collect();
    let picks: Vec<CardId> = before.iter().map(|h| h[0]).collect();
    for (seat, &card) in picks.iter().enumerate() {
        s.apply(seat, Action::Draft { card }).unwrap();
    }
    for (seat, &pick) in picks.iter().enumerate() {
        let from = (seat + 3) % 4;
        assert_eq!(s.players[seat].hand, before[from][1..], "seat {seat} should get seat {from}'s hand");
        assert_eq!(s.players[seat].drafted, vec![pick]);
    }
}

#[test]
fn hands_go_to_the_previous_seat_in_round_two() {
    let mut s = State::new(4, 11);
    s.round = 2;
    assert!(!s.passes_to_next());
    let before: Vec<Vec<CardId>> = s.players.iter().map(|p| p.hand.clone()).collect();
    for (seat, hand) in before.iter().enumerate() {
        s.apply(seat, Action::Draft { card: hand[0] }).unwrap();
    }
    for seat in 0..4 {
        let from = (seat + 1) % 4;
        assert_eq!(s.players[seat].hand, before[from][1..], "seat {seat} should get seat {from}'s hand");
    }
    s.round = 3;
    assert!(s.passes_to_next());
}

#[test]
fn nothing_moves_until_everybody_has_picked() {
    let mut s = State::new(3, 4);
    let hands: Vec<Vec<CardId>> = s.players.iter().map(|p| p.hand.clone()).collect();
    let card = hands[0][2];
    s.apply(0, Action::Draft { card }).unwrap();
    // Seat 0 holds the others back and keeps its pick face down.
    assert_eq!(s.players[0].picked, Some(card));
    assert!(s.players[0].drafted.is_empty());
    assert_eq!(s.players[1].hand, hands[1]);
    assert_eq!(s.apply(0, Action::Draft { card: hands[0][0] }), Err(Error::AlreadyPicked));
    s.apply(1, Action::Draft { card: hands[1][0] }).unwrap();
    assert_eq!(s.players[2].hand, hands[2], "still waiting for seat 2");
    s.apply(2, Action::Draft { card: hands[2][0] }).unwrap();
    assert!(s.players.iter().all(|p| p.picked.is_none() && p.drafted.len() == 1));
}

#[test]
fn a_pick_must_come_from_your_own_hand() {
    let mut s = State::new(3, 4);
    let other = s.players[1].hand[0];
    assert_eq!(s.apply(0, Action::Draft { card: other }), Err(Error::NotInHand));
    assert_eq!(s.apply(0, Action::Draft { card: CardId(9999) }), Err(Error::NotInHand));
    assert_eq!(s.apply(7, Action::Ready), Err(Error::UnknownSeat));
}

#[test]
fn only_draft_moves_work_during_the_draft() {
    let mut s = State::new(3, 4);
    let card = s.players[0].hand[0];
    assert_eq!(s.apply(0, Action::Build { card }), Err(Error::WrongPhase));
    assert_eq!(s.apply(0, Action::Recycle { card, to: Target::Empire }), Err(Error::WrongPhase));
    assert_eq!(s.apply(0, Action::Ready), Err(Error::WrongPhase));
    assert_eq!(
        s.apply(0, Action::Place { piece: Piece::Cube(Res::Gold), target: Target::Empire }),
        Err(Error::WrongPhase)
    );
    assert_eq!(s.apply(0, Action::Choose { token: Token::General }), Err(Error::WrongPhase));
    assert_eq!(s.apply(0, Action::Scrap { card, to: Target::Empire }), Err(Error::WrongPhase));
}

#[test]
fn seven_picks_end_the_draft() {
    for n in 3..=MAX_PLAYERS {
        let s = to_planning(n, 21);
        for p in &s.players {
            assert_eq!(p.drafted.len(), DRAFT_PICKS);
            assert!(p.hand.is_empty() && p.picked.is_none() && !p.ready);
        }
        assert!(s.discard.is_empty(), "nothing is left over with {n} players");
        check_cards(&s);
    }
}

#[test]
fn with_two_players_the_leftovers_are_discarded() {
    let s = to_planning(2, 21);
    for p in &s.players {
        assert_eq!(p.drafted.len(), DRAFT_PICKS);
        assert!(p.hand.is_empty());
    }
    assert_eq!(s.discard.len(), 6, "3 cards each");
    assert!(s.players.iter().all(|p| p.pending == [0; 5] && p.empire_cubes == 0), "no recycling bonus");
    check_cards(&s);
}

// ----- planning ------------------------------------------------------------

#[test]
fn building_puts_a_card_under_construction() {
    let mut s = to_planning(3, 9);
    let card = s.players[1].drafted[3];
    s.apply(1, Action::Build { card }).unwrap();
    assert_eq!(s.players[1].drafted.len(), 6);
    assert_eq!(s.players[1].buildings, vec![Building { card, filled: Cost::default() }]);
    // It can't be built twice, and it isn't someone else's.
    assert_eq!(s.apply(1, Action::Build { card }), Err(Error::NotDrafted));
    let theirs = s.players[0].drafted[0];
    assert_eq!(s.apply(1, Action::Build { card: theirs }), Err(Error::NotDrafted));
}

#[test]
fn recycling_gives_its_cube_to_the_empire() {
    let mut s = to_planning(3, 9);
    let card = s.players[0].drafted[0];
    s.apply(0, Action::Recycle { card, to: Target::Empire }).unwrap();
    assert_eq!(s.players[0].empire_cubes, 1);
    assert_eq!(s.players[0].drafted.len(), 6);
    assert_eq!(s.discard, vec![card]);
    check_cards(&s);
}

#[test]
fn a_recycled_cube_must_fit_where_it_goes() {
    let mut s = blank(2);
    s.players[0].drafted = vec![id("Quarry"), id("Wind Farm"), id("Caravan")];
    s.apply(0, Action::Build { card: id("Quarry") }).unwrap();
    let discarded = s.discard.len(); // the two-player draft already discarded some
    // Wind Farm recycles into Energy; the Quarry only has spaces for Materials.
    let to = Target::Card(id("Quarry"));
    assert_eq!(s.apply(0, Action::Recycle { card: id("Wind Farm"), to }), Err(Error::InvalidTarget));
    // Nothing happened: the card is still there and not discarded.
    assert!(s.players[0].drafted.contains(&id("Wind Farm")));
    assert_eq!(s.discard.len(), discarded);
    // A building you don't have is no target either.
    let nowhere = Target::Card(id("Smelter"));
    assert_eq!(s.apply(0, Action::Recycle { card: id("Wind Farm"), to: nowhere }), Err(Error::InvalidTarget));
    assert_eq!(s.apply(0, Action::Recycle { card: id("Wind Farm"), to: Target::Empire }), Ok(()));
    assert_eq!(s.discard.len(), discarded + 1);
}

#[test]
fn recycling_can_finish_a_building_at_once() {
    let mut s = blank(2);
    s.players[0].buildings = vec![Building {
        card: id("Hover Cart"),
        filled: Cost { res: [1, 0, 0, 0, 0], ..Cost::default() },
    }];
    s.players[0].drafted = vec![id("Wind Farm")]; // recycles into Energy
    s.apply(0, Action::Recycle { card: id("Wind Farm"), to: Target::Card(id("Hover Cart")) }).unwrap();
    assert!(s.players[0].buildings.is_empty());
    assert_eq!(s.players[0].empire, vec![id("Hover Cart")]);
    assert!(s.log.contains(&Event::Completed { seat: 0, card: id("Hover Cart") }));
    // It produces from now on: the Empire's Energy icon plus the cart's.
    assert_eq!(s.production(0, Res::Energy), 2);
}

#[test]
fn finishing_a_building_pays_its_bonus() {
    let almost = |card: &str, missing: usize| Building {
        card: id(card),
        filled: {
            let mut filled = id(card).def().cost;
            filled.res[missing] -= 1;
            filled
        },
    };
    let mut s = blank(2);
    s.players[0].buildings = vec![
        almost("Crystal Cave", Res::Exploration.index()), // Krystallium
        almost("Bazaar", Res::Gold.index()),              // a Financier
        almost("Founding Charter", Res::Gold.index()),    // a General
        almost("Meteor Mine", Res::Exploration.index()),  // a Materials cube
    ];
    s.players[0].drafted = vec![id("Skiff"), twin("Skiff"), id("Caravan"), twin("Caravan")]; // X, X, G, G
    let finish = |s: &mut State, card: CardId, to: &str| {
        s.apply(0, Action::Recycle { card, to: Target::Card(id(to)) }).unwrap();
    };
    finish(&mut s, id("Skiff"), "Crystal Cave");
    assert_eq!(s.players[0].krystallium, 1);
    finish(&mut s, id("Caravan"), "Bazaar");
    assert_eq!(s.players[0].financiers, 1);
    finish(&mut s, twin("Caravan"), "Founding Charter");
    assert_eq!(s.players[0].generals, 1);
    finish(&mut s, twin("Skiff"), "Meteor Mine");
    assert_eq!(s.players[0].pending, [1, 0, 0, 0, 0]);
    assert_eq!(s.players[0].empire.len(), 4);
}

#[test]
fn scrapping_loses_what_was_placed_but_pays_the_recycling_cube() {
    let mut s = blank(2);
    s.players[0].buildings = vec![
        Building { card: id("Smelter"), filled: Cost { res: [2, 1, 0, 0, 0], ..Cost::default() } },
        Building { card: id("Quarry"), filled: Cost::default() },
    ];
    // Into itself is no good; into the other building is fine (Smelter
    // recycles into Energy, which the Quarry has no space for).
    assert_eq!(
        s.apply(0, Action::Scrap { card: id("Smelter"), to: Target::Card(id("Smelter")) }),
        Err(Error::InvalidTarget)
    );
    assert_eq!(
        s.apply(0, Action::Scrap { card: id("Smelter"), to: Target::Card(id("Quarry")) }),
        Err(Error::InvalidTarget)
    );
    s.apply(0, Action::Scrap { card: id("Smelter"), to: Target::Empire }).unwrap();
    assert_eq!(s.players[0].buildings.len(), 1);
    assert_eq!(s.players[0].empire_cubes, 1);
    assert_eq!(s.discard.last(), Some(&id("Smelter")));
    assert_eq!(s.apply(0, Action::Scrap { card: id("Smelter"), to: Target::Empire }), Err(Error::NotUnderConstruction));
}

#[test]
fn planning_ends_when_everyone_has_decided_everything() {
    let mut s = to_planning(2, 3);
    // Cards left to decide: not ready yet.
    assert_eq!(s.apply(0, Action::Ready), Err(Error::CardsLeftToPlan));
    for seat in 0..2 {
        for card in s.players[seat].drafted.clone() {
            s.apply(seat, Action::Build { card }).unwrap();
        }
    }
    s.apply(0, Action::Ready).unwrap();
    assert_eq!(s.phase, Phase::Planning, "seat 1 is still planning");
    assert!(s.players[0].ready);
    s.apply(1, Action::Ready).unwrap();
    assert!(matches!(s.phase, Phase::Production { .. }));
    check_cards(&s);
}

// ----- production ----------------------------------------------------------

#[test]
fn production_adds_the_empire_cards_and_scaling_icons() {
    let mut s = blank(2);
    // Aurelian Union starts with 1 Materials and 1 Gold.
    assert_eq!(s.production(0, Res::Materials), 1);
    assert_eq!(s.production(0, Res::Gold), 1);
    assert_eq!(s.production(1, Res::Energy), 1);
    // Quarry 1 + Smelter 2 + Mega Foundry (1 per Structure: all three are).
    s.players[0].empire = vec![id("Quarry"), id("Smelter"), id("Mega Foundry")];
    assert_eq!(s.production(0, Res::Materials), 1 + 1 + 2 + 3);
    // Skiff 1 + Tram Network (1 per Vehicle: both are).
    s.players[0].empire.extend([id("Skiff"), id("Tram Network")]);
    assert_eq!(s.production(0, Res::Exploration), 1 + 2);
    // Cards of other types don't count for the scaling.
    s.players[0].empire.push(id("Quarry"));
    assert_eq!(s.production(0, Res::Materials), 1 + 1 + 1 + 2 + 4);
}

#[test]
fn the_best_producer_takes_the_character() {
    let mut s = blank(2);
    s.players[0].empire = vec![id("Quarry")]; // 2 Materials against 0
    start_production(&mut s);
    assert_eq!(s.phase, Phase::Production { step: 0 });
    assert_eq!(s.players[0].generals, 1);
    assert_eq!(s.players[1].generals + s.players[1].financiers, 0);
    assert_eq!((s.players[0].pool, s.players[0].produced), (2, 2));
    assert!(!s.players[0].ready, "has cubes to place");
    assert!(s.players[1].ready, "has nothing to do in this step");
    assert!(s.log.contains(&Event::Supremacy { res: Res::Materials, seat: Some(0) }));
}

#[test]
fn gold_and_exploration_give_financiers() {
    let mut s = blank(2);
    s.players[1].empire = vec![id("Granary"), id("Lost Ruins")]; // Gold 1, Exploration 1
    start_production(&mut s);
    skip_to(&mut s, 3); // Gold: seat 0 has 1 (Aurelian), seat 1 has 1 (Granary) -> tie
    assert_eq!(s.players[0].financiers + s.players[1].financiers, 0);
    assert!(s.log.contains(&Event::Supremacy { res: Res::Gold, seat: None }));
    skip_to(&mut s, 4); // Exploration: only seat 1 produces any
    assert_eq!(s.players[1].financiers, 1);
    assert!(s.log.contains(&Event::Supremacy { res: Res::Exploration, seat: Some(1) }));
}

#[test]
fn a_tie_for_the_most_gives_nobody_a_character() {
    let mut s = blank(2);
    start_production(&mut s);
    assert_eq!(s.players[0].generals, 1, "Materials: only seat 0 has any");
    skip_to(&mut s, 1);
    // Both Empires start with one Energy icon.
    assert_eq!((s.players[0].produced, s.players[1].produced), (1, 1));
    assert!(s.log.contains(&Event::Supremacy { res: Res::Energy, seat: None }));
    assert_eq!(s.players[0].generals, 1);
    assert_eq!(s.players[1].generals + s.players[1].financiers, 0, "nobody took the Energy character");
}

#[test]
fn every_resource_is_covered_by_the_empires() {
    // Each Empire starts with three icons, and each resource is on three Empires.
    for empire in EMPIRES {
        assert_eq!(empire.base.iter().map(|&n| n as u32).sum::<u32>(), 3, "{}", empire.name);
    }
    for res in Res::ALL {
        assert_eq!(EMPIRES.iter().filter(|e| e.base[res.index()] > 0).count(), 3, "{res:?}");
    }
    let mut names: Vec<_> = EMPIRES.iter().map(|e| e.name).collect();
    names.sort();
    names.dedup();
    assert_eq!(names.len(), 5);
}

#[test]
fn the_science_winner_chooses_the_character() {
    let mut s = blank(2);
    s.players[0].empire = vec![id("Observatory")]; // Science 2 against 1
    start_production(&mut s);
    skip_to(&mut s, 2);
    assert!(s.players[0].choose);
    assert!(!s.players[1].choose);
    assert_eq!(s.apply(0, Action::Ready), Err(Error::MustChoose));
    assert_eq!(s.apply(1, Action::Choose { token: Token::General }), Err(Error::NothingToChoose));
    s.apply(0, Action::Choose { token: Token::Financier }).unwrap();
    assert!(!s.players[0].choose);
    assert_eq!(s.players[0].financiers, 1);
    assert!(s.log.contains(&Event::Chose { seat: 0, token: Token::Financier }));
    assert_eq!(s.apply(0, Action::Choose { token: Token::General }), Err(Error::NothingToChoose));
    // The cubes are still to be placed.
    assert!(!s.players[0].ready);
}

#[test]
fn cubes_come_from_the_pool_and_the_rest_is_lost() {
    let mut s = blank(2);
    s.players[0].empire = vec![id("Smelter")]; // Materials 1 + 2 = 3
    s.players[0].buildings = vec![Building { card: id("Quarry"), filled: Cost::default() }]; // needs 2 Materials
    start_production(&mut s);
    assert_eq!(s.players[0].pool, 3);

    let materials = Piece::Cube(Res::Materials);
    let quarry = Target::Card(id("Quarry"));
    // Wrong resource for this step, and a building that isn't there.
    assert_eq!(
        s.apply(0, Action::Place { piece: Piece::Cube(Res::Energy), target: quarry }),
        Err(Error::NothingToPlace)
    );
    assert_eq!(
        s.apply(0, Action::Place { piece: materials, target: Target::Card(id("Smelter")) }),
        Err(Error::InvalidTarget)
    );
    assert_eq!(s.players[0].pool, 3, "refused moves cost nothing");

    s.apply(0, Action::Place { piece: materials, target: quarry }).unwrap();
    assert_eq!(s.players[0].pool, 2);
    assert_eq!(s.players[0].buildings[0].filled.res[0], 1);
    s.apply(0, Action::Place { piece: materials, target: quarry }).unwrap();
    assert!(s.players[0].buildings.is_empty(), "the Quarry is finished");
    assert!(s.players[0].empire.contains(&id("Quarry")));
    // The Quarry is full now (and gone), so the last cube goes to the Empire.
    assert_eq!(s.apply(0, Action::Place { piece: materials, target: quarry }), Err(Error::InvalidTarget));
    s.apply(0, Action::Place { piece: materials, target: Target::Empire }).unwrap();
    assert_eq!(s.players[0].empire_cubes, 1);
    // Everything was placed and seat 1 had nothing to do: Energy is next.
    assert_eq!(s.phase, Phase::Production { step: 1 });
    assert_eq!(s.players[0].pool, 1, "that is the new step's cube, not a left-over");
}

#[test]
fn saying_ready_throws_away_the_cubes_left() {
    let mut s = blank(2);
    s.players[0].empire = vec![id("Smelter")];
    start_production(&mut s);
    assert_eq!(s.players[0].pool, 3);
    s.apply(0, Action::Ready).unwrap();
    assert_eq!(s.players[0].empire_cubes, 0, "nothing was placed");
    // Seat 1 had nothing to do in the Materials step, so the game moved on
    // to Energy, where both have a cube; the Materials cubes are gone.
    assert_eq!(s.phase, Phase::Production { step: 1 });
    assert_eq!(s.players[0].produced, 1);
    assert_eq!(s.players[0].pool, 1);
    assert_eq!(s.players[0].pending, [0; 5]);
}

#[test]
fn five_cubes_on_the_empire_make_krystallium() {
    let mut s = blank(2);
    s.players[0].empire = vec![id("Smelter"), twin("Smelter"), id("Great Wall")]; // 1 + 2 + 2 + 2
    start_production(&mut s);
    assert_eq!(s.players[0].pool, 7);
    for _ in 0..5 {
        s.apply(0, Action::Place { piece: Piece::Cube(Res::Materials), target: Target::Empire }).unwrap();
    }
    assert_eq!((s.players[0].empire_cubes, s.players[0].krystallium), (0, 1));
    assert_eq!(s.players[0].pool, 2);
    for _ in 0..2 {
        s.apply(0, Action::Place { piece: Piece::Cube(Res::Materials), target: Target::Empire }).unwrap();
    }
    assert_eq!((s.players[0].empire_cubes, s.players[0].krystallium), (2, 1));
}

#[test]
fn krystallium_stands_in_for_any_cube_but_not_for_characters() {
    let mut s = blank(2);
    // Seat 1 produces no Materials, so it takes no character at the start.
    // Bastion: 3 Materials and a General.
    s.players[1].buildings = vec![Building { card: id("Bastion"), filled: Cost::default() }];
    s.players[1].krystallium = 3;
    s.players[1].generals = 1;
    s.players[1].financiers = 1;
    start_production(&mut s);
    let bastion = Target::Card(id("Bastion"));
    // No Energy space on it, nor a character space for a Financier.
    assert_eq!(
        s.apply(1, Action::Place { piece: Piece::Krystallium(Res::Energy), target: bastion }),
        Err(Error::InvalidTarget)
    );
    assert_eq!(s.apply(1, Action::Place { piece: Piece::Financier, target: bastion }), Err(Error::InvalidTarget));
    // Krystallium doesn't go on the Empire.
    assert_eq!(
        s.apply(1, Action::Place { piece: Piece::Krystallium(Res::Materials), target: Target::Empire }),
        Err(Error::InvalidTarget)
    );
    for _ in 0..3 {
        s.apply(1, Action::Place { piece: Piece::Krystallium(Res::Materials), target: bastion }).unwrap();
    }
    assert_eq!(s.players[1].krystallium, 0);
    assert_eq!(s.players[1].buildings.len(), 1, "still needs its General");
    s.apply(1, Action::Place { piece: Piece::General, target: bastion }).unwrap();
    assert!(s.players[1].buildings.is_empty());
    assert!(s.players[1].empire.contains(&id("Bastion")));
    assert_eq!(s.players[1].generals, 0);
}

#[test]
fn you_cannot_place_what_you_do_not_hold() {
    let mut s = blank(2);
    s.players[1].buildings = vec![Building { card: id("Bastion"), filled: Cost::default() }];
    start_production(&mut s);
    let bastion = Target::Card(id("Bastion"));
    assert_eq!(s.apply(1, Action::Place { piece: Piece::General, target: bastion }), Err(Error::NothingToPlace));
    assert_eq!(
        s.apply(1, Action::Place { piece: Piece::Krystallium(Res::Materials), target: bastion }),
        Err(Error::NothingToPlace)
    );
}

#[test]
fn bonus_cubes_can_be_placed_whenever() {
    let mut s = blank(2);
    s.players[0].pending = [0, 1, 0, 0, 0];
    s.players[0].buildings = vec![Building { card: id("Hover Cart"), filled: Cost::default() }];
    // In planning already...
    s.apply(0, Action::Place { piece: Piece::Cube(Res::Energy), target: Target::Card(id("Hover Cart")) }).unwrap();
    assert_eq!(s.players[0].pending, [0; 5]);
    assert_eq!(
        s.apply(0, Action::Place { piece: Piece::Cube(Res::Energy), target: Target::Empire }),
        Err(Error::NothingToPlace)
    );
}

#[test]
fn a_building_finished_in_one_step_produces_in_the_next() {
    let mut s = blank(2);
    s.players[0].empire = vec![id("Quarry")]; // Materials 2
    s.players[0].buildings = vec![Building {
        card: id("Hover Cart"),
        filled: Cost { res: [1, 0, 0, 0, 0], ..Cost::default() },
    }];
    s.players[0].pending = [0, 1, 0, 0, 0];
    start_production(&mut s);
    assert_eq!(s.players[0].produced, 2);
    assert_eq!(s.players[1].produced, 0);
    // Finish the Hover Cart (Energy 1) with a bonus cube during Materials...
    s.apply(0, Action::Place { piece: Piece::Cube(Res::Energy), target: Target::Card(id("Hover Cart")) }).unwrap();
    skip_to(&mut s, 1);
    // ...and it already produces Energy in the next step: the Empire's 1 + the cart's 1.
    assert_eq!(s.players[0].produced, 2);
}

#[test]
fn only_players_with_something_to_place_wait_in_the_wrap_up() {
    let mut s = blank(2);
    s.players[0].buildings = vec![Building { card: id("Quarry"), filled: Cost::default() }];
    s.players[0].krystallium = 1;
    s.players[1].krystallium = 1; // nothing to put it on
    start_production(&mut s);
    skip_to(&mut s, WRAP_UP);
    assert!(!s.players[0].ready);
    assert!(s.players[1].ready);
    assert_eq!(s.round, 1);
    s.apply(0, Action::Place { piece: Piece::Krystallium(Res::Materials), target: Target::Card(id("Quarry")) }).unwrap();
    assert_eq!(s.phase, Phase::Production { step: WRAP_UP }, "still going: it may place more");
    s.apply(0, Action::Ready).unwrap();
    assert_eq!((s.round, s.phase), (2, Phase::Draft));
}

#[test]
fn the_round_ends_by_dealing_the_next_one() {
    let mut s = blank(2);
    s.players[0].buildings = vec![Building { card: id("Quarry"), filled: Cost::default() }];
    s.players[0].pending = [3, 0, 0, 0, 0];
    s.players[0].krystallium = 2;
    start_production(&mut s);
    skip_to(&mut s, WRAP_UP);
    s.apply(0, Action::Ready).unwrap();
    assert_eq!((s.round, s.phase), (2, Phase::Draft));
    // Leftover bonus cubes went to the Empire; buildings and Krystallium stay.
    assert_eq!(s.players[0].pending, [0; 5]);
    assert_eq!(s.players[0].empire_cubes, 3);
    assert_eq!(s.players[0].buildings.len(), 1);
    assert_eq!(s.players[0].krystallium, 2);
    // New hands, nobody ready, nothing picked.
    for p in &s.players {
        assert_eq!(p.hand.len(), 10);
        assert!(p.picked.is_none() && p.drafted.is_empty() && !p.ready && p.pool == 0);
    }
    assert_eq!(s.deck.len(), DECK_SIZE - 2 * 2 * 10);
    assert_eq!(s.log.last(), Some(&Event::RoundStarted(2)));
}

#[test]
fn leftover_bonus_cubes_still_make_krystallium() {
    let mut s = blank(2);
    s.players[0].pending = [0, 0, 0, 0, 5];
    start_production(&mut s);
    // Nobody can place anything in the wrap-up, so the game skips it.
    finish_round(&mut s);
    assert_eq!(s.round, 2);
    assert_eq!((s.players[0].krystallium, s.players[0].empire_cubes), (1, 0));
    assert_eq!(s.players[0].pending, [0; 5]);
}

// ----- scoring -------------------------------------------------------------

#[test]
fn points_add_up_from_cards_types_and_characters() {
    let mut s = blank(2);
    s.players[0].empire = vec![id("Skyline Tower"), id("Quarry"), id("Smelter"), id("Archive Vault")];
    s.players[0].generals = 1;
    s.players[0].financiers = 2;
    let score = s.score_of(0);
    // Printed points 3 + 1 + 1 + 2.
    assert_eq!(score.gross, 7);
    // The Tower: 1 per Structure (all four); the Vault: 1 per Financier (2).
    assert_eq!(score.combo, 4 + 2);
    assert_eq!((score.generals, score.financiers), (1, 2));
    assert_eq!(score.total, 7 + 6 + 1 + 2);
    assert_eq!(score.cards, 4);
    // Cubes, Krystallium and unfinished buildings are worth nothing.
    s.players[0].krystallium = 4;
    s.players[0].empire_cubes = 3;
    s.players[0].buildings = vec![Building { card: id("Monument"), filled: Cost::default() }];
    assert_eq!(s.score_of(0), score);
}

#[test]
fn combos_count_the_cards_you_own_of_a_type() {
    let mut s = blank(2);
    s.players[0].empire = vec![id("Legacy Hall"), id("Opera House"), id("Aqueduct"), id("Quarry")];
    // Legacy Hall: 2 per Project (it, the Opera House and the Aqueduct).
    let score = s.score_of(0);
    assert_eq!(score.combo, 6);
    assert_eq!(score.gross, 2 + 4 + 2 + 1);
}

fn finished(s: &mut State) {
    s.phase = Phase::Over;
    s.scores = (0..s.seats()).map(|i| s.score_of(i)).collect();
}

#[test]
fn the_most_points_win() {
    let mut s = blank(3);
    s.players[0].empire = vec![id("Monument")];
    s.players[1].empire = vec![id("Utopia Plan")];
    s.players[2].empire = vec![id("Quarry")];
    assert!(s.winners().is_empty(), "nobody wins before the end");
    finished(&mut s);
    assert_eq!(s.winners(), vec![1]);
}

#[test]
fn ties_go_to_the_most_cards_then_the_most_characters_then_are_shared() {
    // Same points: one point and a General against a single card of one point.
    let mut s = blank(2);
    s.players[0].empire = vec![id("Quarry")];
    s.players[1].generals = 1;
    s.players[1].empire = vec![];
    finished(&mut s);
    assert_eq!((s.scores[0].total, s.scores[1].total), (1, 1));
    assert_eq!(s.winners(), vec![0], "more finished cards");

    // Same points, same number of cards: 1 + 2 characters against 3 points.
    let mut s = blank(2);
    s.players[0].empire = vec![id("Smelter")];
    s.players[0].generals = 2;
    s.players[1].empire = vec![id("Bastion")];
    finished(&mut s);
    assert_eq!((s.scores[0].total, s.scores[1].total), (3, 3));
    assert_eq!(s.winners(), vec![0], "more characters");

    // Everything level: they share the win.
    let mut s = blank(2);
    s.players[0].empire = vec![id("Quarry")];
    s.players[1].empire = vec![twin("Quarry")];
    finished(&mut s);
    assert_eq!(s.winners(), vec![0, 1]);
}

// ----- whole games ---------------------------------------------------------

/// A simple legal player: drafts what is first, builds two cards out of
/// three and recycles the rest into the Empire, puts every cube on the
/// first building with room (else the Empire) and uses what it holds.
fn bot(s: &State, seat: usize, turn: usize) -> Option<Action> {
    let p = &s.players[seat];
    match s.phase {
        Phase::Draft => {
            let card = *p.hand.get(turn % p.hand.len().max(1))?;
            p.picked.is_none().then_some(Action::Draft { card })
        }
        Phase::Planning => match p.drafted.first() {
            Some(&card) if !turn.is_multiple_of(3) => Some(Action::Build { card }),
            Some(&card) => {
                let res = card.def().recycle;
                let to = p
                    .buildings
                    .iter()
                    .find(|b| b.remaining().res[res.index()] > 0)
                    .map_or(Target::Empire, |b| Target::Card(b.card));
                Some(Action::Recycle { card, to })
            }
            None => (!p.ready).then_some(Action::Ready),
        },
        Phase::Production { step } if (step as usize) < Res::ALL.len() => {
            if p.choose {
                let token = if turn.is_multiple_of(2) { Token::General } else { Token::Financier };
                return Some(Action::Choose { token });
            }
            if p.ready {
                return None;
            }
            let res = Res::ALL[step as usize];
            let target = p
                .buildings
                .iter()
                .find(|b| b.remaining().res[res.index()] > 0)
                .map_or(Target::Empire, |b| Target::Card(b.card));
            Some(Action::Place { piece: Piece::Cube(res), target })
        }
        Phase::Production { .. } => {
            if p.ready {
                return None;
            }
            for b in &p.buildings {
                let left = b.remaining();
                let target = Target::Card(b.card);
                if p.krystallium > 0 {
                    if let Some(res) = Res::ALL.iter().find(|r| left.res[r.index()] > 0) {
                        return Some(Action::Place { piece: Piece::Krystallium(*res), target });
                    }
                }
                if p.generals > 0 && left.generals > 0 {
                    return Some(Action::Place { piece: Piece::General, target });
                }
                if p.financiers > 0 && left.financiers > 0 {
                    return Some(Action::Place { piece: Piece::Financier, target });
                }
            }
            Some(Action::Ready)
        }
        Phase::Over => None,
    }
}

/// A player that thinks a little: it drafts cheap, productive cards, builds
/// only what it can plausibly finish, aims recycling cubes and produced
/// cubes at the building closest to done, and spends characters and
/// Krystallium at the end of the round.
const OPEN_BUILDINGS: usize = 3;

fn smart_bot(s: &State, seat: usize, _turn: usize) -> Option<Action> {
    let p = &s.players[seat];
    let value = |id: CardId| -> f64 {
        let c = id.def();
        let icons: u32 = c.produces.iter().map(|&n| n as u32).sum::<u32>() + c.scaled.map_or(0, |_| 2);
        let points = c.vp as f64 + c.combo.map_or(0.0, |(_, v)| v as f64 * 3.0) + c.per_token.map_or(0.0, |(_, v)| v as f64 * 2.0);
        let late = (ROUNDS - s.round) as f64; // production is worth more early
        (icons as f64 * (1.0 + late) * 1.5 + points) / c.cost.total() as f64
    };
    // The building that needs `res` and is closest to done.
    let nearest = |res: Res| {
        p.buildings
            .iter()
            .filter(|b| b.remaining().res[res.index()] > 0)
            .min_by_key(|b| b.remaining().total())
            .map(|b| Target::Card(b.card))
    };
    match s.phase {
        Phase::Draft => {
            let card = *p.hand.iter().max_by(|a, b| value(**a).total_cmp(&value(**b)))?;
            p.picked.is_none().then_some(Action::Draft { card })
        }
        Phase::Planning => {
            // Build the best cards that are cheap enough for this stage.
            let limit = 4 + 2 * (s.round as u32 - 1);
            let mut by_value = p.drafted.clone();
            by_value.sort_by(|a, b| value(*b).total_cmp(&value(*a)));
            // Only a few buildings at a time, so the cubes can finish them.
            let wanted = by_value
                .iter()
                .copied()
                .find(|c| c.def().cost.total() <= limit)
                .filter(|_| p.buildings.len() < OPEN_BUILDINGS);
            match (wanted, by_value.first()) {
                (Some(card), _) => Some(Action::Build { card }),
                (None, Some(&card)) => {
                    let to = nearest(card.def().recycle).unwrap_or(Target::Empire);
                    Some(Action::Recycle { card, to })
                }
                (None, None) => (!p.ready).then_some(Action::Ready),
            }
        }
        Phase::Production { step } if (step as usize) < Res::ALL.len() => {
            if p.choose {
                return Some(Action::Choose { token: Token::General });
            }
            if p.ready {
                return None;
            }
            let res = Res::ALL[step as usize];
            let target = nearest(res).unwrap_or(Target::Empire);
            Some(Action::Place { piece: Piece::Cube(res), target })
        }
        Phase::Production { .. } => {
            if p.ready {
                return None;
            }
            let mut order: Vec<&Building> = p.buildings.iter().collect();
            order.sort_by_key(|b| b.remaining().total());
            for b in order {
                let left = b.remaining();
                let target = Target::Card(b.card);
                if p.generals > 0 && left.generals > 0 {
                    return Some(Action::Place { piece: Piece::General, target });
                }
                if p.financiers > 0 && left.financiers > 0 {
                    return Some(Action::Place { piece: Piece::Financier, target });
                }
                if p.krystallium > 0 {
                    if let Some(res) = Res::ALL.iter().find(|r| left.res[r.index()] > 0) {
                        return Some(Action::Place { piece: Piece::Krystallium(*res), target });
                    }
                }
            }
            Some(Action::Ready)
        }
        Phase::Over => None,
    }
}

fn play_out(n: usize, seed: u64) -> State {
    play_with(n, seed, bot)
}

fn play_smart(n: usize, seed: u64) -> State {
    play_with(n, seed, smart_bot)
}

fn play_with(n: usize, seed: u64, bot: fn(&State, usize, usize) -> Option<Action>) -> State {
    let mut s = State::new(n, seed);
    let mut turn = seed as usize;
    for _ in 0..50_000 {
        if s.phase == Phase::Over {
            break;
        }
        let mut moved = false;
        for seat in 0..n {
            if let Some(action) = bot(&s, seat, turn) {
                let phase = s.phase;
                s.apply(seat, action.clone())
                    .unwrap_or_else(|e| panic!("seat {seat} {action:?} in {phase:?}: {e:?}"));
                moved = true;
                turn += 1;
                check_cards(&s);
                assert!(s.log.len() <= 24);
            }
        }
        assert!(moved, "deadlock in {:?}, round {}", s.phase, s.round);
    }
    assert_eq!(s.phase, Phase::Over, "the game never ended");
    s
}

#[test]
fn whole_games_end_for_every_player_count() {
    for n in MIN_PLAYERS..=MAX_PLAYERS {
        for seed in 0..6 {
            for s in [play_out(n, seed), play_smart(n, seed)] {
                assert_eq!((s.round, s.scores.len()), (ROUNDS, n));
                assert!(!s.winners().is_empty());
                for p in &s.players {
                    assert!(p.hand.is_empty() && p.drafted.is_empty() && p.picked.is_none());
                    assert_eq!(p.pending, [0; 5]);
                }
                // Four rounds of drafting used this much of the deck.
                let dealt = ROUNDS as usize * n * hand_size(n);
                assert_eq!(s.deck.len(), DECK_SIZE - dealt);
                // Every score is the sum of its parts.
                for (i, score) in s.scores.iter().enumerate() {
                    assert_eq!(*score, s.score_of(i));
                    assert_eq!(score.total, score.gross + score.combo + score.generals + score.financiers);
                    assert_eq!(score.cards as usize, s.players[i].empire.len());
                }
            }
        }
    }
}

#[test]
fn games_are_reproducible_from_the_seed() {
    assert_eq!(play_out(4, 3).scores, play_out(4, 3).scores);
}

#[test]
fn a_sensible_player_builds_a_good_number_of_cards() {
    for n in MIN_PLAYERS..=MAX_PLAYERS {
        let s = play_smart(n, 1);
        for (i, p) in s.players.iter().enumerate() {
            assert!(p.empire.len() >= 3, "seat {i} of {n} finished only {}", p.empire.len());
        }
        assert!(s.scores.iter().all(|sc| sc.total >= 4), "{:?}", s.scores);
    }
}

#[test]
fn nothing_works_once_the_game_is_over() {
    let mut s = play_out(2, 1);
    let card = CardId(0);
    for action in [
        Action::Ready,
        Action::Draft { card },
        Action::Build { card },
        Action::Choose { token: Token::General },
        Action::Place { piece: Piece::General, target: Target::Empire },
    ] {
        assert_eq!(s.apply(0, action), Err(Error::GameOver));
    }
}

#[test]
fn the_log_keeps_only_the_latest_events() {
    let s = play_smart(3, 2);
    assert_eq!(s.log.len(), 24);
    assert_ne!(s.log.first(), Some(&Event::RoundStarted(1)), "the oldest events were dropped");
}

// ----- views -----------------------------------------------------------------

#[test]
fn a_view_hides_what_other_players_hold() {
    let mut s = State::new(3, 8);
    let secret_pick = s.players[0].hand[0];
    s.apply(0, Action::Draft { card: secret_pick }).unwrap();

    // The owner sees their pick and their hand...
    let mine = s.view_for(0);
    assert_eq!(mine.picked, Some(secret_pick));
    assert_eq!(mine.hand, s.players[0].hand);

    // ...the others only see that a pick was made.
    let theirs = s.view_for(1);
    assert!(theirs.players[0].picked);
    assert!(!theirs.players[1].picked);
    assert_eq!(theirs.picked, None);
    assert_eq!(theirs.hand, s.players[1].hand, "your own hand, not seat 0's");
    assert_eq!(theirs.players[0].hand_size, 6);

    // In the JSON, the hidden parts of a player are counts and flags, never cards.
    let json = serde_json::to_value(&theirs).unwrap();
    let other = &json["players"][0];
    assert!(other["hand_size"].is_number());
    assert!(other["picked"].is_boolean());
    assert!(other["drafted"].is_number());
    assert!(other.get("hand").is_none());
}

#[test]
fn the_draft_area_stays_private_until_cards_are_built() {
    let mut s = to_planning(3, 8);
    let kept = s.players[0].drafted.clone();
    let seen = s.view_for(1);
    assert_eq!(seen.players[0].drafted, 7);
    assert!(seen.players[0].buildings.is_empty());
    assert_eq!(s.view_for(0).drafted, kept);
    // Once built, everybody sees the building and its progress.
    s.apply(0, Action::Build { card: kept[0] }).unwrap();
    let seen = s.view_for(2);
    assert_eq!(seen.players[0].drafted, 6);
    assert_eq!(seen.players[0].buildings.len(), 1);
    assert_eq!(seen.players[0].buildings[0].card, kept[0]);
}

#[test]
fn a_view_shows_the_table() {
    let mut s = blank(2);
    s.players[0].empire = vec![id("Quarry")];
    s.players[0].generals = 1;
    let v = s.view_for(1);
    assert_eq!((v.you, v.round, v.phase), (1, 1, Phase::Planning));
    assert!(v.passes_to_next);
    assert_eq!(v.players[0].production, [2, 1, 0, 1, 0]);
    assert_eq!(v.players[0].points, 2);
    assert_eq!(v.players[0].generals, 1);
    assert_eq!(v.deck, s.deck.len());
    assert!(v.scores.is_empty() && v.winners.is_empty());
}

#[test]
fn scores_and_winners_appear_when_the_game_is_over() {
    let s = play_out(3, 4);
    let v = s.view_for(2);
    assert_eq!(v.phase, Phase::Over);
    assert_eq!(v.scores, s.scores);
    assert_eq!(v.winners, s.winners());
}

#[test]
fn views_survive_json_in_every_phase() {
    let mut s = State::new(3, 6);
    let check = |s: &State| {
        for seat in 0..s.seats() {
            let view = s.view_for(seat);
            let json = serde_json::to_string(&view).unwrap();
            let back: View = serde_json::from_str(&json).unwrap();
            assert_eq!(back, view, "{json}");
        }
    };
    check(&s);
    for _ in 0..DRAFT_PICKS {
        for seat in 0..3 {
            let card = s.players[seat].hand[0];
            s.apply(seat, Action::Draft { card }).unwrap();
        }
    }
    check(&s);
    for seat in 0..3 {
        for card in s.players[seat].drafted.clone() {
            s.apply(seat, Action::Build { card }).unwrap();
        }
        s.apply(seat, Action::Ready).unwrap();
    }
    check(&s);
    check(&play_out(3, 6));
}

#[test]
fn actions_survive_json() {
    let actions = vec![
        Action::Draft { card: CardId(3) },
        Action::Build { card: CardId(4) },
        Action::Recycle { card: CardId(5), to: Target::Empire },
        Action::Scrap { card: CardId(6), to: Target::Card(CardId(7)) },
        Action::Place { piece: Piece::Krystallium(Res::Gold), target: Target::Card(CardId(8)) },
        Action::Place { piece: Piece::Cube(Res::Science), target: Target::Empire },
        Action::Place { piece: Piece::General, target: Target::Card(CardId(9)) },
        Action::Choose { token: Token::Financier },
        Action::Ready,
    ];
    for action in actions {
        let json = serde_json::to_string(&action).unwrap();
        assert_eq!(serde_json::from_str::<Action>(&json).unwrap(), action, "{json}");
    }
}

#[test]
fn every_error_has_a_message() {
    for e in [
        Error::GameOver,
        Error::UnknownSeat,
        Error::WrongPhase,
        Error::AlreadyPicked,
        Error::NotInHand,
        Error::NotDrafted,
        Error::NotUnderConstruction,
        Error::InvalidTarget,
        Error::NothingToPlace,
        Error::CardsLeftToPlan,
        Error::MustChoose,
        Error::NothingToChoose,
    ] {
        assert!(e.message().ends_with('.'), "{e:?}");
    }
}

/// A tuning aid for whoever swaps in other cards: how many cards and points
/// the two bots end with. Run it with
/// `cargo test -p wonderful-core economy_report -- --ignored --nocapture`.
#[test]
#[ignore]
fn economy_report() {
    for n in 2..=5 {
        for (name, play) in [("dumb", play_out as fn(usize, u64) -> State), ("smart", play_smart)] {
            let (mut cards, mut scores) = (vec![], vec![]);
            for seed in 0..20 {
                let s = play(n, seed);
                for (i, p) in s.players.iter().enumerate() {
                    cards.push(p.empire.len());
                    scores.push(s.scores[i].total as usize);
                }
            }
            let avg = |v: &Vec<usize>| v.iter().sum::<usize>() as f64 / v.len() as f64;
            println!(
                "n={n} {name:5} cards avg {:.1} min {} max {} | score avg {:.1} min {} max {}",
                avg(&cards), cards.iter().min().unwrap(), cards.iter().max().unwrap(),
                avg(&scores), scores.iter().min().unwrap(), scores.iter().max().unwrap()
            );
        }
    }
}

