//! The first screen: pick which game to play.

use crate::state::{App, Screen};
use leptos::prelude::*;
use protocol::GameKind;

pub fn menu_screen(app: App) -> impl IntoView {
    let catan = game_card(
        app,
        GameKind::Catan,
        catan_icon().into_any(),
        "Build roads and settlements, trade resources and race to 10 points on a random island.",
    );
    let hanabi = game_card(
        app,
        GameKind::Hanabi,
        hanabi_icon().into_any(),
        "A cooperative card game: build the fireworks in order, but you can't see your own hand.",
    );

    let wonderful = game_card(
        app,
        GameKind::Wonderful,
        wonderful_icon().into_any(),
        "Draft cards, build an empire and out-produce your rivals over four rounds. Everybody plays at once.",
    );

    view! {
        <div class="menu-page">
            <div class="menu">
                <h1>"Pick a game"</h1>
                <p class="muted">"Play with friends in your browser: no accounts, just a room code."</p>
                <div class="game-grid">{catan}{hanabi}{wonderful}</div>
            </div>
        </div>
    }
}

/// One card of the picker; it leads to that game's create/join screen.
fn game_card(app: App, game: GameKind, icon: AnyView, blurb: &'static str) -> impl IntoView {
    let players = format!("{}\u{2013}{} players", game.min_players(), game.max_players());
    view! {
        <button class="game-card" on:click=move |_| app.screen.set(Screen::Home(game))>
            <div class="game-icon">{icon}</div>
            <h2>{game.title()}</h2>
            <div class="game-players">{players}</div>
            <p class="game-blurb">{blurb}</p>
            <div class="game-cta">"Play"</div>
        </button>
    }
}

fn hex_points(cx: f64, cy: f64, r: f64) -> String {
    let h = r * 0.866;
    [
        (cx, cy - r),
        (cx + h, cy - r / 2.0),
        (cx + h, cy + r / 2.0),
        (cx, cy + r),
        (cx - h, cy + r / 2.0),
        (cx - h, cy - r / 2.0),
    ]
    .iter()
    .map(|(x, y)| format!("{x:.2},{y:.2}"))
    .collect::<Vec<_>>()
    .join(" ")
}

/// Three terrain hexes (forest, hills, fields).
fn catan_icon() -> impl IntoView {
    view! {
        <svg viewBox="0 0 64 64" role="img" aria-label="Catan">
            <polygon points=hex_points(21.0, 24.0, 12.0) fill="#2d6a4f" stroke="#fdf6e3" stroke-width="1.5" />
            <polygon points=hex_points(43.0, 24.0, 12.0) fill="#c1553b" stroke="#fdf6e3" stroke-width="1.5" />
            <polygon points=hex_points(32.0, 43.0, 12.0) fill="#e0b94f" stroke="#fdf6e3" stroke-width="1.5" />
            <circle cx="32" cy="43" r="5" fill="#f6efdd" />
            <text x="32" y="46" text-anchor="middle" font-size="8" font-weight="700" fill="#c1121f">"8"</text>
        </svg>
    }
}

/// Five fanned cards in the five Hanabi colours.
fn hanabi_icon() -> impl IntoView {
    let card = |angle: i32, fill: &'static str| {
        view! {
            <rect x="25" y="14" width="14" height="24" rx="2.5" fill=fill stroke="#1d2c3a" stroke-width="1.2"
                  transform=format!("rotate({angle} 32 52)") />
        }
    };
    view! {
        <svg viewBox="0 0 64 64" role="img" aria-label="Hanabi">
            {card(-32, "#e63946")}
            {card(-16, "#f2cc4b")}
            {card(0, "#f6efdd")}
            {card(16, "#2ec4b6")}
            {card(32, "#3a86ff")}
        </svg>
    }
}

/// A card with a planet on it and the five resource cubes along its foot.
fn wonderful_icon() -> impl IntoView {
    let cube = |x: f64, fill: &'static str| {
        view! { <rect x=x y="42" width="5" height="5" rx="1.2" fill=fill stroke="#0b2a2a" stroke-width="0.6" /> }
    };
    view! {
        <svg viewBox="0 0 64 64" role="img" aria-label="It's a Wonderful World">
            <g transform="rotate(-6 32 32)">
                <rect x="14" y="8" width="36" height="48" rx="4" fill="#eef1ea" stroke="#0b2a2a" stroke-width="1.5" />
                <path d="M14 12a4 4 0 0 1 4-4h28a4 4 0 0 1 4 4v6H14z" fill="#1f9d8f" />
                <circle cx="32" cy="31" r="8" fill="#ff6b8b" />
                <ellipse cx="32" cy="31" rx="13" ry="3.4" fill="none" stroke="#0b2a2a" stroke-width="1.4" transform="rotate(-18 32 31)" />
                {cube(16.5, "#9aa5b1")}
                {cube(23.0, "#2b2f33")}
                {cube(29.5, "#3fb86e")}
                {cube(36.0, "#f2c230")}
                {cube(42.5, "#3b82e0")}
            </g>
        </svg>
    }
}
