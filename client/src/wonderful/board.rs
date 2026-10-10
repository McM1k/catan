//! The board of It's a Wonderful World: the card table you draft, plan and
//! produce at, and the final standings.
//!
//! Everything here is drawn from the `View` the server sent for our seat, and
//! every click asks the server for a move (`ClientMsg::Wonderful`); the
//! server's answer is the next state, which rebuilds the screen. What a click
//! means is decided in `look`, so it can be tested natively.
//!
//! Cards are built from `span`s only, so a whole card can sit inside a
//! `<button>` (the draft) as well as in a `<div>`.

use std::sync::Arc;

use leptos::prelude::*;
use protocol::ClientMsg;
use wonderful_core::{
    Action, Bonus, Building, Card, CardId, Cost, Kind, Phase, PlayerView, Res, Token, View, DRAFT_PICKS, ROUNDS,
    WRAP_UP,
};

use super::look::{self, CubeFrom, Held, Slot};
use super::Ui;
use crate::state::{leave, send, App};

/// What every part of the board needs. Cheap to clone: the state is shared.
#[derive(Clone)]
struct Ctx {
    app: App,
    view: Arc<View>,
    names: Arc<Vec<String>>,
    connected: Arc<Vec<bool>>,
}

impl Ctx {
    fn ui(&self) -> Ui {
        self.app.wonderful
    }

    fn me(&self) -> &PlayerView {
        &self.view.players[self.view.you]
    }

    fn act(&self, action: Action) {
        send(self.app, &ClientMsg::Wonderful(action));
    }

    /// A seat's name, "You" for ourselves.
    fn who(&self, seat: usize) -> String {
        look::who(seat, &self.names, self.view.you)
    }

    /// A seat's own name, ours included.
    fn name(&self, seat: usize) -> String {
        self.names.get(seat).cloned().unwrap_or_else(|| format!("Player {}", seat + 1))
    }
}

/// The whole game screen for the state the server last sent.
pub fn board(app: App, view: View, names: Vec<String>, connected: Vec<bool>, room: String) -> impl IntoView {
    let ctx = Ctx { app, view: Arc::new(view), names: Arc::new(names), connected: Arc::new(connected) };
    let over = ctx.view.phase == Phase::Over;

    view! {
        <div class="game-board">
            {top_bar(app, room)}
            {status_band(&ctx)}
            <div class="board-grid">
                <div class="main-col">
                    {phase_panel(&ctx)}
                    {tray(&ctx)}
                    {(!over).then(|| buildings_panel(&ctx))}
                    {empire_panel(&ctx)}
                </div>
                <aside class="side-col">
                    {rivals_panel(&ctx)}
                    {log_panel(&ctx)}
                </aside>
            </div>
            {cube_picker(&ctx)}
        </div>
    }
}

// ----- small parts -------------------------------------------------------------------

/// A glossy token with a letter on it.
fn cube(class: &'static str, letter: &'static str) -> AnyView {
    view! { <span class=format!("cube {class}") aria-hidden="true">{letter}</span> }.into_any()
}

fn res_cube(res: Res) -> AnyView {
    cube(look::res_class(res), look::res_letter(res))
}

fn held_cube(held: Held) -> AnyView {
    cube(held.class(), held.letter())
}

fn bonus_cube(bonus: Bonus) -> AnyView {
    match bonus {
        Bonus::Cube(res) => res_cube(res),
        Bonus::Krystallium => cube("krystallium", "K"),
        Bonus::Token(Token::General) => cube("general", "Gen"),
        Bonus::Token(Token::Financier) => cube("financier", "Fin"),
    }
}

/// `n` of `of` round markers lit, e.g. the cubes on the Empire.
fn pips(n: u8, of: u8) -> AnyView {
    let dots = (0..of)
        .map(|i| view! { <span class={if i < n { "pip on" } else { "pip" }}></span> })
        .collect_view();
    view! { <span class="pips" role="img" aria-label=format!("{n} of {of}")>{dots}</span> }.into_any()
}

fn kind_icon(kind: Kind) -> AnyView {
    let path = match kind {
        Kind::Structure => "M2 14V6l6-4 6 4v8H9.5v-4h-3v4z",
        Kind::Vehicle => "M8 1c3 2 4.5 5 3.5 9h-7C3.5 6 5 3 8 1zm-3 10-2.5 4 3.5-1zm6 0 2.5 4-3.5-1z",
        Kind::Research => "M6 1h4v1.5h-.8v3.7l3.9 6.4a1.2 1.2 0 0 1-1 1.8H3.9a1.2 1.2 0 0 1-1-1.8l3.9-6.4V2.5H6z",
        Kind::Project => "M8 1l6 3.5v7L8 15l-6-3.5v-7zm0 4a3 3 0 1 0 0 6 3 3 0 0 0 0-6z",
        Kind::Discovery => "M8 1l1.8 5.2L15 8l-5.2 1.8L8 15l-1.8-5.2L1 8l5.2-1.8z",
    };
    view! {
        <svg class="kind-icon" viewBox="0 0 16 16" aria-hidden="true">
            <path d=path fill="currentColor" fill-rule="evenodd" />
        </svg>
    }
    .into_any()
}

/// What a seat produces each round, as cubes with a number.
fn production_line(production: [u8; 5]) -> AnyView {
    let stats = Res::ALL
        .iter()
        .map(|&res| {
            let n = production[res.index()];
            view! {
                <span class="stat" title=format!("{}: {} per round", res.name(), n)>
                    {res_cube(res)}
                    <b>{n}</b>
                </span>
            }
        })
        .collect_view();
    view! { <span class="stats production-line" role="group" aria-label="Production per round">{stats}</span> }
        .into_any()
}

// ----- cards ---------------------------------------------------------------------------

fn card_class(card: &Card, extra: &str) -> String {
    format!("card {} {extra}", look::kind_class(card.kind))
}

/// The spaces a card needs filled, drawn as the cubes it asks for.
fn requirement_sockets(cost: &Cost) -> AnyView {
    let list = look::slots(cost, &Cost::default())
        .into_iter()
        .map(|slot| {
            view! {
                <span class=format!("socket req {}", slot.space.class()) title=slot.space.name()>
                    {slot.space.letter()}
                </span>
            }
        })
        .collect_view();
    view! { <span class="sockets">{list}</span> }.into_any()
}

fn row(class: &'static str, label: &'static str, body: AnyView) -> AnyView {
    view! {
        <span class=format!("row {class}")>
            <span class="row-label">{label}</span>
            <span class="row-body">{body}</span>
        </span>
    }
    .into_any()
}

fn makes_row(card: &'static Card) -> Option<AnyView> {
    let mut icons: Vec<AnyView> = Vec::new();
    for res in Res::ALL {
        for _ in 0..card.produces[res.index()] {
            icons.push(res_cube(res));
        }
    }
    let per = card.scaled.map(|(res, kind)| {
        view! {
            <span class="per">
                {res_cube(res)}
                <span class="per-text">{format!("per {}", kind.name())}</span>
            </span>
        }
    });
    if icons.is_empty() && per.is_none() {
        return None;
    }
    Some(row("makes", "Makes", view! { {icons}{per} }.into_any()))
}

fn worth_row(card: &'static Card) -> Option<AnyView> {
    let parts = look::points_parts(card);
    if parts.is_empty() {
        return None;
    }
    Some(row("worth", "Worth", view! { <span class="vp">{parts.join(" + ")}</span> }.into_any()))
}

fn built_row(card: &'static Card) -> Option<AnyView> {
    let bonus = card.bonus?;
    let text = look::bonus_text(card).unwrap_or_default();
    Some(row(
        "built",
        "When built",
        view! {
            {bonus_cube(bonus)}
            <span class="bonus-text">{text}</span>
        }
        .into_any(),
    ))
}

/// What a card shows inside its frame. `cost` is the row of spaces (static
/// for a card in hand, live for a building); `foot` adds what recycling gives.
fn card_body(card: &'static Card, cost: Option<AnyView>, foot: bool) -> AnyView {
    let cost = cost.map(|c| view! { <span class="card-cost">{c}</span> });
    let foot = foot.then(|| {
        view! {
            <span class="card-foot">
                <span class="row-label">"Recycles to"</span>
                {res_cube(card.recycle)}
            </span>
        }
    });
    view! {
        <span class="card-head">
            {kind_icon(card.kind)}
            <span class="card-kind">{card.kind.name()}</span>
        </span>
        <span class="card-name">{card.name}</span>
        {cost}
        <span class="card-rows">{makes_row(card)}{worth_row(card)}{built_row(card)}</span>
        {foot}
    }
    .into_any()
}

/// A card that isn't a button: in the draft area, in the Empire, in a list.
fn card_static(card: &'static Card, with_cost: bool, foot: bool, extra: &str) -> AnyView {
    let cost = with_cost.then(|| requirement_sockets(&card.cost));
    let summary = look::card_summary(card);
    view! {
        <div class=card_class(card, extra) role="group" aria-label=summary.clone() title=summary>
            {card_body(card, cost, foot)}
        </div>
    }
    .into_any()
}

// ----- the status band ------------------------------------------------------------------

fn top_bar(app: App, room: String) -> AnyView {
    view! {
        <header class="top-bar">
            <h1 class="brand">"It's a Wonderful World"</h1>
            <span class="room">"Room " <b class="room-tag">{room}</b></span>
            <button class="link" on:click=move |_| leave(app)>
                "Leave game"
            </button>
        </header>
    }
    .into_any()
}

fn status_band(ctx: &Ctx) -> AnyView {
    let view = &ctx.view;
    let waiting = look::waiting_on(view);
    let over = view.phase == Phase::Over;
    let seats = view
        .players
        .iter()
        .enumerate()
        .map(|(seat, _)| {
            let done = !waiting.contains(&seat);
            view! {
                <li class={if done { "seat done" } else { "seat" }}>
                    <span class="dot"></span>
                    {ctx.who(seat)}
                    <span class="sr">{if done { " is done" } else { " is deciding" }}</span>
                </li>
            }
        })
        .collect_view();

    view! {
        <section class="status" aria-live="polite">
            <div class="round" aria-label=format!("Round {} of {}", view.round, ROUNDS)>
                <span class="round-word">"Round"</span>
                <span class="round-num">{view.round}</span>
                <span class="round-of">{format!("of {ROUNDS}")}</span>
            </div>
            <div class="phase">
                <h2>{look::phase_title(view)}</h2>
                <p class="hint">{look::hint(view, &ctx.names)}</p>
            </div>
            {(!over).then(|| view! { <ul class="seats">{seats}</ul> })}
        </section>
    }
    .into_any()
}

fn phase_panel(ctx: &Ctx) -> AnyView {
    match ctx.view.phase {
        Phase::Draft => draft_panel(ctx),
        Phase::Planning => planning_panel(ctx),
        Phase::Production { step } => production_panel(ctx, step),
        Phase::Over => results_panel(ctx),
    }
}

// ----- draft ------------------------------------------------------------------------------

fn draft_panel(ctx: &Ctx) -> AnyView {
    let view = &ctx.view;
    let picked = view.picked;

    let pick = picked.map(|card| {
        view! {
            <div class="your-pick">{card_static(card.def(), true, false, "picked")}</div>
        }
    });

    let hand = view
        .hand
        .iter()
        .map(|&card| {
            let ctx = ctx.clone();
            let def = card.def();
            let summary = look::card_summary(def);
            view! {
                <li>
                    <button
                        class=card_class(def, if picked.is_some() { "pickable passing" } else { "pickable" })
                        disabled=picked.is_some()
                        aria-label=summary.clone()
                        title=summary
                        on:click=move |_| ctx.act(Action::Draft { card })
                    >
                        {card_body(def, Some(requirement_sockets(&def.cost)), true)}
                    </button>
                </li>
            }
        })
        .collect_view();

    let passing = (picked.is_some() && !view.hand.is_empty()).then(|| {
        let to = ctx.name(look::pass_to(view));
        view! { <p class="caption">{format!("Passing on to {to}")}</p> }
    });

    let kept = (!view.drafted.is_empty()).then(|| {
        let cards = view
            .drafted
            .iter()
            .map(|&card| view! { <li>{card_static(card.def(), false, false, "")}</li> })
            .collect_view();
        view! {
            <h3 class="subhead">
                "Kept so far"
                <span class="count">{format!("{} of {}", view.drafted.len(), DRAFT_PICKS)}</span>
            </h3>
            <ul class="cards kept">{cards}</ul>
        }
    });

    view! {
        <section class="panel draft">
            <h2>{if picked.is_some() { "Your pick is in" } else { "Pick a card" }}</h2>
            {pick}
            {passing}
            <ul class="cards hand">{hand}</ul>
            {kept}
        </section>
    }
    .into_any()
}

// ----- planning ---------------------------------------------------------------------------

fn planning_panel(ctx: &Ctx) -> AnyView {
    let view = &ctx.view;
    let me = ctx.me();
    let left = view.drafted.len();

    let cards = view
        .drafted
        .iter()
        .map(|&card| {
            let def = card.def();
            let build = {
                let ctx = ctx.clone();
                move |_| ctx.act(Action::Build { card })
            };
            let recycle = {
                let ui = ctx.ui();
                move |_| ui.cube_from.set(Some(CubeFrom::Recycle(card)))
            };
            view! {
                <li class="plan-card">
                    {card_static(def, true, false, "")}
                    <div class="plan-actions">
                        <button class="primary" on:click=build>
                            "Build"
                        </button>
                        <button on:click=recycle>"Recycle for " {res_cube(def.recycle)}</button>
                    </div>
                </li>
            }
        })
        .collect_view();

    let done = {
        let ctx = ctx.clone();
        move |_| ctx.act(Action::Ready)
    };
    let label = if me.ready { "Waiting for the others…" } else { "Done planning" };

    view! {
        <section class="panel planning">
            <h2>
                {if left > 0 {
                    format!("Plan your {left} card{}", if left == 1 { "" } else { "s" })
                } else {
                    "Planning".to_string()
                }}
            </h2>
            {(left > 0).then(|| view! { <ul class="cards plan">{cards}</ul> })}
            <div class="panel-actions">
                <button class="primary" disabled={left > 0 || me.ready} on:click=done>
                    {label}
                </button>
            </div>
        </section>
    }
    .into_any()
}

// ----- production -------------------------------------------------------------------------

fn production_panel(ctx: &Ctx, step: u8) -> AnyView {
    let me = ctx.me();
    let steps = (0..=WRAP_UP)
        .map(|i| {
            let class = if i < step {
                "step done"
            } else if i == step {
                "step now"
            } else {
                "step"
            };
            let (icon, name) = match Res::ALL.get(i as usize) {
                Some(&res) => (res_cube(res), res.name()),
                None => (cube("wrap", "+"), "Wrap-up"),
            };
            view! {
                <li class=class aria-current={if i == step { "step" } else { "false" }}>
                    {icon}
                    <span class="step-name">{name}</span>
                </li>
            }
        })
        .collect_view();

    let choose = me.choose.then(|| {
        let take = |token: Token| {
            let ctx = ctx.clone();
            move |_| ctx.act(Action::Choose { token })
        };
        view! {
            <div class="choose">
                <p>"You produced the most Science: take a character."</p>
                <div class="choose-buttons">
                    <button class="primary" on:click=take(Token::General)>
                        "Take a General"
                    </button>
                    <button class="primary" on:click=take(Token::Financier)>
                        "Take a Financier"
                    </button>
                </div>
            </div>
        }
    });

    view! {
        <section class="panel production">
            <ol class="steps">{steps}</ol>
            {race(ctx, step)}
            {choose}
        </section>
    }
    .into_any()
}

/// Who produced how much in this step, and who takes the character.
fn race(ctx: &Ctx, step: u8) -> Option<AnyView> {
    let res = *Res::ALL.get(step as usize)?;
    let view = &ctx.view;
    let most = view.players.iter().map(|p| p.produced).max().unwrap_or(0);
    if most == 0 {
        return None;
    }
    let leader = look::supremacy(view);
    let prize = match res.supremacy_token() {
        Some(token) => format!("takes a {}", token.name()),
        None => "chooses a character".to_string(),
    };
    let rows = view
        .players
        .iter()
        .enumerate()
        .map(|(seat, p)| {
            let lead = leader == Some(seat);
            let width = u32::from(p.produced) * 100 / u32::from(most);
            view! {
                <li class={if lead { "race-row lead" } else { "race-row" }}>
                    <span class="race-name">{ctx.who(seat)}</span>
                    <span class="race-bar" style=format!("--w:{width}%")></span>
                    <span class="race-n">{p.produced}</span>
                    {lead.then(|| view! { <span class="race-prize">{prize.clone()}</span> })}
                </li>
            }
        })
        .collect_view();
    let tied = leader.is_none();
    Some(
        view! {
            <div class="race">
                <h3 class="subhead">
                    {res_cube(res)}
                    {format!("{} produced", res.name())}
                    {tied.then(|| view! { <span class="count">"tied: no character"</span> })}
                </h3>
                <ul class="race-list">{rows}</ul>
            </div>
        }
        .into_any(),
    )
}

// ----- the pieces you hold ------------------------------------------------------------------

fn piece_button(ctx: &Ctx, held: Held, n: u8) -> AnyView {
    let ui = ctx.ui();
    let selected = {
        let ctx = ctx.clone();
        move || look::effective_held(&ctx.view, ctx.ui().held.get()) == Some(held)
    };
    let class = {
        let selected = selected.clone();
        move || if selected() { "piece selected" } else { "piece" }
    };
    view! {
        <button
            class=class
            aria-pressed=move || selected().to_string()
            title=held.name()
            on:click=move |_| ui.held.set(Some(held))
        >
            {held_cube(held)}
            <span class="count">{format!("×{n}")}</span>
            <span class="piece-name">{held.name()}</span>
        </button>
    }
    .into_any()
}

/// What you can place, the Empire as a place for cubes, and (in production)
/// the button that ends your step. Sticks to the bottom of the screen while
/// you scroll through your buildings.
fn tray(ctx: &Ctx) -> AnyView {
    let view = &ctx.view;
    let in_production = matches!(view.phase, Phase::Production { .. });
    if !in_production && view.phase != Phase::Planning {
        return ().into_any();
    }
    let have = look::available(view);
    if have.is_empty() && !in_production {
        return ().into_any();
    }
    let me = ctx.me();
    let has_cubes = have.iter().any(|(h, _)| matches!(h, Held::Cube(_)));
    let pieces = have.iter().map(|&(held, n)| piece_button(ctx, held, n)).collect_view();

    let empire = has_cubes.then(|| {
        let can = {
            let ctx = ctx.clone();
            move || look::pick_for_empire(&ctx.view, ctx.ui().held.get()).is_some()
        };
        let click = {
            let ctx = ctx.clone();
            move |_| {
                let chosen = ctx.ui().held.get_untracked();
                if let Some(action) = look::pick_for_empire(&ctx.view, chosen).and_then(look::empire_action) {
                    ctx.act(action);
                }
            }
        };
        view! {
            <button class="empire-target" disabled=move || !can() on:click=click>
                <span class="target-name">"Put on your Empire"</span>
                {pips(me.empire_cubes, 5)}
            </button>
        }
    });

    let done = in_production.then(|| {
        let ctx2 = ctx.clone();
        let click = move |_| ctx2.act(Action::Ready);
        let (label, disabled) = if me.ready {
            ("Waiting for the others…".to_string(), true)
        } else if me.choose {
            ("Take a character first".to_string(), true)
        } else if me.pool > 0 {
            (format!("Done: drop {} cube{}", me.pool, if me.pool == 1 { "" } else { "s" }), false)
        } else {
            ("Done".to_string(), false)
        };
        view! {
            <button class="primary" disabled=disabled on:click=click>
                {label}
            </button>
        }
    });

    view! {
        <section class="panel tray">
            <div class="tray-pieces" role="group" aria-label="Pieces you can place">
                {if have.is_empty() {
                    view! { <span class="empty">"Nothing to place right now."</span> }.into_any()
                } else {
                    pieces.into_any()
                }}
            </div>
            <div class="tray-actions">{empire}{done}</div>
        </section>
    }
    .into_any()
}

// ----- your buildings ------------------------------------------------------------------------

/// One space of a building: a free one lights up when what you hold fits.
fn live_socket(ctx: &Ctx, card: CardId, slot: Slot) -> AnyView {
    let space = slot.space;
    if slot.filled {
        return view! {
            <span class=format!("socket filled {}", space.class()) title=format!("{} placed", space.name())>
                {space.letter()}
            </span>
        }
        .into_any();
    }
    let class = {
        let ctx = ctx.clone();
        move || {
            let fits = look::pick_for_space(&ctx.view, ctx.ui().held.get(), space).is_some();
            format!("socket free {}{}", space.class(), if fits { " fits" } else { "" })
        }
    };
    let disabled = {
        let ctx = ctx.clone();
        move || look::pick_for_space(&ctx.view, ctx.ui().held.get(), space).is_none()
    };
    let click = {
        let ctx = ctx.clone();
        move |_| {
            let chosen = ctx.ui().held.get_untracked();
            let action = look::pick_for_space(&ctx.view, chosen, space)
                .and_then(|held| look::slot_action(held, space, card));
            if let Some(action) = action {
                ctx.act(action);
            }
        }
    };
    view! {
        <button
            class=class
            disabled=disabled
            on:click=click
            aria-label=format!("Free {} space on {}", space.name(), card.def().name)
        >
            {space.letter()}
        </button>
    }
    .into_any()
}

fn building_card(ctx: &Ctx, building: &Building) -> AnyView {
    let card = building.card;
    let def = card.def();
    let (placed, needed) = look::progress(&def.cost, &building.filled);
    let sockets = look::slots(&def.cost, &building.filled)
        .into_iter()
        .map(|slot| live_socket(ctx, card, slot))
        .collect_view();
    let scrap = {
        let ui = ctx.ui();
        move |_| ui.cube_from.set(Some(CubeFrom::Scrap(card)))
    };
    let summary = look::card_summary(def);
    view! {
        <li>
            <div class=card_class(def, "building") role="group" aria-label=summary.clone() title=summary>
                {card_body(def, Some(view! { <span class="sockets">{sockets}</span> }.into_any()), false)}
                <span class="card-foot">
                    <span class="progress">{format!("{placed} of {needed} placed")}</span>
                    <button class="link" on:click=scrap>
                        "Scrap"
                    </button>
                </span>
            </div>
        </li>
    }
    .into_any()
}

fn buildings_panel(ctx: &Ctx) -> AnyView {
    let me = ctx.me();
    let cards = me.buildings.iter().map(|b| building_card(ctx, b)).collect_view();
    view! {
        <section class="panel buildings">
            <h2>"Under construction"</h2>
            {if me.buildings.is_empty() {
                view! {
                    <p class="empty">
                        "Nothing yet. Cards you build in the planning phase wait here for cubes."
                    </p>
                }
                .into_any()
            } else {
                view! { <ul class="cards building-list">{cards}</ul> }.into_any()
            }}
        </section>
    }
    .into_any()
}

// ----- your Empire ------------------------------------------------------------------------------

fn stock_line(p: &PlayerView) -> AnyView {
    view! {
        <span class="stats stock-line" role="group" aria-label="What you hold">
            <span class="stat" title="Cubes on the Empire: five make a Krystallium">
                "Empire"
                {pips(p.empire_cubes, 5)}
            </span>
            <span class="stat" title="Krystallium">
                {cube("krystallium", "K")}
                <b>{p.krystallium}</b>
            </span>
            <span class="stat" title="Generals">
                {cube("general", "Gen")}
                <b>{p.generals}</b>
            </span>
            <span class="stat" title="Financiers">
                {cube("financier", "Fin")}
                <b>{p.financiers}</b>
            </span>
        </span>
    }
    .into_any()
}

fn points(p: &PlayerView) -> AnyView {
    view! {
        <div class="points">
            <span class="points-num">{p.points}</span>
            <span class="points-word">{if p.points == 1 { "point" } else { "points" }}</span>
        </div>
    }
    .into_any()
}

fn empire_panel(ctx: &Ctx) -> AnyView {
    let me = ctx.me();
    let counts = look::kind_counts(&me.empire);
    let columns = Kind::ALL
        .iter()
        .zip(counts)
        .filter(|(_, n)| *n > 0)
        .map(|(&kind, n)| {
            let cards = me
                .empire
                .iter()
                .filter(|id| id.def().kind == kind)
                .map(|id| view! { <li>{card_static(id.def(), false, false, "")}</li> })
                .collect_view();
            view! {
                <section class="kind-col">
                    <h3 class="subhead">
                        {kind_icon(kind)}
                        {kind.name()}
                        <span class="count">{n}</span>
                    </h3>
                    <ul class="cards finished">{cards}</ul>
                </section>
            }
        })
        .collect_view();

    view! {
        <section class="panel empire">
            <div class="empire-head">
                <div>
                    <h2>"Your Empire"</h2>
                    <p class="empire-name">{look::empire_name(ctx.view.you)}</p>
                </div>
                {points(me)}
            </div>
            <div class="empire-stats">
                <div class="stat-block">
                    <span class="row-label">"Produces each round"</span>
                    {production_line(me.production)}
                </div>
                <div class="stat-block">
                    <span class="row-label">"You hold"</span>
                    {stock_line(me)}
                </div>
            </div>
            {if me.empire.is_empty() {
                view! { <p class="empty">"No finished cards yet. Fill every space of a building to finish it."</p> }
                    .into_any()
            } else {
                view! { <div class="kinds">{columns}</div> }.into_any()
            }}
        </section>
    }
    .into_any()
}

// ----- the other players ---------------------------------------------------------------------------

fn mini_building(b: &Building) -> AnyView {
    let def = b.card.def();
    let (placed, needed) = look::progress(&def.cost, &b.filled);
    let pct = (placed * 100).checked_div(needed).unwrap_or(100);
    view! {
        <li class=format!("mini {}", look::kind_class(def.kind)) title=look::card_summary(def)>
            <span class="mini-name">{def.name}</span>
            <span class="mini-progress" style=format!("--done:{pct}%")></span>
            <span class="mini-count">{format!("{placed}/{needed}")}</span>
        </li>
    }
    .into_any()
}

fn rival(ctx: &Ctx, seat: usize) -> AnyView {
    let p = &ctx.view.players[seat];
    let offline = ctx.connected.get(seat).is_some_and(|c| !c);
    let waiting = look::waiting_on(&ctx.view).contains(&seat);
    let over = ctx.view.phase == Phase::Over;
    let status = if over {
        ""
    } else if waiting {
        "deciding"
    } else {
        "done"
    };
    let buildings = p.buildings.iter().map(mini_building).collect_view();
    let chips = p
        .empire
        .iter()
        .map(|id| {
            let def = id.def();
            view! { <li class=format!("chip {}", look::kind_class(def.kind)) title=look::card_summary(def)>{def.name}</li> }
        })
        .collect_view();
    view! {
        <li class="rival">
            <div class="rival-head">
                <div>
                    <h3>
                        {ctx.name(seat)}
                        {offline.then(|| view! { <span class="tag tag-off">"offline"</span> })}
                    </h3>
                    <p class="empire-name">{look::empire_name(seat)}</p>
                </div>
                {points(p)}
            </div>
            {(!status.is_empty()).then(|| view! { <p class={format!("rival-status {status}")}>{status}</p> })}
            {production_line(p.production)}
            {stock_line(p)}
            {(!p.buildings.is_empty()).then(|| view! { <ul class="minis">{buildings}</ul> })}
            {(!p.empire.is_empty()).then(|| view! { <ul class="chips">{chips}</ul> })}
        </li>
    }
    .into_any()
}

fn rivals_panel(ctx: &Ctx) -> AnyView {
    let rows = (0..ctx.view.players.len())
        .filter(|&seat| seat != ctx.view.you)
        .map(|seat| rival(ctx, seat))
        .collect_view();
    view! {
        <section class="panel rivals">
            <h2>"Other players"</h2>
            <ul class="rival-list">{rows}</ul>
        </section>
    }
    .into_any()
}

fn log_panel(ctx: &Ctx) -> AnyView {
    let lines = ctx
        .view
        .log
        .iter()
        .rev()
        .take(12)
        .map(|event| view! { <li>{look::describe_event(event, &ctx.names, ctx.view.you)}</li> })
        .collect_view();
    view! {
        <section class="panel log-panel">
            <h2>"What happened"</h2>
            {if ctx.view.log.is_empty() {
                view! { <p class="empty">"Nothing yet."</p> }.into_any()
            } else {
                view! { <ol class="log">{lines}</ol> }.into_any()
            }}
        </section>
    }
    .into_any()
}

// ----- where a recycled cube goes ---------------------------------------------------------------------

/// The dialog that asks where the cube of a recycled card or a scrapped
/// building goes (see `Ui::cube_from`).
fn cube_picker(ctx: &Ctx) -> AnyView {
    let ctx = ctx.clone();
    (move || {
        let ui = ctx.ui();
        let from = ui.cube_from.get().filter(|from| from.valid(&ctx.view))?;
        let res = from.res();
        let def = from.card().def();
        let scrapping = matches!(from, CubeFrom::Scrap(_));
        let title = if scrapping { format!("Scrap {}", def.name) } else { format!("Recycle {}", def.name) };

        let targets = look::cube_targets(&ctx.view, from)
            .into_iter()
            .map(|to| {
                let detail = match to {
                    wonderful_core::Target::Empire => format!("{} of 5 cubes", ctx.me().empire_cubes),
                    wonderful_core::Target::Card(id) => {
                        let left = ctx.me().buildings.iter().find(|b| b.card == id).map_or(0, |b| b.remaining().res[res.index()]);
                        format!("needs {left} more {}", res.name())
                    }
                };
                let click = {
                    let ctx = ctx.clone();
                    move |_| {
                        ctx.act(from.action(to));
                        ctx.ui().cube_from.set(None);
                    }
                };
                view! {
                    <li>
                        <button on:click=click>
                            <span class="target-name">{look::target_name(to)}</span>
                            <span class="target-detail">{detail}</span>
                        </button>
                    </li>
                }
            })
            .collect_view();

        Some(
            view! {
                <div class="modal-bg">
                    <div class="modal panel" role="dialog" aria-modal="true" aria-labelledby="cube-picker-title">
                        <h2 id="cube-picker-title">{title}</h2>
                        <p class="modal-text">
                            "It gives you a cube: " {res_cube(res)} {format!(" {}. Where does it go?", res.name())}
                        </p>
                        {scrapping
                            .then(|| view! { <p class="warning">"Everything placed on this building is lost."</p> })}
                        <ul class="targets">{targets}</ul>
                        <button class="link" on:click=move |_| ui.cube_from.set(None)>
                            "Cancel"
                        </button>
                    </div>
                </div>
            }
            .into_any(),
        )
    })
    .into_any()
}

// ----- the end of the game ----------------------------------------------------------------------------

fn results_panel(ctx: &Ctx) -> AnyView {
    let app = ctx.app;
    let you = ctx.view.you;
    let rows = look::results(&ctx.view, &ctx.names)
        .into_iter()
        .map(|r| {
            view! {
                <tr class={if r.winner { "winner" } else { "" }}>
                    <th scope="row">
                        <span class="rank-name">{r.name.clone()}</span>
                        {(r.seat == you).then(|| view! { <span class="tag">"you"</span> })}
                        {r.winner.then(|| view! { <span class="tag tag-win">"winner"</span> })}
                    </th>
                    <td data-label="Card points">{r.score.gross}</td>
                    <td data-label="Bonuses">{r.score.combo}</td>
                    <td data-label="Generals">{r.score.generals}</td>
                    <td data-label="Financiers">{r.score.financiers}</td>
                    <td class="total">{r.score.total}</td>
                    <td class="cards-done" data-label="Finished cards">{r.score.cards}</td>
                </tr>
            }
        })
        .collect_view();
    view! {
        <section class="panel results">
            <h2 class="verdict">{look::verdict(&ctx.view, &ctx.names)}</h2>
            <div class="table-wrap">
                <table class="standings">
                    <thead>
                        <tr>
                            <th scope="col">"Player"</th>
                            <th scope="col">"Card points"</th>
                            <th scope="col">"Bonuses"</th>
                            <th scope="col">"Generals"</th>
                            <th scope="col">"Financiers"</th>
                            <th scope="col">"Total"</th>
                            <th scope="col">"Finished cards"</th>
                        </tr>
                    </thead>
                    <tbody>{rows}</tbody>
                </table>
            </div>
            <p class="note">
                "Bonuses are the points that depend on card types and characters. A tie goes to whoever finished the most cards, then to whoever holds the most characters."
            </p>
            <div class="panel-actions">
                <button class="primary" on:click=move |_| leave(app)>
                    "Leave room"
                </button>
            </div>
        </section>
    }
    .into_any()
}

#[cfg(test)]
mod tests {
    use super::*;
    use leptos::tachys::view::RenderHtml;
    use wonderful_core::{catalogue, State};

    const NAMES: [&str; 5] = ["Ann", "Bob", "Cy", "Di", "Ed"];

    /// Runs `f` inside a fresh reactive owner, with an executor registered so
    /// that creating effects doesn't panic (the tasks are never polled).
    fn in_owner<R>(f: impl FnOnce() -> R) -> R {
        let _ = any_spawner::Executor::init_futures_executor();
        Owner::new().with(f)
    }

    fn names(n: usize) -> Vec<String> {
        NAMES[..n].iter().map(|s| s.to_string()).collect()
    }

    /// The board for `seat` as the browser would first paint it.
    fn render_with(app: App, state: &State, seat: usize) -> String {
        let n = state.seats();
        board(app, state.view_for(seat), names(n), vec![true; n], "ABCD".into()).to_html()
    }

    fn render(state: &State, seat: usize) -> String {
        in_owner(|| render_with(App::new(), state, seat))
    }

    fn id(name: &str) -> CardId {
        CardId(catalogue().iter().position(|c| c.name == name).unwrap() as u16)
    }

    /// What a simple player would do next, if anything: the game is played
    /// to its end with these moves so that every state gets drawn.
    fn next_move(state: &State, seat: usize) -> Option<Action> {
        let p = &state.players[seat];
        match state.phase {
            Phase::Draft => p.picked.is_none().then(|| Action::Draft { card: p.hand[0] }),
            Phase::Planning => match p.drafted.first() {
                Some(&card) if p.buildings.len() < 3 => Some(Action::Build { card }),
                Some(&card) => Some(Action::Recycle { card, to: wonderful_core::Target::Empire }),
                None => (!p.ready).then_some(Action::Ready),
            },
            Phase::Production { step } => {
                if p.choose {
                    return Some(Action::Choose { token: Token::General });
                }
                let step_res = Res::ALL.get(step as usize).copied();
                if let Some(res) = step_res {
                    if p.pool > 0 {
                        let to = p
                            .buildings
                            .iter()
                            .find(|b| b.remaining().res[res.index()] > 0)
                            .map_or(wonderful_core::Target::Empire, |b| wonderful_core::Target::Card(b.card));
                        return Some(Action::Place { piece: wonderful_core::Piece::Cube(res), target: to });
                    }
                }
                (!p.ready).then_some(Action::Ready)
            }
            Phase::Over => None,
        }
    }

    /// Plays on until nobody has anything left to do, drawing the board for
    /// every seat after each move (or only when the phase changes, if `sparse`).
    fn play_and_draw(state: &mut State, sparse: bool, mut check: impl FnMut(&State, usize, &str)) -> usize {
        let n = state.seats();
        let mut moves = 0;
        let mut seen = (state.round, state.phase);
        for seat in 0..n {
            check(state, seat, &render(state, seat));
        }
        loop {
            let mut moved = false;
            for seat in 0..n {
                if let Some(action) = next_move(state, seat) {
                    state.apply(seat, action.clone()).unwrap_or_else(|e| panic!("{action:?}: {e:?}"));
                    moved = true;
                    moves += 1;
                    let now = (state.round, state.phase);
                    if !sparse || now != seen {
                        for s in 0..n {
                            check(state, s, &render(state, s));
                        }
                    }
                    seen = now;
                }
            }
            assert!(moves < 5000, "the game doesn't end");
            if !moved {
                break;
            }
        }
        moves
    }

    #[test]
    fn a_whole_game_can_be_drawn_from_every_seat_at_every_step() {
        for players in 2..=5 {
            let mut state = State::new(players, 40 + players as u64);
            let moves = play_and_draw(&mut state, players != 3, |state, seat, html| {
                assert!(html.contains("ABCD") && html.contains("Leave game"), "{:?}", state.phase);
                assert!(html.contains("It&#39;s a Wonderful World") || html.contains("It's a Wonderful World"));
                assert!(html.contains(&format!("{}", state.round)));
                assert!(html.contains("Your Empire") && html.contains(look::empire_name(seat)));
                // Every other player is on the table; you are "You", not a name.
                for (other, name) in NAMES.iter().enumerate().take(state.seats()) {
                    let listed = html.contains(&format!("<h3>{name}"));
                    assert_eq!(listed, other != seat, "{name} seen from {seat}");
                }
                // (The list of who is still deciding goes away once the game is over.)
                assert_eq!(html.contains("</span>You<span class=\"sr\">"), state.phase != Phase::Over);
                match state.phase {
                    Phase::Draft if state.players[seat].picked.is_none() => {
                        assert!(html.contains("Pick a card") && html.contains("pickable"));
                    }
                    Phase::Draft => assert!(html.contains("Your pick is in")),
                    Phase::Planning => assert!(html.contains("Done planning") || html.contains("Waiting for the others")),
                    Phase::Production { .. } => assert!(html.contains("Production:")),
                    Phase::Over => assert!(html.contains("Leave room") && html.contains("standings")),
                }
            });
            assert!(moves > 100, "{players} players: only {moves} moves");
            assert_eq!(state.phase, Phase::Over, "{players} players");
        }
    }

    #[test]
    fn the_final_standings_name_the_winner_and_every_score() {
        let mut state = State::new(3, 9);
        play_and_draw(&mut state, true, |_, _, _| {});
        assert_eq!(state.phase, Phase::Over);
        for seat in 0..3 {
            let html = render(&state, seat);
            let verdict = look::verdict(&state.view_for(seat), &names(3));
            assert!(html.contains(&verdict.replace('\'', "&#39;")) || html.contains(&verdict), "{verdict}");
            assert!(html.contains("Game over"));
            for s in 0..3 {
                assert!(html.contains(&format!("<td class=\"total\">{}</td>", state.scores[s].total)));
            }
            // One winner mark per winner, and nothing left to build.
            assert_eq!(html.matches("tag tag-win").count(), state.winners().len());
            assert!(!html.contains("Under construction"));
            assert!(html.contains("Leave room"));
        }
    }

    #[test]
    fn the_draft_shows_your_hand_then_your_pick() {
        let mut state = State::new(3, 5);
        let hand = state.players[0].hand.clone();
        let before = render(&state, 0);
        assert!(before.contains("Pick a card") && !before.contains("Your pick is in"));
        assert_eq!(before.matches("class=\"card kind-").count(), hand.len(), "one pickable card each");
        assert!(before.contains("Pick a card (1 of 7)"));
        assert!(!before.contains("Kept so far"));
        for &card in &hand {
            assert!(before.contains(card.def().name), "{}", card.def().name);
        }

        state.apply(0, Action::Draft { card: hand[2] }).unwrap();
        let after = render(&state, 0);
        assert!(after.contains("Your pick is in") && after.contains("Your pick"));
        assert!(after.contains("Passing on to Bob") && after.contains("Waiting for Bob and Cy"));
        assert!(after.contains("disabled"), "the others can't be picked any more");
        // The picked card shows once, apart from the rest.
        assert_eq!(after.matches(" picked\"").count(), 1);

        // Bob hasn't picked: his own view still offers a choice and hides nothing of Ann's pick.
        let bob = render(&state, 1);
        assert!(bob.contains("Pick a card") && !bob.contains("Your pick is in"));
        assert!(bob.contains("Ann"));
    }

    #[test]
    fn planning_offers_build_or_recycle_for_each_card() {
        let mut state = State::new(3, 6);
        while state.phase == Phase::Draft {
            for seat in 0..3 {
                if let Some(a) = next_move(&state, seat) {
                    state.apply(seat, a).unwrap();
                }
            }
        }
        assert_eq!(state.phase, Phase::Planning);
        let html = render(&state, 0);
        assert!(html.contains("Plan your 7 cards"));
        assert_eq!(html.matches(">Build<").count(), 7);
        assert_eq!(html.matches("Recycle for ").count(), 7);
        assert!(html.contains("Done planning") && html.contains("disabled"), "not yet");
        assert!(html.contains("Nothing yet."), "no building yet");

        let card = state.players[0].drafted[0];
        state.apply(0, Action::Build { card }).unwrap();
        let html = render(&state, 0);
        assert!(html.contains("Plan your 6 cards") && !html.contains("Nothing yet."));
        assert!(html.contains("Under construction") && html.contains("0 of "));
        assert!(html.contains("Scrap"));

        // Everything decided: just the button.
        for card in state.players[0].drafted.clone() {
            state.apply(0, Action::Build { card }).unwrap();
        }
        let html = render(&state, 0);
        assert!(!html.contains("Plan your") && html.contains("Done planning"));
        assert!(!html.contains(">Build<"));
        state.apply(0, Action::Ready).unwrap();
        let html = render(&state, 0);
        assert!(html.contains("Waiting for the others"));
    }

    /// Seat 0 in the production phase of a game in which everybody has
    /// built what was drafted.
    fn production_state() -> State {
        let mut state = State::new(3, 11);
        for _ in 0..2000 {
            if matches!(state.phase, Phase::Production { .. }) {
                break;
            }
            for seat in 0..3 {
                if state.phase == Phase::Planning {
                    for card in state.players[seat].drafted.clone() {
                        state.apply(seat, Action::Build { card }).unwrap();
                    }
                    if state.phase == Phase::Planning && !state.players[seat].ready {
                        state.apply(seat, Action::Ready).unwrap();
                    }
                } else if let Some(a) = next_move(&state, seat) {
                    state.apply(seat, a).unwrap();
                }
            }
        }
        assert!(matches!(state.phase, Phase::Production { step: 0 }));
        state
    }

    #[test]
    fn production_shows_the_step_the_race_and_what_to_place() {
        let mut state = production_state();
        // Seat 2 (Solar Concord) also makes a Materials cube at the start of
        // the game: take it away, so that seat 0 is alone in front.
        state.players[2].produced = 0;
        state.players[2].pool = 0;
        state.players[2].ready = true;
        let html = render(&state, 0);
        assert!(html.contains("Production: Materials"));
        for step in ["Materials", "Energy", "Science", "Gold", "Exploration", "Wrap-up"] {
            assert!(html.contains(step), "{step}");
        }
        assert!(html.contains("step now") && html.contains("aria-current=\"step\""));
        // Seat 0 (Aurelian Union) makes a Materials cube and nobody else does:
        // it leads the race and takes a General.
        // (The log below says "...tied: nobody takes a General" for earlier
        // races, so the prize is looked for in the race itself.)
        assert!(html.contains("Materials produced") && html.contains("race-row lead"));
        assert!(html.contains("class=\"race-prize\">takes a General<"));
        assert!(html.contains("Put on your Empire") && html.contains("Done: drop 1 cube"));
        assert!(html.contains("piece selected"), "the first piece is the one used");
        assert!(html.contains("Materials cube"));
        assert!(!html.contains("tied: no character"));

        // Level with someone else: nobody takes the character.
        state.players[2].produced = state.players[0].produced;
        let tied = render(&state, 0);
        assert!(tied.contains("tied: no character") && !tied.contains("race-row lead") && !tied.contains("race-prize"));

        // Seat 1 produces nothing in that step: nothing to place, nothing to drop.
        let idle = render(&state, 1);
        assert!(idle.contains("Nothing to place right now."));
        assert!(idle.contains("Waiting for the others"));
        assert!(!idle.contains("Put on your Empire"));
    }

    #[test]
    fn free_spaces_light_up_for_what_you_hold() {
        let mut state = production_state();
        // Give seat 0 a building that wants Energy and Gold, and some pieces.
        let card = id("Smelter"); // 3 Materials, 1 Energy
        state.players[0].buildings = vec![wonderful_core::Building { card, filled: Cost::default() }];
        state.players[0].pool = 2;
        state.players[0].ready = false;
        state.players[0].krystallium = 1;

        let html = in_owner(|| render_with(App::new(), &state, 0));
        // Two cubes of Materials: the Materials spaces are the only ones that fit.
        assert_eq!(html.matches("socket free res-materials fits").count(), 3, "{html}");
        assert_eq!(html.matches("socket free res-energy fits").count(), 0);
        assert_eq!(html.matches("socket free res-energy\"").count(), 1);
        assert!(html.contains("Krystallium") && html.contains("piece-name"));

        // With the Krystallium picked, it fits every space.
        let html = in_owner(|| {
            let app = App::new();
            app.wonderful.held.set(Some(Held::Krystallium));
            render_with(app, &state, 0)
        });
        assert_eq!(html.matches("socket free res-energy fits").count(), 1);
        assert_eq!(html.matches("socket free res-materials fits").count(), 3);
        assert!(html.contains("aria-pressed=\"true\""));
        // ...but it doesn't go on the Empire.
        assert!(html.contains("Put on your Empire"));
    }

    #[test]
    fn a_character_space_takes_a_character_and_the_science_winner_chooses() {
        let mut state = production_state();
        let card = id("Bastion"); // 3 Materials, 1 General
        state.players[0].buildings = vec![wonderful_core::Building { card, filled: Cost::default() }];
        state.players[0].generals = 1;
        state.players[0].pool = 0;
        state.players[0].ready = true;
        let html = render(&state, 0);
        assert!(html.contains("socket free general fits"), "{html}");
        assert!(html.contains("1 General") || html.contains("General"));

        state.players[0].choose = true;
        state.players[0].ready = false;
        let html = render(&state, 0);
        assert!(html.contains("Take a General") && html.contains("Take a Financier"));
        assert!(html.contains("Take a character first"));
        assert!(html.contains("disabled"));
        assert!(!render(&state, 1).contains("Take a General"));
    }

    #[test]
    fn the_recycle_dialog_asks_where_the_cube_goes() {
        let mut state = State::new(3, 6);
        while state.phase == Phase::Draft {
            for seat in 0..3 {
                if let Some(a) = next_move(&state, seat) {
                    state.apply(seat, a).unwrap();
                }
            }
        }
        let card = state.players[0].drafted[1];
        let built = state.players[0].drafted[0];
        state.apply(0, Action::Build { card: built }).unwrap();

        // Nothing is asked until a card is chosen.
        assert!(!render(&state, 0).contains("modal-bg"));

        let html = in_owner(|| {
            let app = App::new();
            app.wonderful.cube_from.set(Some(CubeFrom::Recycle(card)));
            render_with(app, &state, 0)
        });
        assert!(html.contains("modal-bg") && html.contains("role=\"dialog\""));
        assert!(html.contains(&format!("Recycle {}", card.def().name)) || html.contains("Recycle"));
        assert!(html.contains("your Empire") && html.contains("0 of 5 cubes"));
        assert!(html.contains("Cancel") && !html.contains("is lost"));

        // Scrapping warns about what is lost and doesn't offer the building itself.
        let html = in_owner(|| {
            let app = App::new();
            app.wonderful.cube_from.set(Some(CubeFrom::Scrap(built)));
            render_with(app, &state, 0)
        });
        assert!(html.contains("Everything placed on this building is lost."));
        assert!(html.contains(&format!("Scrap {}", built.def().name)));
        assert_eq!(html.matches("target-detail").count(), 1, "only the Empire");

        // A choice that no longer applies (the card is gone) shows nothing.
        let html = in_owner(|| {
            let app = App::new();
            app.wonderful.cube_from.set(Some(CubeFrom::Recycle(built)));
            render_with(app, &state, 0)
        });
        assert!(!html.contains("modal-bg"));
    }

    #[test]
    fn the_other_players_show_what_they_have_built() {
        let mut state = State::new(4, 12);
        state.players[2].empire = vec![id("Quarry"), id("Skiff")];
        state.players[2].buildings = vec![wonderful_core::Building { card: id("Bastion"), filled: Cost::default() }];
        state.players[2].krystallium = 2;
        let html = in_owner(|| {
            let n = state.seats();
            board(
                App::new(),
                state.view_for(0),
                names(n),
                vec![true, true, false, true],
                "ABCD".into(),
            )
            .to_html()
        });
        assert_eq!(html.matches("class=\"rival\"").count(), 3);
        assert!(html.contains("Cy") && html.contains("tag tag-off"), "Cy is offline");
        assert_eq!(html.matches("tag tag-off").count(), 1);
        assert!(html.contains("Quarry") && html.contains("Skiff") && html.contains("Bastion"));
        assert!(html.contains("0/4"), "progress of the Bastion");
        assert!(html.contains("deciding"));
    }

    #[test]
    fn nothing_of_the_others_hands_is_drawn() {
        let mut state = State::new(3, 12);
        state.players[0].hand = vec![id("Lab Bench"), id("Lost Ruins")];
        state.players[1].hand = vec![id("Utopia Plan"), id("Archive Vault")];
        state.players[2].hand = vec![id("Monument"), id("Skiff")];
        state.players[1].picked = Some(id("Bazaar"));
        state.players[1].drafted = vec![id("Caravan")];
        let html = render(&state, 0);
        assert!(html.contains("Lab Bench") && html.contains("Lost Ruins"));
        for hidden in ["Utopia Plan", "Archive Vault", "Monument", "Skiff", "Bazaar", "Caravan"] {
            assert!(!html.contains(hidden), "{hidden} leaked");
        }
    }

    #[test]
    fn the_empire_groups_finished_cards_by_type() {
        let mut state = State::new(3, 12);
        state.players[0].empire = vec![id("Quarry"), id("Smelter"), id("Skiff"), id("Monument"), id("Lab Bench")];
        let html = render(&state, 0);
        assert!(html.contains("Your Empire") && html.contains("Aurelian Union"));
        assert_eq!(html.matches("class=\"kind-col\"").count(), 3, "Structure, Vehicle and Research");
        assert!(html.contains("Produces each round") && html.contains("You hold"));
        assert!(!html.contains("No finished cards yet."));
        let bare = render(&State::new(3, 12), 0);
        assert!(bare.contains("No finished cards yet."));
    }

    #[test]
    fn the_log_lists_what_happened_newest_first() {
        let mut state = State::new(3, 4);
        let fresh = render(&state, 0);
        assert!(fresh.contains("Round 1 of 4 begins."));
        state.log = vec![
            wonderful_core::Event::RoundStarted(1),
            wonderful_core::Event::Completed { seat: 1, card: id("Quarry") },
            wonderful_core::Event::Supremacy { res: Res::Gold, seat: Some(0) },
        ];
        let html = render(&state, 0);
        let newest = html.find("You produced the most Gold").unwrap();
        let older = html.find("Bob finished Quarry.").unwrap();
        assert!(newest < older);
        // Only the latest twelve lines are listed.
        state.log = (0..24).map(|_| wonderful_core::Event::RoundStarted(1)).collect();
        assert_eq!(render(&state, 0).matches("begins.").count(), 12);
    }

    #[test]
    fn every_card_of_the_catalogue_can_be_drawn() {
        let mut state = State::new(3, 1);
        state.players[0].hand = (0..catalogue().len() as u16).map(CardId).collect();
        let html = render(&state, 0);
        for card in catalogue() {
            assert!(html.contains(card.name.replace('\'', "&#39;").as_str()) || html.contains(card.name), "{}", card.name);
        }
        for kind in Kind::ALL {
            assert!(html.contains(kind.name()));
        }
    }

    #[test]
    fn player_names_are_escaped() {
        let state = State::new(2, 3);
        let html = in_owner(|| {
            board(
                App::new(),
                state.view_for(0),
                vec!["<b>Ann</b>".into(), "Bob & co".into()],
                vec![true, true],
                "ABCD".into(),
            )
            .to_html()
        });
        assert!(!html.contains("<b>Ann</b>"), "an unescaped name");
        assert!(html.contains("Bob &amp; co"));
    }
}
