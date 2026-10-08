//! The Hanabi screens around the board: the page frame, create/join, and
//! the waiting room with its rules picker.
//!
//! The picker is ported from the stand-alone client. What changed is who may
//! use it: rooms now have a host, and only the host changes the rules (and
//! starts the game); everyone else sees the same controls, locked.

use crate::state::{create_room, join_room, leave, send, App, Screen};
use hanabi_core::GameRules;
use leptos::prelude::*;
use protocol::{ClientMsg, GameKind, LobbyPlayer};

/// The rules of the hanabii mode as shown in the box that appears when the
/// page title is hovered: the clue rule first, then how your own cards show
/// what you know.
const HANABII_CLUE_RULES: &str = "Only red, yellow and blue can be clued — and always, even when a clue touches nothing, since ruling a color out is information too. Orange is red + yellow, green is yellow + blue and purple is red + blue, so a red clue touches every red, orange and purple card, and so on.";
const HANABII_CARD_MARKERS: &str = "A spinning ring shows every color a card could still be — on your own cards, and on everyone else's so you can see what they know — and the whole card fills in once its color is certain.";

/// The page title of a Hanabi screen, which the browser tab follows.
pub fn tab_title(app: App) -> &'static str {
    if app.hanabi.hanabii() {
        "Hanabii"
    } else {
        "Hanabi"
    }
}

/// The frame every Hanabi screen sits in: its own look (see `hanabi.css`,
/// everything in it is scoped under `.hanabi`) and the page title.
pub fn shell(app: App, content: impl IntoView + 'static) -> impl IntoView {
    // A memo, so the title is only rebuilt when the mode flips, not on the
    // (many) state updates that leave it alone.
    let hanabii = Memo::new(move |_| app.hanabi.hanabii());
    view! {
        <div class="hanabi">
            <main class="app">
                {move || {
                    if hanabii.get() {
                        // Focusable, so the box also opens on a tap or from the
                        // keyboard where there's no hover.
                        view! {
                            <div class="page-title page-title-hanabii" tabindex="0">
                                <h1>"Hanabii"</h1>
                                <div class="mode-popover" role="tooltip">
                                    <p>{HANABII_CLUE_RULES}</p>
                                    <p>{HANABII_CARD_MARKERS}</p>
                                </div>
                            </div>
                        }
                        .into_any()
                    } else {
                        view! {
                            <div class="page-title">
                                <h1>"Hanabi"</h1>
                            </div>
                        }
                        .into_any()
                    }
                }}
                {content}
            </main>
        </div>
    }
}

/// Name + create a room, or join one with its code.
pub fn home_screen(app: App) -> impl IntoView {
    let create = move |_| create_room(app, GameKind::Hanabi);
    let join = move |_| join_room(app);

    view! {
        <div class="panel join-form">
            <label>
                "Your name"
                <input
                    type="text"
                    maxlength="20"
                    prop:value=move || app.name.get()
                    on:input=move |ev| app.name.set(event_target_value(&ev))
                />
            </label>
            <button on:click=create disabled=move || !app.online.get()>
                "Create a room"
            </button>
            <p class="hint">"or join a friend's room with its code"</p>
            <label>
                "Room code"
                <input
                    type="text"
                    maxlength="4"
                    placeholder="ABCD"
                    class="code-input"
                    prop:value=move || app.room_input.get()
                    on:input=move |ev| app.room_input.set(event_target_value(&ev).to_uppercase())
                />
            </label>
            <button on:click=join disabled=move || !app.online.get()>
                "Join room"
            </button>
            <button class="link" on:click=move |_| app.screen.set(Screen::Menu)>
                "← All games"
            </button>
            <p class="hint">
                "A cooperative game for 2 to 5 players. Share the room code with everyone you're playing with."
            </p>
        </div>
    }
}

fn set_rules(app: App, rules: GameRules) {
    send(app, &ClientMsg::SetRules(rules));
}

/// The class for one of the ordinary option blocks below: dimmed and
/// inert-looking while the hanabii mode is on, since that mode is a fixed
/// preset that replaces all of them (the server swaps the preset in when
/// it's picked and echoes it back, so the locked controls show exactly what
/// the mode plays with).
fn rule_group_class(app: App) -> &'static str {
    if app.hanabi.rules.get().hanabii {
        "rule-group rule-group-locked"
    } else {
        "rule-group"
    }
}

/// Renders the "hanabii mode" lobby control: one on/off toggle for the
/// fixed preset described in `GameRules::hanabii` — six colors, six-card
/// suits, and primary-color-only clues where orange, green and purple are
/// mixed from red, yellow and blue. While it's on, every other option is
/// locked (see `rule_group_class`).
///
/// Same server-authoritative pattern as every other toggle here: this only
/// ever asks for the change, and the lobby echo is what moves the
/// checkbox. Picking the mode just flags it — the server replaces the rest
/// with the preset. Un-picking goes back to a plain default game rather
/// than leaving the preset's options ticked behind it, so unticking really
/// does mean "back to normal".
fn hanabii_mode_control(app: App, can_edit: bool) -> impl IntoView {
    let on_toggle = move |_| {
        let rules = app.hanabi.rules.get_untracked();
        let rules = if rules.hanabii {
            GameRules::default()
        } else {
            GameRules { hanabii: true, ..rules }
        };
        set_rules(app, rules);
    };

    view! {
        <div class="rule-group">
            <label class="rule-toggle">
                <input
                    type="checkbox"
                    prop:checked=move || app.hanabi.rules.get().hanabii
                    disabled=!can_edit
                    on:change=on_toggle
                />
                <span>"Hanabii mode"</span>
            </label>
            <p class="hint">
                "The real deal: six colors (red, orange, yellow, green, blue, purple) with six cards each — three 1s, two each of 2 to 5, one 6 — for a max score of 36. Only the primary colors (red, yellow, blue) can be clued, and every other color is mixed from them: orange is red + yellow, green is yellow + blue, purple is red + blue. So a red clue touches every red, orange and purple card. Picking it locks all the other options below."
            </p>
        </div>
    }
}

/// Renders one optional suit's lobby controls: a main on/off toggle, its
/// blurb, and a "short deck" sub-toggle (one copy of each rank instead of
/// the usual distribution) that's only meaningful — and only enabled —
/// once the suit itself is on. Both are locked while the hanabii mode is
/// on (see `hanabii_mode_control`).
///
/// `get`/`set` read and write the suit's own on/off flag; `get_short`/
/// `set_short` do the same for its short-deck flag. Rules are
/// server-authoritative and shared by the whole lobby: rather than trust
/// an uncontrolled checkbox, every toggle here always flips the last
/// confirmed rules and lets the server's echo be what actually moves the
/// checkbox.
#[allow(clippy::too_many_arguments)]
fn suit_rule_toggle(
    app: App,
    can_edit: bool,
    label: &'static str,
    blurb: &'static str,
    get: impl Fn(&GameRules) -> bool + Copy + Send + Sync + 'static,
    set: impl Fn(&mut GameRules, bool) + Copy + Send + Sync + 'static,
    get_short: impl Fn(&GameRules) -> bool + Copy + Send + Sync + 'static,
    set_short: impl Fn(&mut GameRules, bool) + Copy + Send + Sync + 'static,
) -> impl IntoView {
    let on_toggle = move |_| {
        let mut rules = app.hanabi.rules.get_untracked();
        let was_on = get(&rules);
        set(&mut rules, !was_on);
        set_rules(app, rules);
    };
    let on_toggle_short = move |_| {
        let mut rules = app.hanabi.rules.get_untracked();
        let was_on = get_short(&rules);
        set_short(&mut rules, !was_on);
        set_rules(app, rules);
    };

    view! {
        <div class=move || rule_group_class(app)>
            <label class="rule-toggle">
                <input
                    type="checkbox"
                    prop:checked=move || get(&app.hanabi.rules.get())
                    disabled={move || !can_edit || app.hanabi.rules.get().hanabii}
                    on:change=on_toggle
                />
                <span>{label}</span>
            </label>
            <p class="hint">{blurb}</p>
            <label class="rule-toggle rule-toggle-sub">
                <input
                    type="checkbox"
                    prop:checked=move || get_short(&app.hanabi.rules.get())
                    disabled={move || {
                        let rules = app.hanabi.rules.get();
                        !can_edit || rules.hanabii || !get(&rules)
                    }}
                    on:change=on_toggle_short
                />
                <span>"Only 1 of each card (harder)"</span>
            </label>
        </div>
    }
}

/// Renders the "extra colors" lobby control: a 0-2 count selector (picked
/// in priority from ordinary, non-special suits — orange first, then
/// purple) and a shared "short deck" sub-toggle that applies uniformly to
/// however many are added. Kept separate from `suit_rule_toggle` since its
/// shape is a count, not a plain on/off flag.
fn extra_colors_control(app: App, can_edit: bool) -> impl IntoView {
    let on_change_count = move |ev: leptos::ev::Event| {
        let value: u8 = event_target_value(&ev).parse().unwrap_or(0).min(2);
        let mut rules = app.hanabi.rules.get_untracked();
        rules.extra_colors = value;
        set_rules(app, rules);
    };
    let on_toggle_short = move |_| {
        let mut rules = app.hanabi.rules.get_untracked();
        rules.extra_colors_short = !rules.extra_colors_short;
        set_rules(app, rules);
    };
    let option = move |count: u8| {
        view! {
            <option value=count.to_string() prop:selected=move || app.hanabi.rules.get().extra_colors == count>
                {count.to_string()}
            </option>
        }
    };

    view! {
        <div class=move || rule_group_class(app)>
            <label class="rule-select">
                <span>"Extra colors"</span>
                <select
                    disabled={move || !can_edit || app.hanabi.rules.get().hanabii}
                    on:change=on_change_count
                >
                    {option(0)}
                    {option(1)}
                    {option(2)}
                </select>
            </label>
            <p class="hint">"0: just the plain five (max score 25). 1: Orange and Purple both come in and White drops out to make room — 6 suits, max score 30. 2: White comes back too, all seven suits at once, max score 35. (6 per suit instead of 5 with six-card suits.)"</p>
            <label class="rule-toggle rule-toggle-sub">
                <input
                    type="checkbox"
                    prop:checked=move || app.hanabi.rules.get().extra_colors_short
                    disabled={move || {
                        let rules = app.hanabi.rules.get();
                        !can_edit || rules.hanabii || rules.extra_colors == 0
                    }}
                    on:change=on_toggle_short
                />
                <span>"Only 1 of each card (harder)"</span>
            </label>
        </div>
    }
}

/// Renders the "six-card suits" lobby control: a single on/off toggle that
/// extends *every* active suit's distribution by one rank (see
/// `GameRules::six_cards`), base five included. Unlike the suits above,
/// this isn't its own suit to turn on — it's a modifier that applies
/// uniformly to whichever suits end up active, so it gets a plain on/off
/// block of its own rather than reusing `suit_rule_toggle`'s shape.
fn six_cards_control(app: App, can_edit: bool) -> impl IntoView {
    let on_toggle = move |_| {
        let mut rules = app.hanabi.rules.get_untracked();
        rules.six_cards = !rules.six_cards;
        set_rules(app, rules);
    };

    view! {
        <div class=move || rule_group_class(app)>
            <label class="rule-toggle">
                <input
                    type="checkbox"
                    prop:checked=move || app.hanabi.rules.get().six_cards
                    disabled={move || !can_edit || app.hanabi.rules.get().hanabii}
                    on:change=on_toggle
                />
                <span>"Six-card suits"</span>
            </label>
            <p class="hint">
                "Adds a 6th card to every active suit. A suit's old unique 5 becomes a pair, and 6 becomes the new unique top card — mirrored for Black powder, where 6 becomes the abundant starting card instead. Adds 1 to the max score per active suit."
            </p>
        </div>
    }
}

/// The waiting room: who is here, the rules (the host's to change), and the
/// host's start button.
pub fn lobby_screen(
    app: App,
    room: String,
    players: Vec<LobbyPlayer>,
    you: usize,
    is_host: bool,
) -> impl IntoView {
    let can_start = players.len() >= GameKind::Hanabi.min_players();
    let rows = players
        .iter()
        .enumerate()
        .map(|(i, p)| {
            view! {
                <li>
                    {p.name.clone()}
                    {(i == you).then_some(" (you)")}
                    {(i == 0).then(|| view! { <span class="tag">"host"</span> })}
                    {(!p.connected).then(|| view! { <span class="tag tag-off">"offline"</span> })}
                </li>
            }
        })
        .collect_view();

    let start = if is_host {
        view! {
            <button disabled=!can_start on:click=move |_| send(app, &ClientMsg::Start)>
                "Start game"
            </button>
            <p class="hint">
                {if can_start {
                    "Everyone who's here is in. Start whenever you're ready."
                } else {
                    "Needs at least 2 players."
                }}
            </p>
        }
        .into_any()
    } else {
        view! { <p class="hint">"Waiting for the host to pick the rules and start the game…"</p> }.into_any()
    };

    view! {
        <div class="panel">
            <h2>"Waiting for players"</h2>
            <p class="hint">"Share this code with everyone you're playing with:"</p>
            <div class="room-code">{room}</div>
            <ul class="roster">{rows}</ul>

            <div class="rules-picker">
                <h3>"Game mode"</h3>
                {hanabii_mode_control(app, is_host).into_any()}

                <h3>"House rules"</h3>

                {suit_rule_toggle(
                    app,
                    is_host,
                    "Multicolor suit",
                    "Adds a 6th suit that's wild for color clues but can't be clued directly. Adds 5 to the max score (6 with six-card suits).",
                    |r| r.multicolor,
                    |r, v| r.multicolor = v,
                    |r| r.multicolor_short,
                    |r, v| r.multicolor_short = v,
                )
                .into_any()}
                {suit_rule_toggle(
                    app,
                    is_host,
                    "Black powder suit",
                    "Adds a suit with no color at all — color clues never touch it — played 5 down to 1 instead of 1 up to 5 (6 down to 1 with six-card suits). Adds 5 to the max score (6 with six-card suits).",
                    |r| r.black,
                    |r, v| r.black = v,
                    |r| r.black_short,
                    |r, v| r.black_short = v,
                )
                .into_any()}
                {extra_colors_control(app, is_host).into_any()}
                {six_cards_control(app, is_host).into_any()}
            </div>

            {start}
            <button class="link" on:click=move |_| leave(app)>
                "Leave room"
            </button>
        </div>
    }
}
