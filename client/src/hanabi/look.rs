//! What the Hanabi board looks like, as plain functions: colours, clue
//! buttons, the rings around cards, the move descriptions. Nothing here
//! touches the DOM, so all of it is unit tested natively.
//!
//! Ported unchanged from `crates/frontend/src/game_board.rs` of the Hanabi
//! project (McM1k/hanabii); only the crate name and visibility differ.

use hanabi_core::{Card, Clue, Color, GameRules, LastMove, PlayerId, VisibleCard};


pub fn color_class(c: Color) -> &'static str {
    match c {
        Color::White => "white",
        Color::Red => "red",
        Color::Yellow => "yellow",
        Color::Green => "green",
        Color::Blue => "blue",
        Color::Multicolor => "multicolor",
        Color::Black => "black",
        Color::Orange => "orange",
        Color::Purple => "purple",
    }
}

/// A single-letter abbreviation for the "ruled out" marks on own-hand
/// cards. Only ever called with a base or plain-optional color in
/// practice — a color clue can never name Multicolor or Black directly, so
/// neither can ever end up in a card's `not_colors` set — but the match
/// stays exhaustive.
pub fn color_initial(c: Color) -> &'static str {
    match c {
        Color::White => "W",
        Color::Red => "R",
        Color::Yellow => "Y",
        Color::Green => "G",
        Color::Blue => "B",
        Color::Multicolor => "M",
        Color::Black => "K",
        Color::Orange => "O",
        Color::Purple => "P",
    }
}

pub fn pips(current: u8, max: u8) -> String {
    let filled = "\u{25cf}".repeat(current as usize);
    let empty = "\u{25cb}".repeat((max - current) as usize);
    format!("{filled}{empty}")
}

/// The distinct colors and numbers worth offering as clues for a hand.
///
/// Ordinarily that's the ones that would actually touch something — the
/// only clues the engine wouldn't reject as touching zero cards. The colors
/// come from `GameRules::cluable_colors` (every active color but Multicolor
/// and Black), each kept only if `GameRules::color_clue_touches` says it
/// would touch at least one card here. That one definition covers the special
/// cases: a multicolor card counts as *every* color when receiving a clue, so
/// a hand holding one makes every cluable color valid; black is touched by
/// nothing, so it never makes a color valid.
///
/// The exception is hanabii mode (`GameRules::allows_empty_color_clues`):
/// its three primary colors can *always* be given, whatever the hand holds,
/// because a clue that touches nothing still rules that color out. Number
/// clues are only offered for numbers actually present, in every mode.
pub fn valid_clues(cards: &[VisibleCard], rules: &GameRules) -> (Vec<Color>, Vec<u8>) {
    let visible: Vec<Card> = cards.iter().filter_map(|c| c.card).collect();

    let mut colors: Vec<Color> = rules
        .cluable_colors()
        .into_iter()
        .filter(|&clue| {
            rules.allows_empty_color_clues()
                || visible
                    .iter()
                    .any(|card| rules.color_clue_touches(clue, card.color))
        })
        .collect();
    colors.sort();
    colors.dedup();

    let mut numbers: Vec<u8> = visible.iter().map(|card| card.number).collect();
    numbers.sort();
    numbers.dedup();
    (colors, numbers)
}

/// Joins names as "a", "a and b" or "a, b and c".
pub fn join_with_and(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [only] => only.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

/// What a color clue would touch, worded for a button tooltip — only worth
/// showing where it isn't obvious from the button itself, i.e. in hanabii
/// mode, where a red clue also touches orange and purple cards. Empty
/// (which browsers show no tooltip for) in an ordinary game.
pub fn clue_touch_tooltip(rules: &GameRules, clue: Color) -> String {
    if !rules.hanabii {
        return String::new();
    }
    let names: Vec<String> = rules
        .active_colors()
        .into_iter()
        .filter(|&color| rules.color_clue_touches(clue, color))
        .map(|color| format!("{color:?}").to_lowercase())
        .collect();
    format!("Touches {} cards", join_with_and(&names))
}

/// In hanabii mode a color clue also touches the mixed colors it's part of, so
/// its button is painted to show them: mostly its own color in the middle,
/// blending into the two neighbours on the color wheel at the sides — a red
/// button is mostly red with a little purple on one side and orange on the
/// other, because a red clue touches red, orange *and* purple cards. (The
/// wheel is just the game's color order — red, orange, yellow, green, blue,
/// purple, around and around — which puts each primary between exactly the
/// two mixed colors it's an ingredient of.) Empty in an ordinary game, where
/// the button keeps its plain color, and for anything that isn't a color on
/// the wheel.
pub fn clue_button_style(rules: &GameRules, clue: Color) -> String {
    if !rules.hanabii {
        return String::new();
    }
    let wheel = rules.active_colors();
    let Some(at) = wheel.iter().position(|&c| c == clue) else {
        return String::new();
    };
    let paint = |color: Color| format!("var(--{}-fw)", color_class(color));
    // Only blend in a neighbour the clue really does touch; otherwise that
    // side just stays the clue's own color.
    let side = |neighbour: Color| {
        if rules.color_clue_touches(clue, neighbour) {
            paint(neighbour)
        } else {
            paint(clue)
        }
    };
    let n = wheel.len();
    format!(
        "background: linear-gradient(90deg, {} 0%, {} 26%, {} 74%, {} 100%)",
        side(wheel[(at + n - 1) % n]),
        paint(clue),
        paint(clue),
        side(wheel[(at + 1) % n]),
    )
}

/// What hanabii mode draws around a card while its color is still uncertain
/// to its owner: a spinning ring made of every color the card could still
/// be — that's the whole of what the clues have told them, so no separate
/// marks are needed. Everyone sees it: on your own cards it's what you know,
/// and on everyone else's it's what *they* know, which is what tells you
/// what's worth clueing. A red clue that touches a card leaves red, orange and
/// purple (three equal arcs); a yellow miss on top of that leaves red and
/// purple (two). Clues that *miss* a card narrow it down just as much (a red
/// miss leaves yellow, green and blue), so touched and missed cards get the
/// same ring.
///
/// This returns the colors on the ring, in the game's display order — `None`
/// when there's no ring: nothing has narrowed the card down yet, or the
/// clues leave a single color, in which case the whole card face fills in
/// with that color instead (see the `card-<color>` classes) — and outside
/// hanabii mode.
pub fn hanabii_ring_colors(
    knowledge: &hanabi_core::CardKnowledge,
    rules: &GameRules,
) -> Option<Vec<Color>> {
    if !rules.hanabii {
        return None;
    }
    let possible = knowledge.hanabii_possible_colors(rules);
    if possible.len() <= 1 || possible.len() == rules.active_colors().len() {
        return None;
    }
    Some(possible)
}

/// The `--ring-stops` custom property a card's ring gradient is built from
/// (see `.card-ring` in style.css): one hard-edged arc per color, all the
/// same size.
pub fn ring_stops_style(colors: &[Color]) -> String {
    let n = colors.len() as f64;
    let stops: Vec<String> = colors
        .iter()
        .enumerate()
        .map(|(i, &color)| {
            format!(
                "var(--{}-fw) {:.2}% {:.2}%",
                color_class(color),
                100.0 * i as f64 / n,
                100.0 * (i + 1) as f64 / n,
            )
        })
        .collect();
    format!("--ring-stops: {}", stops.join(", "))
}

/// The `(class, style, label)` a clue button for `clue` renders with —
/// factored out of the real button so `clue_image` (the small inert replica
/// shown next to a hand that just received a clue) can reuse the exact same
/// look without duplicating the logic, and so the look itself is unit
/// testable without a DOM. For a color this is `clue_button_style`'s
/// gradient (or nothing, outside hanabii mode, same as the real button);
/// for a number there's no inline style at all — same as the real button,
/// which relies on the plain `button { background: var(--ember); }` rule.
pub fn clue_image_parts(clue: Clue, rules: &GameRules) -> (String, String, String) {
    match clue {
        Clue::Color(color) => (
            format!("clue-btn clue-image card-{}", color_class(color)),
            clue_button_style(rules, color),
            format!("{color:?}"),
        ),
        Clue::Number(n) => ("clue-btn clue-image".to_string(), String::new(), n.to_string()),
    }
}

/// All seated players, starting from whoever's turn it is right now and
/// wrapping around in normal turn order. This is what lets the hand list
/// show "who plays when" just by reading top to bottom.
pub fn turn_order(all_ids: &[PlayerId], current_turn: PlayerId) -> Vec<PlayerId> {
    let start = all_ids.iter().position(|&id| id == current_turn).unwrap_or(0);
    let mut ordered = Vec::with_capacity(all_ids.len());
    ordered.extend_from_slice(&all_ids[start..]);
    ordered.extend_from_slice(&all_ids[..start]);
    ordered
}

pub fn describe_move(mv: &LastMove, name_of: &dyn Fn(PlayerId) -> String) -> String {
    match mv {
        LastMove::Clue {
            target,
            clue,
            touched,
        } => {
            let about = match clue {
                Clue::Color(c) => format!("{c:?}"),
                Clue::Number(n) => n.to_string(),
            };
            let cards = match touched.len() {
                0 => "no cards".to_string(),
                1 => "1 card".to_string(),
                n => format!("{n} cards"),
            };
            format!("Clued {} about {about} ({cards})", name_of(*target))
        }
        LastMove::Play { card, success } => {
            let verb = if *success { "Played" } else { "Misplayed" };
            format!("{verb} {:?} {}", card.color, card.number)
        }
        LastMove::Discard { card } => {
            format!("Discarded {:?} {}", card.color, card.number)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hanabi_core::{CardId, CardKnowledge};

    fn hand(cards: &[(Color, u8)]) -> Vec<VisibleCard> {
        cards
            .iter()
            .enumerate()
            .map(|(i, &(color, number))| VisibleCard {
                id: CardId(i as u32),
                card: Some(Card { color, number }),
                knowledge: CardKnowledge::default(),
            })
            .collect()
    }

    fn hanabii() -> GameRules {
        GameRules { hanabii: true, ..Default::default() }.normalized()
    }

    #[test]
    fn hanabii_always_offers_all_three_primaries_whatever_the_hand_holds() {
        // Two orange cards touch red and yellow but not blue — and blue is
        // offered anyway, since a clue that touches nothing still rules blue
        // out.
        let (colors, numbers) = valid_clues(&hand(&[(Color::Orange, 1), (Color::Orange, 2)]), &hanabii());
        assert_eq!(colors, vec![Color::Red, Color::Yellow, Color::Blue]);
        // Numbers are still only offered where they'd touch a card.
        assert_eq!(numbers, vec![1, 2]);
        for color in [Color::Red, Color::Orange, Color::Yellow, Color::Green, Color::Blue, Color::Purple] {
            assert_eq!(
                valid_clues(&hand(&[(color, 3)]), &hanabii()).0,
                vec![Color::Red, Color::Yellow, Color::Blue],
                "a hand of {color:?}"
            );
        }
    }

    #[test]
    fn hanabii_never_offers_a_secondary_color_as_a_clue() {
        let cards = hand(&[(Color::Green, 1), (Color::Purple, 2), (Color::Orange, 3)]);
        let (colors, _) = valid_clues(&cards, &hanabii());
        assert_eq!(colors, vec![Color::Red, Color::Yellow, Color::Blue]);
    }

    #[test]
    fn clue_buttons_blend_in_the_mixed_colors_their_clue_touches() {
        let rules = hanabii();
        // Mostly the clue's own color in the middle, its two colour-wheel
        // neighbours (the mixed colors it's an ingredient of) at the sides.
        assert_eq!(
            clue_button_style(&rules, Color::Red),
            "background: linear-gradient(90deg, var(--purple-fw) 0%, var(--red-fw) 26%, var(--red-fw) 74%, var(--orange-fw) 100%)"
        );
        assert_eq!(
            clue_button_style(&rules, Color::Yellow),
            "background: linear-gradient(90deg, var(--orange-fw) 0%, var(--yellow-fw) 26%, var(--yellow-fw) 74%, var(--green-fw) 100%)"
        );
        assert_eq!(
            clue_button_style(&rules, Color::Blue),
            "background: linear-gradient(90deg, var(--green-fw) 0%, var(--blue-fw) 26%, var(--blue-fw) 74%, var(--purple-fw) 100%)"
        );
    }

    #[test]
    fn every_color_a_hanabii_clue_touches_shows_up_on_its_button_and_nothing_else() {
        let rules = hanabii();
        for primary in Color::PRIMARIES {
            let style = clue_button_style(&rules, primary);
            for color in rules.active_colors() {
                let painted = style.contains(&format!("var(--{}-fw)", color_class(color)));
                assert_eq!(
                    painted,
                    rules.color_clue_touches(primary, color),
                    "{primary:?} button, {color:?}"
                );
            }
        }
    }

    #[test]
    fn clue_buttons_stay_plain_outside_hanabii_mode() {
        for rules in [
            GameRules::default(),
            GameRules { multicolor: true, black: true, extra_colors: 2, ..Default::default() },
        ] {
            for color in [Color::Red, Color::Yellow, Color::Blue] {
                assert_eq!(clue_button_style(&rules, color), "");
            }
        }
    }

    #[test]
    fn a_last_move_line_says_how_many_cards_a_clue_touched() {
        let name = |_: PlayerId| "Bob".to_string();
        let clue = |touched: Vec<CardId>| LastMove::Clue {
            target: PlayerId(1),
            clue: Clue::Color(Color::Red),
            touched,
        };
        assert_eq!(describe_move(&clue(vec![]), &name), "Clued Bob about Red (no cards)");
        assert_eq!(describe_move(&clue(vec![CardId(4)]), &name), "Clued Bob about Red (1 card)");
        assert_eq!(
            describe_move(&clue(vec![CardId(4), CardId(6), CardId(7)]), &name),
            "Clued Bob about Red (3 cards)"
        );
    }

    #[test]
    fn ordinary_games_offer_the_colors_present_in_the_hand() {
        let cards = hand(&[(Color::Red, 1), (Color::Blue, 2), (Color::Blue, 3)]);
        let (colors, numbers) = valid_clues(&cards, &GameRules::default());
        assert_eq!(colors, vec![Color::Red, Color::Blue]);
        assert_eq!(numbers, vec![1, 2, 3]);
    }

    #[test]
    fn a_multicolor_card_makes_every_cluable_color_valid() {
        let rules = GameRules { multicolor: true, extra_colors: 1, ..Default::default() };
        let (colors, _) = valid_clues(&hand(&[(Color::Multicolor, 3)]), &rules);
        // Everything in play except Multicolor itself (never named in a
        // clue) — and no White, which `extra_colors: 1` drops.
        assert_eq!(
            colors,
            vec![Color::Red, Color::Yellow, Color::Green, Color::Blue, Color::Orange, Color::Purple]
        );
    }

    #[test]
    fn a_black_card_never_makes_a_color_clue_valid() {
        let rules = GameRules { black: true, ..Default::default() };
        let (colors, numbers) = valid_clues(&hand(&[(Color::Black, 5)]), &rules);
        assert!(colors.is_empty());
        assert_eq!(numbers, vec![5]);
    }

    fn knowledge_after(results: &[(Color, bool)]) -> hanabi_core::CardKnowledge {
        let rules = hanabii();
        let mut k = CardKnowledge::default();
        for &(primary, touched) in results {
            k.apply_clue_result(Clue::Color(primary), touched, &rules);
        }
        k
    }

    fn ring(colors: &[Color]) -> Option<Vec<Color>> {
        Some(colors.to_vec())
    }

    #[test]
    fn a_touched_card_gets_a_ring_of_every_color_it_could_be() {
        let rules = hanabii();
        // Red touched it: red, orange or purple.
        let k = knowledge_after(&[(Color::Red, true)]);
        assert_eq!(
            hanabii_ring_colors(&k, &rules),
            ring(&[Color::Red, Color::Orange, Color::Purple])
        );
        // ...and a yellow miss on top of that leaves red or purple.
        let k = knowledge_after(&[(Color::Red, true), (Color::Yellow, false)]);
        assert_eq!(hanabii_ring_colors(&k, &rules), ring(&[Color::Red, Color::Purple]));
        // The other two primaries work the same way.
        let k = knowledge_after(&[(Color::Yellow, true)]);
        assert_eq!(
            hanabii_ring_colors(&k, &rules),
            ring(&[Color::Orange, Color::Yellow, Color::Green])
        );
        let k = knowledge_after(&[(Color::Blue, true)]);
        assert_eq!(
            hanabii_ring_colors(&k, &rules),
            ring(&[Color::Green, Color::Blue, Color::Purple])
        );
    }

    #[test]
    fn a_card_that_was_only_missed_gets_a_ring_too() {
        let rules = hanabii();
        // A red miss rules out red, orange and purple: yellow, green or blue.
        let k = knowledge_after(&[(Color::Red, false)]);
        assert_eq!(
            hanabii_ring_colors(&k, &rules),
            ring(&[Color::Yellow, Color::Green, Color::Blue])
        );
        let k = knowledge_after(&[(Color::Blue, false)]);
        assert_eq!(
            hanabii_ring_colors(&k, &rules),
            ring(&[Color::Red, Color::Orange, Color::Yellow])
        );
    }

    #[test]
    fn a_card_no_color_clue_has_narrowed_down_has_no_ring() {
        let rules = hanabii();
        assert_eq!(hanabii_ring_colors(&CardKnowledge::default(), &rules), None);
        // A number clue says nothing about color.
        let mut k = CardKnowledge::default();
        k.apply_clue_result(Clue::Number(3), true, &rules);
        assert_eq!(hanabii_ring_colors(&k, &rules), None);
    }

    #[test]
    fn a_card_whose_color_is_certain_has_no_ring_because_the_whole_card_fills_in() {
        let rules = hanabii();
        // Red + yellow hit: orange.
        let k = knowledge_after(&[(Color::Red, true), (Color::Yellow, true)]);
        assert_eq!(hanabii_ring_colors(&k, &rules), None);
        // Red hit, yellow and blue missed: plain red.
        let k = knowledge_after(&[(Color::Red, true), (Color::Yellow, false), (Color::Blue, false)]);
        assert_eq!(hanabii_ring_colors(&k, &rules), None);
        // Red and yellow both missed: blue, without ever being touched.
        let k = knowledge_after(&[(Color::Red, false), (Color::Yellow, false)]);
        assert_eq!(hanabii_ring_colors(&k, &rules), None);
    }

    #[test]
    fn there_is_never_a_ring_outside_hanabii_mode() {
        let k = knowledge_after(&[(Color::Red, true)]);
        assert_eq!(hanabii_ring_colors(&k, &GameRules::default()), None);
    }

    #[test]
    fn every_reachable_ring_has_two_or_three_colors_including_the_real_one() {
        // Over every card color and every subset of the three primary
        // clues (with the results they'd really have): whenever a ring is
        // drawn it lists the card's true color and has two or three arcs.
        let rules = hanabii();
        for color in rules.active_colors() {
            for subset in 0u8..8 {
                let mut results = Vec::new();
                for (i, primary) in Color::PRIMARIES.into_iter().enumerate() {
                    if subset & (1 << i) != 0 {
                        results.push((primary, rules.color_clue_touches(primary, color)));
                    }
                }
                let k = knowledge_after(&results);
                if let Some(colors) = hanabii_ring_colors(&k, &rules) {
                    assert!(colors.contains(&color), "{color:?} {results:?}");
                    assert!((2..=3).contains(&colors.len()), "{color:?} {results:?}");
                }
            }
        }
    }

    #[test]
    fn the_ring_is_drawn_as_equal_hard_edged_arcs_one_per_color() {
        assert_eq!(
            ring_stops_style(&[Color::Red, Color::Orange, Color::Purple]),
            "--ring-stops: var(--red-fw) 0.00% 33.33%, var(--orange-fw) 33.33% 66.67%, var(--purple-fw) 66.67% 100.00%"
        );
        assert_eq!(
            ring_stops_style(&[Color::Red, Color::Purple]),
            "--ring-stops: var(--red-fw) 0.00% 50.00%, var(--purple-fw) 50.00% 100.00%"
        );
    }

    #[test]
    fn clue_tooltips_name_everything_a_hanabii_clue_touches() {
        let rules = hanabii();
        assert_eq!(clue_touch_tooltip(&rules, Color::Red), "Touches red, orange and purple cards");
        assert_eq!(clue_touch_tooltip(&rules, Color::Yellow), "Touches orange, yellow and green cards");
        assert_eq!(clue_touch_tooltip(&rules, Color::Blue), "Touches green, blue and purple cards");
    }

    #[test]
    fn clue_tooltips_are_left_off_in_ordinary_games() {
        assert_eq!(clue_touch_tooltip(&GameRules::default(), Color::Red), "");
    }

    #[test]
    fn join_with_and_reads_naturally() {
        let names = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(join_with_and(&names(&[])), "");
        assert_eq!(join_with_and(&names(&["red"])), "red");
        assert_eq!(join_with_and(&names(&["red", "blue"])), "red and blue");
        assert_eq!(join_with_and(&names(&["red", "orange", "purple"])), "red, orange and purple");
    }

    #[test]
    fn a_hanabii_color_badge_matches_the_real_buttons_class_and_gradient() {
        let rules = hanabii();
        for color in [Color::Red, Color::Yellow, Color::Blue] {
            let (class, style, label) = clue_image_parts(Clue::Color(color), &rules);
            // Same class the real button gets, plus the inert marker, and
            // the exact same gradient string the button's own style uses.
            assert_eq!(class, format!("clue-btn clue-image card-{}", color_class(color)));
            assert_eq!(style, clue_button_style(&rules, color));
            assert!(!style.is_empty(), "hanabii buttons always carry a gradient");
            assert_eq!(label, format!("{color:?}"));
        }
    }

    #[test]
    fn an_ordinary_color_badge_has_no_inline_style_same_as_its_button() {
        let rules = GameRules::default();
        let (class, style, label) = clue_image_parts(Clue::Color(Color::Blue), &rules);
        assert_eq!(class, "clue-btn clue-image card-blue");
        // Outside hanabii mode the real button has no inline style either —
        // it's colored entirely by the plain `.card-blue` class rule.
        assert_eq!(style, clue_button_style(&rules, Color::Blue));
        assert_eq!(style, "");
        assert_eq!(label, "Blue");
    }

    #[test]
    fn a_number_badge_has_the_plain_clue_btn_class_no_style_and_the_digit() {
        for rules in [GameRules::default(), hanabii()] {
            let (class, style, label) = clue_image_parts(Clue::Number(4), &rules);
            // No color class — relies on the same plain `button` background
            // the real number clue button does.
            assert_eq!(class, "clue-btn clue-image");
            assert_eq!(style, "");
            assert_eq!(label, "4");
        }
    }

    #[test]
    fn every_primary_and_every_number_round_trips_through_clue_image_parts() {
        // A broad sweep, rather than hand-picked cases: whatever the real
        // clue button would look like, the badge's (class, style) always
        // matches it exactly.
        let rules = hanabii();
        for color in Color::PRIMARIES {
            let (_, style, _) = clue_image_parts(Clue::Color(color), &rules);
            assert_eq!(style, clue_button_style(&rules, color));
        }
        for n in 1..=6u8 {
            let (class, style, label) = clue_image_parts(Clue::Number(n), &rules);
            assert_eq!(class, "clue-btn clue-image");
            assert_eq!(style, "");
            assert_eq!(label, n.to_string());
        }
    }
}
