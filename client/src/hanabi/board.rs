//! The Hanabi board: fireworks, discard pile, the play/discard drop zones and
//! the turn-ordered list of hands with their clue buttons.
//!
//! Ported from `game_board.rs` of the stand-alone client (McM1k/hanabii,
//! Leptos 0.6). What changed: the state comes from the shared `App` (see
//! `crate::state`) instead of a context of its own, moves go out as
//! `ClientMsg::Hanabi`, and the bookkeeping that used `Rc<RefCell<..>>` now
//! rides on the effects' own return values or on `StoredValue`s, because
//! Leptos 0.8 wants every closure that ends up in a view to be `Send + Sync`.
//! The looks and the animations are the original's.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use hanabi_core::{
    next_expected_rank, points_for, Action, CardId, Clue, Color, EndReason, GameRules, GameStatus,
    LastMove, PlayerId, VisibleCard, MAX_CLUE_TOKENS, MAX_FUSE_TOKENS,
};
use leptos::html::Div;
use leptos::prelude::*;
use protocol::ClientMsg;

use super::look::{
    clue_button_style, clue_image_parts, clue_touch_tooltip, color_class, color_initial,
    describe_move, hanabii_ring_colors, pips, ring_stops_style, turn_order, valid_clues,
};
use crate::state::{leave, send, App};

/// Pulls the dragged card's id back out of a drop event. `dragstart` stores
/// it as plain text via `DataTransfer::set_data`; this is the other half.
fn dragged_card_id(ev: &web_sys::DragEvent) -> Option<CardId> {
    let data_transfer = ev.data_transfer()?;
    let id_str = data_transfer.get_data("text/plain").ok()?;
    id_str.parse::<u32>().ok().map(CardId)
}

/// A small, non-interactive copy of the clue button for `clue` — same
/// classes and inline style as the real, clickable one (so it's pixel-for-
/// pixel the same color/gradient), just inert: no listeners, out of the tab
/// order, and `.clue-image` sets `pointer-events: none` so it can never be
/// mistaken for something to click. A real `<button>` rather than a `<span>`
/// specifically so it picks up the exact same CSS (in particular the plain
/// `button { background: var(--ember); }` rule a number clue relies on) —
/// deliberately *not* `disabled`, since `button:disabled` is styled quite
/// differently and would defeat the point of an exact replica.
fn clue_image(clue: Clue, rules: &GameRules) -> impl IntoView {
    let (class, style, label) = clue_image_parts(clue, rules);
    view! {
        <button type="button" class=class style=style tabindex="-1">
            {label}
        </button>
    }
}

/// Endpoints for an 8-ray burst radiating from (20, 20) in a 40x40 viewBox,
/// precomputed (not done at runtime) at three different radii so each
/// firework's "reveal" can grow in both ray count and length as it fills in.
/// Order is E, SE, S, SW, W, NW, N, NE.
const RAYS_SHORT: [(&str, &str); 8] = [
    ("26", "20"),
    ("24.2", "24.2"),
    ("20", "26"),
    ("15.8", "24.2"),
    ("14", "20"),
    ("15.8", "15.8"),
    ("20", "14"),
    ("24.2", "15.8"),
];
const RAYS_MEDIUM: [(&str, &str); 8] = [
    ("32", "20"),
    ("28.5", "28.5"),
    ("20", "32"),
    ("11.5", "28.5"),
    ("8", "20"),
    ("11.5", "11.5"),
    ("20", "8"),
    ("28.5", "11.5"),
];
const RAYS_FULL: [(&str, &str); 8] = [
    ("35", "20"),
    ("30.6", "30.6"),
    ("20", "35"),
    ("9.4", "30.6"),
    ("5", "20"),
    ("9.4", "9.4"),
    ("20", "5"),
    ("30.6", "9.4"),
];

/// A denser, larger burst reserved for a suit that's actually *complete*
/// (every card of that colour has been played) rather than merely at the
/// same "full" stage `RAYS_FULL` renders at progress 5. The two used to be
/// indistinguishable, which looked fine for an ordinary 5-card suit (where
/// progress 5 always meant done) but reads as wrong for a `six_cards` suit
/// sitting at progress 5 with one play still to go. Twelve rays instead of
/// eight, reaching further out toward the icon's edge.
const RAYS_BRILLIANT: [(&str, &str); 12] = [
    ("37", "20"),
    ("34.7", "28.5"),
    ("28.5", "34.7"),
    ("20", "37"),
    ("11.5", "34.7"),
    ("5.3", "28.5"),
    ("3", "20"),
    ("5.3", "11.5"),
    ("11.5", "5.3"),
    ("20", "3"),
    ("28.5", "5.3"),
    ("34.7", "11.5"),
];

/// A small burst icon that fills in more as `progress` (0-5) increases —
/// an original take on the physical Hanabi cards, where laying out a suit's
/// cards in order reveals progressively more of a firework illustration.
/// `complete` overrides all of that once the suit is actually finished
/// (see `RAYS_BRILLIANT`), rendering a bigger, denser burst that also picks
/// up its own color via the `firework-burst--complete` class instead of
/// just inheriting the tile's ordinary dark/light icon color — the same
/// "something notable happened" accent already used for the drawn-card
/// highlight elsewhere in this app.
fn firework_burst(progress: u8, complete: bool) -> AnyView {
    if complete {
        let ray_lines = RAYS_BRILLIANT
            .iter()
            .map(|&(x2, y2)| {
                view! {
                    <line
                        x1="20"
                        y1="20"
                        x2=x2
                        y2=y2
                        stroke="currentColor"
                        stroke-width="2.2"
                        stroke-linecap="round"
                    />
                }
            })
            .collect_view();
        let tip_dots = RAYS_BRILLIANT
            .iter()
            .map(|&(x, y)| view! { <circle cx=x cy=y r="1.6" fill="currentColor" /> })
            .collect_view();

        return view! {
            <svg class="firework-burst firework-burst--complete" viewBox="0 0 40 40">
                {ray_lines}
                <circle cx="20" cy="20" r="6" fill="currentColor" />
                {tip_dots}
            </svg>
        }
        .into_any();
    }

    let (rays, dot_r): (&[(&str, &str)], &str) = match progress {
        0 => (&[], "2"),
        1 => (&RAYS_SHORT[0..1], "3"),
        2 => (&RAYS_SHORT[0..2], "3.5"),
        3 => (&RAYS_MEDIUM[0..4], "4"),
        4 => (&RAYS_MEDIUM[0..6], "4.5"),
        _ => (&RAYS_FULL[0..8], "5"),
    };

    let ray_lines = rays
        .iter()
        .map(|&(x2, y2)| {
            view! {
                <line
                    x1="20"
                    y1="20"
                    x2=x2
                    y2=y2
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                />
            }
        })
        .collect_view();

    let tip_dots = (progress >= 5).then(|| {
        RAYS_FULL
            .iter()
            .step_by(2)
            .map(|&(x, y)| view! { <circle cx=x cy=y r="1.5" fill="currentColor" /> })
            .collect_view()
    });

    view! {
        <svg class="firework-burst" viewBox="0 0 40 40">
            {ray_lines}
            <circle cx="20" cy="20" r=dot_r fill="currentColor" />
            {tip_dots}
        </svg>
    }
    .into_any()
}

/// How long the hand-swap slide takes. Kept in one place since it has to
/// match between the CSS `transition` we set from Rust and (loosely) how
/// long it feels right for a handful of DOM nodes gliding past each other.
const HAND_SLIDE_MS: u32 = 350;

/// How long the cards a clue just touched stay highlighted, in milliseconds.
const TOUCHED_FLASH_MS: u64 = 2400;

/// How long a freshly drawn card stays highlighted, in milliseconds.
const DRAW_FLASH_MS: u64 = 1500;

/// The purely visual state of the board: which hand's clue buttons are open,
/// what is being dragged or hovered, and what is flashing. Created once per
/// game next to the board, so it survives every state update.
#[derive(Clone, Copy)]
struct Ui {
    /// The other player whose clue buttons are open, if any.
    selected_target: RwSignal<Option<PlayerId>>,
    /// Whether a card is being dragged, so the drop zones can light up.
    is_dragging: RwSignal<bool>,
    /// The clue button under the pointer, so the cards it would touch can be
    /// outlined as a preview before it's actually given.
    hovered_clue: RwSignal<Option<Clue>>,
    /// The cards the latest clue touched, for a moment after it's given — on
    /// every screen, so the whole table sees what was pointed at.
    touched_flash: RwSignal<HashSet<CardId>>,
    /// The clue most recently given, and who received it: drives a small
    /// replica of the clue button (see `clue_image`) shown next to that
    /// player's hand. Set and cleared together with `touched_flash`, so the
    /// flash and the badge always appear and disappear in lockstep.
    last_clue_badge: RwSignal<Option<(PlayerId, Clue)>>,
}

impl Ui {
    fn new() -> Ui {
        Ui {
            selected_target: RwSignal::new(None),
            is_dragging: RwSignal::new(false),
            hovered_clue: RwSignal::new(None),
            touched_flash: RwSignal::new(HashSet::new()),
            last_clue_badge: RwSignal::new(None),
        }
    }
}

/// A seat's name, as the lobby knew it.
fn player_name(app: App, id: PlayerId) -> String {
    app.hanabi
        .names
        .with_untracked(|names| names.get(id.0 as usize).cloned())
        .unwrap_or_else(|| format!("Player {}", id.0 as usize + 1))
}

/// Whether it's our turn in a game that is still running.
fn can_act(app: App, you: PlayerId) -> bool {
    app.hanabi.view.with(|view| {
        view.as_ref()
            .is_some_and(|v| v.current_turn == you && v.status == GameStatus::InProgress)
    })
}

/// The whole in-progress board, built once per game (the screen is only
/// entered once the first state has arrived, and later states only change
/// the signals). That matters most for the hand list: it's rendered through
/// `<For>`, so each player keeps the *same* DOM node turn after turn, which
/// is what lets the slide animation move a hand to its new spot instead of
/// the whole list just popping into a new order.
pub fn board(app: App) -> impl IntoView {
    let hanabi = app.hanabi;
    let Some((you, all_ids)) = hanabi.view.with_untracked(|view| {
        view.as_ref().map(|v| {
            let mut ids: Vec<PlayerId> = v.hands.keys().copied().collect();
            ids.sort_by_key(|id| id.0);
            (v.you, ids)
        })
    }) else {
        return view! { <p class="hint">"Waiting for the game to start…"</p> }.into_any();
    };

    let ui = Ui::new();

    // A stable node ref per seat, created once — `<For>` re-uses (moves,
    // never recreates) the underlying `<div class="hand">` for a given key
    // as the turn order rotates, so these keep pointing at the same real
    // DOM element for the whole game.
    let hand_refs: HashMap<PlayerId, NodeRef<Div>> =
        all_ids.iter().map(|&pid| (pid, NodeRef::<Div>::new())).collect();

    // Slide the hands to their new places whenever the state changes, which
    // is when the order can have changed: every accepted action advances
    // whose turn it is.
    let last_tops = StoredValue::new(HashMap::<PlayerId, f64>::new());
    {
        let hand_refs = hand_refs.clone();
        Effect::new(move |_| {
            if hanabi.view.with(Option::is_none) {
                return;
            }
            // Measure once the list has been reordered, just before the next
            // paint.
            let hand_refs = hand_refs.clone();
            request_animation_frame(move || slide_hands(&hand_refs, last_tops));
        });
    }

    // Whoever was selected as a clue target only makes sense for the turn
    // during which they were picked — once the turn moves on (for any
    // reason: a clue given, a play, a discard), clear it so the next turn
    // starts without a stale target and its clue panel still showing.
    Effect::new(move |prev_turn: Option<PlayerId>| {
        match hanabi.view.with(|view| view.as_ref().map(|v| v.current_turn)) {
            Some(turn) => {
                if prev_turn.is_some_and(|prev| prev != turn) {
                    ui.selected_target.set(None);
                    // The clue button under the pointer is about to vanish
                    // without a `mouseleave`.
                    ui.hovered_clue.set(None);
                }
                turn
            }
            None => prev_turn.unwrap_or(you),
        }
    });

    watch_clues(app, ui);

    let hands = move |pid: PlayerId| {
        let node_ref = hand_refs.get(&pid).copied().unwrap_or_default();
        hand(app, ui, you, pid, node_ref)
    };

    view! {
        <div class="game-board">
            {room_bar(app)}
            <div>
                {table(app, ui, you)}

                <div class="panel">
                    <p class="hint">
                        "Top of the list plays next. Click another player's name to see clues you can give them."
                    </p>
                    <For
                        each=move || {
                            hanabi.view.with(|view| match view {
                                Some(v) => {
                                    let mut ids: Vec<PlayerId> = v.hands.keys().copied().collect();
                                    ids.sort_by_key(|id| id.0);
                                    turn_order(&ids, v.current_turn)
                                }
                                None => Vec::new(),
                            })
                        }
                        key=|pid: &PlayerId| *pid
                        children=hands
                    />
                </div>
            </div>
        </div>
    }
    .into_any()
}

/// The bar above the board: which room this is, and the way out.
fn room_bar(app: App) -> impl IntoView {
    view! {
        <div class="room-bar">
            <span>"Room " <b class="room-tag">{move || app.hanabi.room.get()}</b></span>
            <button class="link" on:click=move |_| leave(app)>
                "Leave game"
            </button>
        </div>
    }
}

/// FLIP, the "last" and "invert/play" steps: measure where every hand is now,
/// and for those that moved since the last measurement, jump them back to
/// where they visually were (transitions off) and let them glide to their
/// real position.
fn slide_hands(hand_refs: &HashMap<PlayerId, NodeRef<Div>>, last_tops: StoredValue<HashMap<PlayerId, f64>>) {
    let previous = last_tops.try_get_value().unwrap_or_default();
    let mut tops = HashMap::with_capacity(hand_refs.len());
    for (&pid, node_ref) in hand_refs {
        let Some(el) = node_ref.get_untracked() else { continue };
        let top = el.get_bounding_client_rect().top();
        tops.insert(pid, top);

        let Some(&old_top) = previous.get(&pid) else { continue };
        let delta = old_top - top;
        if delta.abs() > 1.0 {
            // (`HtmlElement::style` itself: the view builder's `.style(..)` would
            // otherwise take the name.)
            let html_el: &web_sys::HtmlElement = &el;
            let style = html_el.style();
            // Jump back to where it visually was, with transitions off so
            // this doesn't itself animate...
            let _ = style.set_property("transition", "none");
            let _ = style.set_property("transform", &format!("translateY({delta:.1}px)"));
            // ...force the browser to actually commit that frame before we
            // change anything else...
            let _ = el.get_bounding_client_rect();
            // ...then animate back to its real (natural, zero offset)
            // position.
            let _ = style.set_property("transition", &format!("transform {HAND_SLIDE_MS}ms ease"));
            let _ = style.set_property("transform", "translateY(0)");
        }
    }
    last_tops.try_set_value(tops);
}

/// Notices a clue being given and flashes what it pointed at, for a moment,
/// on every screen. A new action has just been played out exactly when whose
/// turn it is changed since the last state this browser saw; the very first
/// state (the initial deal, or joining/refreshing mid-game) is never "new".
fn watch_clues(app: App, ui: Ui) {
    // Bumped for every flash, so an older flash's timer can tell it has been
    // replaced and must not clear the newer one.
    let generation = StoredValue::new(0u32);

    Effect::new(move |previous: Option<Option<PlayerId>>| {
        let previous = previous.flatten();
        let Some((turn, clue)) = app.hanabi.view.with(|view| {
            view.as_ref().map(|v| {
                let clue = match v.last_actor.and_then(|actor| v.last_moves.get(&actor)) {
                    Some(LastMove::Clue { target, clue, touched }) => {
                        Some((*target, *clue, touched.iter().copied().collect::<HashSet<CardId>>()))
                    }
                    _ => None,
                };
                (v.current_turn, clue)
            })
        }) else {
            return previous;
        };
        if previous.is_none() || previous == Some(turn) {
            return Some(turn);
        }

        // Whatever was flashing belongs to a move that's over now.
        let mine = generation.try_update_value(|g| {
            *g = g.wrapping_add(1);
            *g
        });
        let Some(mine) = mine else { return Some(turn) };

        // Whether a clue was given at all, not whether it touched anything —
        // hanabii mode allows a clue that touches nothing (see
        // `GameRules::allows_empty_color_clues`), and "none of your cards
        // are red" is still real information the receiving player just got,
        // worth badging even with nothing to flash.
        let flashing = clue.is_some();
        let (badge, touched) = match clue {
            Some((target, clue, touched)) => (Some((target, clue)), touched),
            None => (None, HashSet::new()),
        };
        ui.touched_flash.set(touched);
        ui.last_clue_badge.set(badge);

        if flashing {
            set_timeout(
                move || {
                    // Only clear our own flash, not a newer one's.
                    if generation.try_get_value() == Some(mine) {
                        ui.touched_flash.try_set(HashSet::new());
                        ui.last_clue_badge.try_set(None);
                    }
                },
                Duration::from_millis(TOUCHED_FLASH_MS),
            );
        }
        Some(turn)
    });
}

/// The top of the board: the status line once the game is over, the
/// fireworks with the play drop zone, and the discard pile with its drop
/// zone. Rebuilt on every state update — it is cheap, and it holds nothing
/// that has to survive one except what it needs to tell what just changed.
fn table(app: App, ui: Ui, you: PlayerId) -> impl IntoView {
    // Remembers each firework's previous value and the discard pile's
    // previous size, purely to detect "did this just change" so the
    // relevant tile can briefly flash — the current value alone can't tell
    // "just happened" from "already true a while ago" without this. `None`
    // means nothing has been shown yet, so the first state (the deal, or
    // rejoining a game in progress) is never "new".
    let previous_fireworks = StoredValue::new(None::<HashMap<Color, u8>>);
    let previous_discard_count = StoredValue::new(None::<usize>);

    move || table_view(app, ui, you, previous_fireworks, previous_discard_count)
}

fn table_view(
    app: App,
    ui: Ui,
    you: PlayerId,
    previous_fireworks: StoredValue<Option<HashMap<Color, u8>>>,
    previous_discard_count: StoredValue<Option<usize>>,
) -> AnyView {
    let Some(view) = app.hanabi.view.get() else {
        return ().into_any();
    };

    let is_my_turn = view.current_turn == you;
    let can_act = is_my_turn && view.status == GameStatus::InProgress;
    let can_discard = can_act && view.clue_tokens < MAX_CLUE_TOKENS;
    let active_colors = view.rules.active_colors();

    // Which fireworks just gained a card, and whether the discard
    // pile just grew — compared against what was stored last render,
    // so this only fires on the render where it actually happened.
    let just_played: HashMap<Color, bool> = previous_fireworks
        .try_update_value(|prev| {
            let now: HashMap<Color, u8> = active_colors
                .iter()
                .map(|&color| (color, *view.fireworks.get(&color).unwrap_or(&0)))
                .collect();
            let changed = match prev.as_ref() {
                None => HashMap::new(),
                Some(before) => now
                    .iter()
                    .map(|(&color, &top)| (color, top > *before.get(&color).unwrap_or(&0)))
                    .collect(),
            };
            *prev = Some(now);
            changed
        })
        .unwrap_or_default();
    let just_discarded = previous_discard_count
        .try_update_value(|prev| {
            let len = view.discard_pile.len();
            let changed = prev.is_some_and(|before| len > before);
            *prev = Some(len);
            changed
        })
        .unwrap_or(false);

    let status_line = match view.status {
        GameStatus::InProgress => None,
        GameStatus::Finished(reason) => {
            let why = match reason {
                EndReason::FusesExhausted => "ran out of fuses",
                EndReason::DeckExhausted => "the deck ran out",
                EndReason::PerfectScore => "a perfect score",
            };
            let max_score = view.rules.max_score();
            Some(format!("Game over — {why}. Final score: {}/{max_score}", view.score))
        }
    };

    let fireworks_items = active_colors
        .iter()
        .map(|&color| {
            let top = *view.fireworks.get(&color).unwrap_or(&0);
            let label = if top == 0 { "—".to_string() } else { top.to_string() };
            // The burst illustration fills in based on how many cards
            // of this suit have actually been played — the same
            // "points this suit is worth" computation the score itself
            // uses, since a normal suit's progress is just its top
            // rank but a reverse suit (Black) counts down instead.
            let progress = points_for(color, top, view.rules.max_rank());
            // Distinct from "progress is at its highest displayed
            // stage" — with `six_cards` on, a suit can sit at progress
            // 5 for one more play before it's actually done.
            let complete = next_expected_rank(color, top, view.rules.max_rank()).is_none();
            let suit_label = if color == Color::Black {
                format!("{color:?} \u{2193}")
            } else {
                format!("{color:?}")
            };
            let short_badge = view.rules.is_short(color).then(|| {
                view! {
                    <span class="firework-short-badge" title="Short deck: only 1 of each card">
                        "1×"
                    </span>
                }
            });
            let mut tile_class = format!("firework firework-{}", color_class(color));
            if *just_played.get(&color).unwrap_or(&false) {
                tile_class.push_str(" firework-flash");
            }
            if complete {
                tile_class.push_str(" firework-complete");
            }
            view! {
                <div class=tile_class>
                    {short_badge}
                    <span class="firework-label">{suit_label}</span>
                    {firework_burst(progress, complete)}
                    <span class="firework-value">{label}</span>
                </div>
            }
        })
        .collect_view();

    let discard_groups = active_colors
        .iter()
        .filter_map(|&color| {
            let mut numbers: Vec<u8> = view
                .discard_pile
                .iter()
                .filter(|c| c.color == color)
                .map(|c| c.number)
                .collect();
            if numbers.is_empty() {
                return None;
            }
            numbers.sort_unstable();
            let chips = numbers
                .iter()
                .map(|n| {
                    view! {
                        <span class=format!("chip card-{}", color_class(color))>{n.to_string()}</span>
                    }
                })
                .collect_view();
            Some(view! {
                <div class="discard-row">
                    <span class="discard-color-label">{format!("{color:?}")}</span>
                    <span class="discard-chips">{chips}</span>
                </div>
            })
        })
        .collect_view();

    let discard_body = if view.discard_pile.is_empty() {
        view! { <p class="hint">"Nothing discarded yet."</p> }.into_any()
    } else {
        let class = if just_discarded {
            "discard-groups discard-flash"
        } else {
            "discard-groups"
        };
        view! { <div class=class>{discard_groups}</div> }.into_any()
    };

    view! {
        {status_line.map(|line| view! { <p class="status-line">{line}</p> })}

        <div
            class=move || drop_zone_class(can_act, ui.is_dragging.get())
            on:dragover=move |ev: web_sys::DragEvent| ev.prevent_default()
            on:drop=move |ev: web_sys::DragEvent| {
                ev.prevent_default();
                ui.is_dragging.set(false);
                if let Some(card_id) = dragged_card_id(&ev) {
                    send(app, &ClientMsg::Hanabi(Action::Play { card_id }));
                }
            }
        >
            <div class="fireworks">{fireworks_items}</div>
            <p class="tokens">
                "Clues " <span class="pip-row">{pips(view.clue_tokens, MAX_CLUE_TOKENS)}</span>
                "   Fuses " <span class="pip-row">{pips(view.fuse_tokens, MAX_FUSE_TOKENS)}</span>
                "   Deck: " {view.draw_pile_count}
            </p>
            <p class="hint">"Drag a card here to play it."</p>
        </div>

        <div
            class=move || drop_zone_class(can_discard, ui.is_dragging.get())
            on:dragover=move |ev: web_sys::DragEvent| ev.prevent_default()
            on:drop=move |ev: web_sys::DragEvent| {
                ev.prevent_default();
                ui.is_dragging.set(false);
                if let Some(card_id) = dragged_card_id(&ev) {
                    send(app, &ClientMsg::Hanabi(Action::Discard { card_id }));
                }
            }
        >
            <h3>"Discard pile"</h3>
            {discard_body}
            <p class="hint">"Drag a card here to discard it."</p>
        </div>
    }
    .into_any()
}

/// A drop zone is greyed out when its move isn't available, and lights up
/// while a card is being dragged and could be dropped on it.
fn drop_zone_class(available: bool, dragging: bool) -> &'static str {
    match (available, dragging) {
        (false, _) => "panel drop-zone disabled",
        (true, true) => "panel drop-zone drag-active",
        (true, false) => "panel drop-zone",
    }
}

/// One player's hand: their name, whose turn it is, what they did last, the
/// cards (their own seen through what the clues told them, everyone else's
/// face up with what that player knows about them) and, for another player,
/// the clue buttons once their name has been clicked.
fn hand(app: App, ui: Ui, you: PlayerId, pid: PlayerId, node_ref: NodeRef<Div>) -> AnyView {
    let hanabi = app.hanabi;
    let is_you = pid == you;
    let name_of = move |id: PlayerId| player_name(app, id);

    // Which of this hand's cards were drawn recently enough to still be
    // worth calling out. A genuine signal (not just a CSS animation fired at
    // render time) so the highlight is reliably visible for a fixed stretch
    // — added the instant a draw is detected, removed by its own timeout —
    // regardless of whether the underlying `<li>` for that card is a
    // freshly-created DOM node or one reused from a previous render.
    let recently_drawn = RwSignal::new(HashSet::<CardId>::new());

    // Dedicated to detecting draws and scheduling their highlight — kept
    // separate from `card_items` below (which only *reads* `recently_drawn`)
    // so nothing both reads and writes the same signal from within one
    // reactive scope. The effect's own value is the set of cards seen last
    // time; `None` means "nothing seen yet", so the very first run (the
    // initial deal) doesn't get flagged as a draw.
    Effect::new(move |seen: Option<Option<HashSet<CardId>>>| {
        let seen = seen.flatten();
        let Some(current) = hanabi.view.with(|view| {
            view.as_ref().map(|v| {
                v.hands
                    .get(&pid)
                    .map(|cards| cards.iter().map(|c| c.id).collect::<HashSet<CardId>>())
                    .unwrap_or_default()
            })
        }) else {
            return seen;
        };

        let newly_drawn: HashSet<CardId> = match &seen {
            Some(old) => current.difference(old).copied().collect(),
            None => HashSet::new(),
        };
        if !newly_drawn.is_empty() {
            recently_drawn.update(|set| set.extend(newly_drawn.iter().copied()));
            for id in newly_drawn {
                set_timeout(
                    move || {
                        recently_drawn.try_update(|set| {
                            set.remove(&id);
                        });
                    },
                    Duration::from_millis(DRAW_FLASH_MS),
                );
            }
        }
        Some(current)
    });

    let is_current = move || hanabi.view.with(|view| view.as_ref().is_some_and(|v| v.current_turn == pid));

    let hand_class = move || {
        let mut classes = String::from("hand");
        if is_current() {
            classes.push_str(" hand-current");
        }
        if !is_you && ui.selected_target.get() == Some(pid) {
            classes.push_str(" hand-selected");
        }
        classes
    };

    let now_playing = move || is_current().then(|| view! { <span class="now-playing">"Now playing"</span> });

    let last_move_line = move || {
        hanabi
            .view
            .with(|view| {
                view.as_ref()
                    .and_then(|v| v.last_moves.get(&pid))
                    .map(|mv| describe_move(mv, &name_of))
            })
            .map(|line| view! { <span class="last-move">{line}</span> })
    };

    // Whether the seat's connection is down: it keeps its place and cards,
    // and comes back with the same name.
    let offline_tag = move || {
        let online = hanabi
            .connected
            .with(|connected| connected.get(pid.0 as usize).copied().unwrap_or(true));
        (!online).then(|| view! { <span class="tag tag-off">"offline"</span> })
    };

    // A brief replica of the clue button this hand's owner was just given,
    // shown for as long as `last_clue_badge` holds it (see `watch_clues`) —
    // on every screen, not just the receiver's own, so whoever's about to
    // clue next can also see what this player was just told.
    let received_clue_badge = move || {
        let (target, clue) = ui.last_clue_badge.get()?;
        if target != pid {
            return None;
        }
        let rules = hanabi.view.with(|view| view.as_ref().map(|v| v.rules))?;
        Some(view! {
            <div class="received-clue" title="What this player was just told">
                {clue_image(clue, &rules)}
            </div>
        })
    };

    let card_items = move || {
        let Some((cards, rules)) = hanabi
            .view
            .with(|view| view.as_ref().map(|v| (v.hands.get(&pid).cloned().unwrap_or_default(), v.rules)))
        else {
            return ().into_any();
        };
        let can_act = can_act(app, you);
        let recently_drawn_now = recently_drawn.get();
        let touched_now = ui.touched_flash.get();

        if is_you {
            cards
                .iter()
                .map(|c| {
                    own_card(
                        c,
                        &rules,
                        can_act,
                        ui,
                        recently_drawn_now.contains(&c.id),
                        touched_now.contains(&c.id),
                    )
                })
                .collect_view()
                .into_any()
        } else {
            // Only preview a hover on the hand the clue buttons actually
            // belong to — other hands may coincidentally share a
            // color/number but aren't what's about to be clued.
            let preview_clue = if ui.selected_target.get() == Some(pid) {
                ui.hovered_clue.get()
            } else {
                None
            };
            cards
                .iter()
                .map(|c| {
                    other_card(
                        c,
                        &rules,
                        preview_clue,
                        recently_drawn_now.contains(&c.id),
                        touched_now.contains(&c.id),
                    )
                })
                .collect_view()
                .into_any()
        }
    };

    let clue_section = move || {
        if is_you || ui.selected_target.get() != Some(pid) {
            return None;
        }
        let (cards, rules, can_clue) = hanabi.view.with(|view| {
            view.as_ref().map(|v| {
                let can_clue = v.current_turn == you && v.status == GameStatus::InProgress && v.clue_tokens > 0;
                (v.hands.get(&pid).cloned().unwrap_or_default(), v.rules, can_clue)
            })
        })?;
        Some(clue_buttons(app, ui, pid, &cards, &rules, can_clue))
    };

    if is_you {
        view! {
            <div class=hand_class node_ref=node_ref>
                <div class="hand-main">
                    <div class="hand-header">
                        <h3>"Your hand"</h3>
                        {now_playing}
                        {last_move_line}
                    </div>
                    <ul class="cards">{card_items}</ul>
                </div>
                {received_clue_badge}
            </div>
        }
        .into_any()
    } else {
        view! {
            <div class=hand_class node_ref=node_ref>
                <div class="hand-main">
                    <div class="hand-header">
                        <h3
                            class="player-name"
                            on:click=move |_| {
                                ui.selected_target
                                    .update(|t| {
                                        *t = if *t == Some(pid) { None } else { Some(pid) };
                                    });
                            }
                        >
                            {name_of(pid)}
                        </h3>
                        {offline_tag}
                        {now_playing}
                        {last_move_line}
                    </div>
                    <ul class="cards">{card_items}</ul>
                </div>
                {received_clue_badge}
                {clue_section}
            </div>
        }
        .into_any()
    }
}

/// One of your own cards: you can't see its face, only what the clues have
/// told you about it, so that's all that is drawn.
fn own_card(
    c: &VisibleCard,
    rules: &GameRules,
    can_act: bool,
    ui: Ui,
    recently_drawn: bool,
    touched: bool,
) -> AnyView {
    // The card's own background already shows its color (or gradient, for
    // multicolor, or the dark black styling) once it's known or inferred —
    // same as how other players' cards never repeat their color as text
    // either. Only the ambiguity flag and the number aren't otherwise
    // visible, so those are all that get text.
    //
    // A single color clue could still be explained by the multicolor
    // wildcard rather than the color itself — flag that ambiguity rather
    // than silently picking one. Stops applying the moment any other color
    // clue comes back negative, since a multicolor card could never miss
    // one.
    let multicolor_caveat = c.knowledge.could_be_multicolor(rules);
    // A number that's *known* is drawn big (`.card-number`) so it can't be
    // mistaken for one of the small struck-through numbers a card has been
    // ruled out for. Nothing known about the number means nothing to say: an
    // empty card already reads as "don't know yet", so there's no "?"
    // placeholder — the room goes to the ruled-out numbers instead.
    let known_number = c.knowledge.known_number;

    // In hanabii mode a color clue never simply "makes the card red": a red
    // hit means red, orange *or* purple. So the card's color only counts as
    // known once the primary-color clues so far leave a single possibility
    // (red and yellow both hit → orange; red and yellow both missed → blue).
    // Until then the card keeps its neutral face and shows what it could
    // still be as a ring around it (see `hanabii_ring_colors`).
    let hanabii_color = if rules.hanabii {
        c.knowledge.hanabii_certain_color(rules)
    } else {
        None
    };
    let color_settled = if rules.hanabii {
        hanabii_color.is_some()
    } else {
        c.knowledge.known_color.is_some() || c.knowledge.inferred_black(rules)
    };
    // Ruled-out colors/numbers, shown only while that aspect is still
    // uncertain — once the color (or a black/multicolor inference) or number
    // is already known above, repeating what it *isn't* is just clutter.
    // Hanabii mode has no struck-through color marks at all: its ring
    // already lists everything the card could still be.
    let struck_colors: Vec<Color> = if color_settled || rules.hanabii {
        Vec::new()
    } else {
        c.knowledge.ruled_out_colors(rules)
    };
    let not_colors_row = (!struck_colors.is_empty()).then(|| {
        let marks = struck_colors
            .iter()
            .map(|&nc| {
                view! {
                    <span class=format!("not-mark not-mark-{}", color_class(nc))>{color_initial(nc)}</span>
                }
            })
            .collect_view();
        view! { <span class="not-row">{marks}</span> }
    });
    let not_numbers_row = c
        .knowledge
        .known_number
        .is_none()
        .then(|| {
            let mut ruled_out: Vec<u8> = c.knowledge.not_numbers.iter().copied().collect();
            ruled_out.sort_unstable();
            (!ruled_out.is_empty()).then(|| {
                let marks = ruled_out
                    .iter()
                    .map(|&n| view! { <span class="not-mark">{n.to_string()}</span> })
                    .collect_view();
                view! { <span class="not-row not-row-numbers">{marks}</span> }
            })
        })
        .flatten();

    // Color the card face itself once enough is known, same as other players
    // see it — "card-own" carries the stacked-info layout regardless of
    // which of these applies.
    let color_class_name = if rules.hanabii {
        match hanabii_color {
            Some(color) => format!("card-{}", color_class(color)),
            None => "card-unknown".to_string(),
        }
    } else if c.knowledge.inferred_multicolor() {
        "card-multicolor".to_string()
    } else if c.knowledge.inferred_black(rules) {
        "card-black".to_string()
    } else if let Some(color) = c.knowledge.known_color {
        format!("card-{}", color_class(color))
    } else {
        "card-unknown".to_string()
    };

    let card_id = c.id;
    let mut li_class = format!("card card-own {color_class_name}");
    // Hanabii mode's ring of still-possible colors, if the card has one (see
    // `hanabii_ring_colors`).
    let ring_colors = hanabii_ring_colors(&c.knowledge, rules);
    if ring_colors.is_some() {
        li_class.push_str(" card-ring");
    }
    let ring_style = ring_colors.as_deref().map(ring_stops_style).unwrap_or_default();
    if recently_drawn {
        li_class.push_str(" card-recent-draw");
    }
    if touched {
        li_class.push_str(" card-touched");
    }
    let draggable = if can_act { "true" } else { "false" };

    view! {
        <li
            class=li_class
            style=ring_style
            draggable=draggable
            on:dragstart=move |ev: web_sys::DragEvent| {
                if let Some(dt) = ev.data_transfer() {
                    let _ = dt.set_data("text/plain", &card_id.0.to_string());
                }
                ui.is_dragging.set(true);
            }
            on:dragend=move |_ev: web_sys::DragEvent| {
                ui.is_dragging.set(false);
            }
        >
            {multicolor_caveat.then(|| view! { <span class="card-hint">"M?"</span> })}
            {known_number.map(|number| view! { <span class="card-number">{number.to_string()}</span> })}
            {not_colors_row}
            {not_numbers_row}
        </li>
    }
    .into_any()
}

/// Someone else's card: face up for you, outlined if the clue button under
/// the pointer would touch it, ringed with what its owner could still think
/// it is.
fn other_card(
    c: &VisibleCard,
    rules: &GameRules,
    preview_clue: Option<Clue>,
    recently_drawn: bool,
    touched: bool,
) -> AnyView {
    let Some(card) = c.card else {
        // Other players' cards always come face up; a blank card is the
        // harmless answer to a state that shouldn't exist.
        return view! { <li class="card card-unknown"></li> }.into_any();
    };
    // Same definition of "touches" the engine uses, so the preview can't
    // drift from what the clue would actually do — including, in hanabii
    // mode, a red clue lighting up orange and purple cards too.
    let is_targeted = preview_clue.is_some_and(|clue| rules.clue_touches(clue, card));
    let mut class = if is_targeted {
        format!("card card-{} card-clue-target", color_class(card.color))
    } else {
        format!("card card-{}", color_class(card.color))
    };
    if recently_drawn {
        class.push_str(" card-recent-draw");
    }
    if touched {
        class.push_str(" card-touched");
    }
    // The same ring their owner sees on the card — every color it could
    // still be to them — so whoever's about to clue knows at a glance what's
    // still worth telling them.
    let ring_colors = hanabii_ring_colors(&c.knowledge, rules);
    if ring_colors.is_some() {
        class.push_str(" card-ring");
    }
    let ring_style = ring_colors.as_deref().map(ring_stops_style).unwrap_or_default();
    view! {
        <li class=class style=ring_style>
            {card.number.to_string()}
        </li>
    }
    .into_any()
}

/// The clue buttons offered for another player's hand: only the clues that
/// would touch something (see `valid_clues`), greyed out unless it's your
/// turn and a clue token is left. Hovering one previews the cards it would
/// touch.
fn clue_buttons(
    app: App,
    ui: Ui,
    pid: PlayerId,
    cards: &[VisibleCard],
    rules: &GameRules,
    can_clue: bool,
) -> impl IntoView {
    let (valid_colors, valid_numbers) = valid_clues(cards, rules);
    let color_buttons = valid_colors
        .iter()
        .map(|&color| {
            let tooltip = clue_touch_tooltip(rules, color);
            let paint = clue_button_style(rules, color);
            view! {
                <button
                    class=format!("clue-btn card-{}", color_class(color))
                    style=paint
                    title=tooltip
                    disabled=!can_clue
                    on:mouseenter=move |_| ui.hovered_clue.set(Some(Clue::Color(color)))
                    on:mouseleave=move |_| ui.hovered_clue.set(None)
                    on:click=move |_| {
                        send(
                            app,
                            &ClientMsg::Hanabi(Action::Clue {
                                target: pid,
                                clue: Clue::Color(color),
                            }),
                        );
                    }
                >
                    {format!("{color:?}")}
                </button>
            }
        })
        .collect_view();
    let number_buttons = valid_numbers
        .iter()
        .map(|&number| {
            view! {
                <button
                    class="clue-btn"
                    disabled=!can_clue
                    on:mouseenter=move |_| ui.hovered_clue.set(Some(Clue::Number(number)))
                    on:mouseleave=move |_| ui.hovered_clue.set(None)
                    on:click=move |_| {
                        send(
                            app,
                            &ClientMsg::Hanabi(Action::Clue {
                                target: pid,
                                clue: Clue::Number(number),
                            }),
                        );
                    }
                >
                    {number.to_string()}
                </button>
            }
        })
        .collect_view();

    view! {
        <div class="clue-options">
            <p class="hint">"Give a clue:"</p>
            <div class="clue-buttons">{color_buttons}</div>
            <div class="clue-buttons">{number_buttons}</div>
        </div>
    }
}

/// Native render tests: build real games with the rules crate and render the
/// board for every seat, which catches the panics (bad indexing, unwraps)
/// the type checker can't see. The effects never run here (that needs a
/// browser), so what these cover is what the first paint looks like and the
/// small pure pieces; the live behaviour — animations, drag and drop — has
/// to be checked in a browser.
#[cfg(test)]
mod tests {
    use super::*;
    use hanabi_core::{Card, CardKnowledge, GameState};
    use leptos::tachys::view::RenderHtml;

    const NAMES: [&str; 5] = ["Ann", "Bob", "Cy", "Di", "Ed"];

    /// Runs `f` inside a fresh reactive owner, with an executor registered so
    /// that creating effects doesn't panic (the tasks are never polled).
    fn in_owner<R>(f: impl FnOnce() -> R) -> R {
        let _ = any_spawner::Executor::init_futures_executor();
        Owner::new().with(f)
    }

    /// An app that is looking at `game` from `seat`.
    fn app_showing(game: &GameState, seat: u8) -> App {
        let app = App::new();
        let n = game.players.len();
        app.hanabi.names.set(NAMES[..n].iter().map(|s| s.to_string()).collect());
        app.hanabi.connected.set(vec![true; n]);
        app.hanabi.room.set("ABCD".into());
        app.hanabi.view.set(Some(game.view_for(PlayerId(seat))));
        app
    }

    fn render_board(game: &GameState, seat: u8) -> String {
        in_owner(|| board(app_showing(game, seat)).to_html())
    }

    fn rule_sets() -> Vec<GameRules> {
        vec![
            GameRules::default(),
            GameRules { hanabii: true, ..Default::default() },
            GameRules { multicolor: true, black: true, ..Default::default() },
            GameRules { six_cards: true, extra_colors: 2, multicolor: true, ..Default::default() },
            GameRules { extra_colors: 1, black_short: true, black: true, ..Default::default() },
        ]
    }

    fn visible(id: u32, card: Option<Card>, knowledge: CardKnowledge) -> VisibleCard {
        VisibleCard { id: CardId(id), card, knowledge }
    }

    fn html_of(view: AnyView) -> String {
        in_owner(|| view.to_html())
    }

    #[test]
    fn every_seat_sees_its_own_hand_hidden_and_the_others_face_up() {
        for rules in rule_sets() {
            for players in 2..=5u8 {
                let game = GameState::new(players, 7 + players as u64, rules);
                let hand_size = if players <= 3 { 5 } else { 4 };
                for seat in 0..players {
                    let html = render_board(&game, seat);
                    assert!(html.contains("Your hand") && html.contains("Discard pile"), "{rules:?}");
                    assert!(html.contains("Drag a card here to play it."));
                    assert!(html.contains("ABCD") && html.contains("Leave game"));
                    // One tile per active color.
                    assert_eq!(html.matches("class=\"firework ").count(), game.rules.active_colors().len());
                    // Own cards: all there, none with a face (no numbers known yet).
                    assert_eq!(html.matches("card-own").count(), hand_size, "{rules:?} seat {seat}");
                    // Everybody else, by name, with their cards face up.
                    for other in (0..players).filter(|&o| o != seat) {
                        assert!(html.contains(NAMES[other as usize]), "{} missing", NAMES[other as usize]);
                        for card in &game.hands[&PlayerId(other)] {
                            assert!(html.contains(&format!("card-{}", color_class(card.card.color))));
                        }
                    }
                    assert!(!html.contains("Give a clue:"), "clue buttons only open on a click");
                }
            }
        }
    }

    #[test]
    fn only_the_player_whose_turn_it_is_can_drag_cards_and_the_zones_follow() {
        let game = GameState::new(3, 5, GameRules::default());
        // Seat 0 has the turn; the clue tokens are full, so a discard is not allowed.
        let mine = render_board(&game, 0);
        assert_eq!(mine.matches("draggable=\"true\"").count(), 5);
        assert!(mine.contains("class=\"panel drop-zone\""), "play zone open");
        assert!(mine.contains("class=\"panel drop-zone disabled\""), "discard zone shut with all clues in hand");
        assert!(mine.contains("Now playing"));

        let theirs = render_board(&game, 1);
        assert_eq!(theirs.matches("draggable=\"true\"").count(), 0);
        assert_eq!(theirs.matches("draggable=\"false\"").count(), 5);
        assert_eq!(theirs.matches("panel drop-zone disabled").count(), 2);

        // With a clue token spent the discard zone opens too.
        let mut game = game;
        game.clue_tokens = 7;
        let mine = render_board(&game, 0);
        assert_eq!(mine.matches("class=\"panel drop-zone\"").count(), 2);
    }

    #[test]
    fn the_end_of_a_game_names_the_reason_and_the_score_and_locks_the_table() {
        for (reason, why) in [
            (EndReason::FusesExhausted, "ran out of fuses"),
            (EndReason::DeckExhausted, "the deck ran out"),
            (EndReason::PerfectScore, "a perfect score"),
        ] {
            let mut game = GameState::new(2, 3, GameRules::default());
            game.fireworks.insert(Color::Red, 3);
            game.fireworks.insert(Color::Blue, 2);
            game.status = GameStatus::Finished(reason);
            let html = render_board(&game, 0);
            assert!(html.contains("Game over") && html.contains(why), "{html}");
            assert!(html.contains("Final score: 5/25"));
            assert_eq!(html.matches("draggable=\"true\"").count(), 0);
            assert_eq!(html.matches("panel drop-zone disabled").count(), 2);
            assert!(!html.contains("Now playing") || html.matches("Now playing").count() == 1);
        }
        let ongoing = render_board(&GameState::new(2, 3, GameRules::default()), 0);
        assert!(!ongoing.contains("Game over"));
    }

    #[test]
    fn fireworks_grow_with_the_suit_and_a_finished_suit_gets_the_big_burst() {
        let mut game = GameState::new(2, 3, GameRules::default());
        game.fireworks.insert(Color::Red, 5);
        game.fireworks.insert(Color::Green, 2);
        let html = render_board(&game, 0);
        assert_eq!(html.matches("firework-complete").count(), 1, "only red is done");
        assert_eq!(html.matches("firework-burst--complete").count(), 1);
        assert!(html.contains("firework firework-red firework-complete"));
        // Value labels: the played rank, or a dash for an untouched suit.
        assert!(html.contains(">5<") && html.contains(">2<") && html.contains("—"));

        // With six-card suits a suit at 5 still has one play to go.
        let mut game = GameState::new(2, 3, GameRules { six_cards: true, ..Default::default() });
        game.fireworks.insert(Color::Red, 5);
        let html = render_board(&game, 0);
        assert_eq!(html.matches("firework-burst--complete").count(), 0);
        game.fireworks.insert(Color::Red, 6);
        assert_eq!(render_board(&game, 0).matches("firework-burst--complete").count(), 1);

        // Black is built downwards and says so; short suits carry their badge.
        let game = GameState::new(
            2,
            3,
            GameRules { black: true, black_short: true, ..Default::default() },
        );
        let html = render_board(&game, 0);
        assert!(html.contains("Black \u{2193}"));
        assert!(html.contains("firework-short-badge"));
    }

    #[test]
    fn the_discard_pile_is_grouped_by_color_and_sorted() {
        let mut game = GameState::new(2, 3, GameRules::default());
        game.discard_pile = vec![
            Card { color: Color::Red, number: 3 },
            Card { color: Color::Blue, number: 2 },
            Card { color: Color::Red, number: 1 },
        ];
        let html = render_board(&game, 0);
        assert_eq!(html.matches("class=\"discard-row\"").count(), 2);
        assert!(!html.contains("Nothing discarded yet."));
        let red_row = html.split("class=\"discard-row\"").nth(1).unwrap();
        let (one, three) = (red_row.find(">1<").unwrap(), red_row.find(">3<").unwrap());
        assert!(one < three, "chips are sorted: {red_row}");

        let empty = render_board(&GameState::new(2, 3, GameRules::default()), 0);
        assert!(empty.contains("Nothing discarded yet."));
        assert!(!empty.contains("discard-row"));
    }

    #[test]
    fn only_changes_flash_never_the_first_paint() {
        in_owner(|| {
            let mut game = GameState::new(2, 3, GameRules::default());
            game.fireworks.insert(Color::Red, 2);
            game.discard_pile = vec![Card { color: Color::Blue, number: 1 }];
            let app = app_showing(&game, 0);
            let played = StoredValue::new(None::<HashMap<Color, u8>>);
            let discarded = StoredValue::new(None::<usize>);
            let paint = || table_view(app, Ui::new(), PlayerId(0), played, discarded).to_html();

            // Joining (or rejoining) a game in progress: nothing there is "new".
            let first = paint();
            assert!(!first.contains("firework-flash") && !first.contains("discard-flash"));
            assert!(!paint().contains("flash"), "and repainting the same state changes nothing");

            // Red gains a card and a card is discarded: exactly those two flash...
            game.fireworks.insert(Color::Red, 3);
            game.discard_pile.push(Card { color: Color::Green, number: 4 });
            app.hanabi.view.set(Some(game.view_for(PlayerId(0))));
            let after = paint();
            assert_eq!(after.matches("firework-flash").count(), 1);
            assert!(after.contains("firework firework-red firework-flash"));
            assert!(after.contains("discard-groups discard-flash"));

            // ...once.
            assert!(!paint().contains("flash"));
        });
    }

    #[test]
    fn a_whole_game_can_be_rendered_from_every_seat_at_every_step() {
        for players in [2u8, 3, 5] {
            for rules in rule_sets() {
                let mut game = GameState::new(players, 21, rules);
                let mut moves = 0;
                while game.status == GameStatus::InProgress && moves < 400 {
                    for seat in 0..players {
                        let html = render_board(&game, seat);
                        assert!(html.contains("Your hand"));
                    }
                    // A simple table: clue the next player while tokens last,
                    // otherwise play the first card (misplays end it sooner or later).
                    let me = game.current_player();
                    let next = PlayerId((me.0 + 1) % players);
                    let action = if game.clue_tokens > 0 && moves % 3 != 2 {
                        let touched = game.hands[&next][0].card;
                        let clue = if moves % 2 == 0 && !game.rules.hanabii {
                            Clue::Color(touched.color)
                        } else {
                            Clue::Number(touched.number)
                        };
                        Action::Clue { target: next, clue }
                    } else if moves % 3 == 2 && game.clue_tokens < 8 {
                        Action::Discard { card_id: game.hands[&me][0].id }
                    } else {
                        Action::Play { card_id: game.hands[&me][0].id }
                    };
                    // Some clues are illegal (touching nothing); skip those by playing on.
                    if game.apply_action(me, action).is_err() {
                        let card_id = game.hands[&me][0].id;
                        let _ = game.apply_action(me, Action::Play { card_id });
                    }
                    moves += 1;
                }
                for seat in 0..players {
                    let html = render_board(&game, seat);
                    assert!(html.contains("Your hand"));
                    if game.status != GameStatus::InProgress {
                        assert!(html.contains("Game over"), "{rules:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn a_hand_shows_who_is_up_what_they_did_and_whether_they_are_connected() {
        let mut game = GameState::new(3, 9, GameRules::default());
        // Ann clues Bob about a number that is in his hand; now it's Bob's turn.
        let number = game.hands[&PlayerId(1)][0].card.number;
        let touched = game.hands[&PlayerId(1)].iter().filter(|c| c.card.number == number).count();
        game.apply_action(PlayerId(0), Action::Clue { target: PlayerId(1), clue: Clue::Number(number) })
            .unwrap();

        in_owner(|| {
            let app = app_showing(&game, 2);
            app.hanabi.connected.set(vec![true, false, true]);
            let ui = Ui::new();
            let bob = hand(app, ui, PlayerId(2), PlayerId(1), NodeRef::new()).to_html();
            assert!(bob.contains("Bob") && bob.contains("hand-current") && bob.contains("Now playing"));
            assert!(bob.contains("offline"), "his connection is down");
            assert!(!bob.contains("Give a clue:") && !bob.contains("hand-selected"));

            let ann = hand(app, ui, PlayerId(2), PlayerId(0), NodeRef::new()).to_html();
            let plural = if touched == 1 { "1 card" } else { &format!("{touched} cards") };
            assert!(ann.contains(&format!("Clued Bob about {number} ({plural})")), "{ann}");
            assert!(!ann.contains("offline") && !ann.contains("Now playing"));

            // Your own hand has no name to click and no clue buttons.
            let mine = hand(app, ui, PlayerId(2), PlayerId(2), NodeRef::new()).to_html();
            assert!(mine.contains("Your hand") && !mine.contains("player-name"));

            // Clicking a name opens that player's clue buttons — which stay shut
            // while it isn't your turn — and outlines the hand.
            ui.selected_target.set(Some(PlayerId(1)));
            let bob = hand(app, ui, PlayerId(2), PlayerId(1), NodeRef::new()).to_html();
            assert!(bob.contains("hand-selected") && bob.contains("Give a clue:"));
            assert!(bob.contains("disabled"), "not your turn: {bob}");
            // Never on your own hand, whatever is selected.
            ui.selected_target.set(Some(PlayerId(2)));
            let mine = hand(app, ui, PlayerId(2), PlayerId(2), NodeRef::new()).to_html();
            assert!(!mine.contains("Give a clue:") && !mine.contains("hand-selected"));
        });
    }

    #[test]
    fn a_clue_on_screen_flashes_the_cards_it_touched_and_badges_the_receiver() {
        let game = GameState::new(2, 4, GameRules::default());
        in_owner(|| {
            let app = app_showing(&game, 0);
            let ui = Ui::new();
            let bobs: Vec<CardId> = game.hands[&PlayerId(1)].iter().map(|c| c.id).collect();
            ui.touched_flash.set(bobs.iter().copied().take(2).collect());
            ui.last_clue_badge.set(Some((PlayerId(1), Clue::Number(3))));

            let bob = hand(app, ui, PlayerId(0), PlayerId(1), NodeRef::new()).to_html();
            assert_eq!(bob.matches("card-touched").count(), 2);
            assert!(bob.contains("received-clue") && bob.contains("clue-image"));
            assert!(bob.contains(">3<"));

            // The badge belongs to the receiver only.
            let mine = hand(app, ui, PlayerId(0), PlayerId(0), NodeRef::new()).to_html();
            assert!(!mine.contains("received-clue") && !mine.contains("card-touched"));
        });
    }

    #[test]
    fn your_own_card_shows_only_what_the_clues_told_you() {
        let ordinary = GameRules::default();
        let own = |k: CardKnowledge, rules: &GameRules| {
            html_of(own_card(&visible(1, None, k), rules, true, Ui::new(), false, false))
        };

        // Nothing known: a blank card.
        let blank = own(CardKnowledge::default(), &ordinary);
        assert!(blank.contains("card-unknown") && !blank.contains("card-number") && !blank.contains("not-row"));
        assert!(blank.contains("draggable=\"true\""));

        // Known color and number: the face takes the color, the number is big.
        let known = CardKnowledge {
            known_color: Some(Color::Red),
            known_number: Some(3),
            ..Default::default()
        };
        let html = own(known, &ordinary);
        assert!(html.contains("card-red") && html.contains("card-number") && html.contains(">3<"));

        // Ruled-out numbers and colors, struck through, until they are settled.
        let ruled = CardKnowledge {
            not_numbers: [2, 1].into_iter().collect(),
            not_colors: [Color::Blue].into_iter().collect(),
            ..Default::default()
        };
        let html = own(ruled.clone(), &ordinary);
        assert!(html.contains("not-row-numbers"));
        assert!(html.find(">1<").unwrap() < html.find(">2<").unwrap(), "sorted: {html}");
        assert!(html.contains("not-mark-blue") && html.contains(">B<"));
        let settled = CardKnowledge { known_number: Some(4), known_color: Some(Color::Green), ..ruled };
        let html = own(settled, &ordinary);
        assert!(!html.contains("not-row"), "nothing left to rule out: {html}");

        // A single color hit could still be the multicolor wildcard.
        let wild = GameRules { multicolor: true, ..Default::default() };
        let hit = CardKnowledge { known_color: Some(Color::Red), clued_colors: [Color::Red].into_iter().collect(), ..Default::default() };
        assert!(own(hit.clone(), &wild).contains("M?"));
        assert!(!own(hit.clone(), &ordinary).contains("M?"));
        let two = CardKnowledge { clued_colors: [Color::Red, Color::Blue].into_iter().collect(), ..hit };
        let html = own(two, &wild);
        assert!(html.contains("card-multicolor") && !html.contains("M?"));

        // Everything else ruled out leaves only black.
        let black_rules = GameRules { black: true, ..Default::default() };
        let all_missed = CardKnowledge {
            not_colors: black_rules
                .active_colors()
                .into_iter()
                .filter(|&c| c != Color::Black)
                .collect(),
            ..Default::default()
        };
        assert!(own(all_missed, &black_rules).contains("card-black"));

        // Fresh draws and just-clued cards are flagged.
        let k = CardKnowledge::default();
        let html = html_of(own_card(&visible(2, None, k), &ordinary, false, Ui::new(), true, true));
        assert!(html.contains("card-recent-draw") && html.contains("card-touched"));
        assert!(html.contains("draggable=\"false\""));
    }

    #[test]
    fn in_hanabii_mode_an_undecided_card_gets_a_ring_and_a_decided_one_its_color() {
        let rules = GameRules { hanabii: true, ..Default::default() }.normalized();
        let own = |k: CardKnowledge| html_of(own_card(&visible(1, None, k), &rules, true, Ui::new(), false, false));

        // Untouched: neutral face, no ring.
        let html = own(CardKnowledge::default());
        assert!(html.contains("card-unknown") && !html.contains("card-ring"));

        // One red hit: red, orange or purple — a three-arc ring on a neutral face.
        let red = CardKnowledge { hit_primaries: [Color::Red].into_iter().collect(), ..Default::default() };
        let html = own(red.clone());
        assert!(html.contains("card-unknown") && html.contains("card-ring"));
        assert!(html.contains("--ring-stops") && html.contains("var(--orange-fw)"));
        assert!(!html.contains("not-mark-"), "the ring replaces the struck-out colors");

        // Red and yellow both hit: orange, and the whole face says so.
        let orange = CardKnowledge { hit_primaries: [Color::Red, Color::Yellow].into_iter().collect(), ..red.clone() };
        let html = own(orange);
        assert!(html.contains("card-orange") && !html.contains("card-ring"));

        // Red and yellow both missed: blue.
        let blue = CardKnowledge { missed_primaries: [Color::Red, Color::Yellow].into_iter().collect(), ..Default::default() };
        assert!(own(blue).contains("card-blue"));

        // Everyone else's view of that card carries the same ring.
        let other = html_of(other_card(
            &visible(3, Some(Card { color: Color::Purple, number: 2 }), red),
            &rules,
            None,
            false,
            false,
        ));
        assert!(other.contains("card-purple") && other.contains("card-ring"));
    }

    #[test]
    fn another_players_card_previews_the_clue_under_the_pointer() {
        let ordinary = GameRules::default();
        let red3 = visible(1, Some(Card { color: Color::Red, number: 3 }), CardKnowledge::default());
        let other = |c: &VisibleCard, rules: &GameRules, clue| html_of(other_card(c, rules, clue, false, false));

        assert!(!other(&red3, &ordinary, None).contains("card-clue-target"));
        assert!(other(&red3, &ordinary, Some(Clue::Color(Color::Red))).contains("card-clue-target"));
        assert!(!other(&red3, &ordinary, Some(Clue::Color(Color::Blue))).contains("card-clue-target"));
        assert!(other(&red3, &ordinary, Some(Clue::Number(3))).contains("card-clue-target"));
        assert!(!other(&red3, &ordinary, Some(Clue::Number(2))).contains("card-clue-target"));
        assert!(other(&red3, &ordinary, None).contains(">3<"));

        // Hanabii: a red clue also lights up orange and purple, but not blue.
        let rules = GameRules { hanabii: true, ..Default::default() }.normalized();
        let card = |color| visible(2, Some(Card { color, number: 1 }), CardKnowledge::default());
        let red = Some(Clue::Color(Color::Red));
        assert!(other(&card(Color::Orange), &rules, red).contains("card-clue-target"));
        assert!(other(&card(Color::Purple), &rules, red).contains("card-clue-target"));
        assert!(!other(&card(Color::Blue), &rules, red).contains("card-clue-target"));

        // Fresh draws and just-clued cards are flagged here too.
        let html = html_of(other_card(&red3, &ordinary, None, true, true));
        assert!(html.contains("card-recent-draw") && html.contains("card-touched"));

        // A face-down card among someone else's is not expected, and not fatal.
        let hidden = visible(9, None, CardKnowledge::default());
        assert!(other(&hidden, &ordinary, None).contains("card-unknown"));
    }

    #[test]
    fn clue_buttons_offer_what_would_touch_something_and_lock_without_a_token() {
        let cards = vec![
            visible(1, Some(Card { color: Color::Red, number: 1 }), CardKnowledge::default()),
            visible(2, Some(Card { color: Color::Red, number: 4 }), CardKnowledge::default()),
            visible(3, Some(Card { color: Color::Green, number: 4 }), CardKnowledge::default()),
        ];
        in_owner(|| {
            let app = App::new();
            let ui = Ui::new();
            let ordinary = GameRules::default();
            let open = clue_buttons(app, ui, PlayerId(1), &cards, &ordinary, true).into_any().to_html();
            assert!(open.contains("Give a clue:"));
            assert!(open.contains(">Red<") && open.contains(">Green<"));
            assert!(!open.contains(">Blue<") && !open.contains(">Yellow<"), "nothing blue to point at");
            assert!(open.contains(">1<") && open.contains(">4<") && !open.contains(">2<"));
            assert!(!open.contains("disabled") && !open.contains("title=\"Touches"));

            // No clue token (or not your turn): the same buttons, locked.
            let locked = clue_buttons(app, ui, PlayerId(1), &cards, &ordinary, false).into_any().to_html();
            assert_eq!(locked.matches("disabled").count(), 4);

            // Hanabii: all three primaries whatever the hand holds, painted with
            // the colors they touch and explained on hover.
            let rules = GameRules { hanabii: true, ..Default::default() }.normalized();
            let html = clue_buttons(app, ui, PlayerId(1), &cards, &rules, true).into_any().to_html();
            for primary in ["Red", "Yellow", "Blue"] {
                assert!(html.contains(&format!(">{primary}<")), "{primary} missing");
            }
            assert!(!html.contains(">Orange<") && !html.contains(">Green<") && !html.contains(">Purple<"));
            assert!(html.contains("linear-gradient"));
            assert!(html.contains("Touches red, orange and purple cards"));
        });
    }

    #[test]
    fn the_board_without_a_game_says_so_instead_of_panicking() {
        let html = in_owner(|| board(App::new()).to_html());
        assert!(html.contains("Waiting for the game to start"));
    }

    #[test]
    fn small_pure_helpers() {
        assert_eq!(drop_zone_class(false, true), "panel drop-zone disabled");
        assert_eq!(drop_zone_class(true, true), "panel drop-zone drag-active");
        assert_eq!(drop_zone_class(true, false), "panel drop-zone");

        in_owner(|| {
            let app = App::new();
            app.hanabi.names.set(vec!["Ann".into(), "Bob".into()]);
            assert_eq!(player_name(app, PlayerId(1)), "Bob");
            assert_eq!(player_name(app, PlayerId(4)), "Player 5");
            assert!(!can_act(app, PlayerId(0)), "no game, no turn");

            let game = GameState::new(2, 1, GameRules::default());
            app.hanabi.view.set(Some(game.view_for(PlayerId(0))));
            assert!(can_act(app, PlayerId(0)) && !can_act(app, PlayerId(1)));
        });

        // The firework icon grows ray by ray, then jumps to the dense burst.
        let rays = |progress, complete| html_of(firework_burst(progress, complete)).matches("<line").count();
        assert_eq!([rays(0, false), rays(1, false), rays(2, false), rays(3, false), rays(4, false), rays(5, false)], [0, 1, 2, 4, 6, 8]);
        assert_eq!(rays(5, true), 12);
    }
}
