//! The first screen: pick which game to play.

use crate::state::{App, Screen};
use leptos::prelude::*;

/// Where the Hanabi card leads. Set `HANABI_URL` when building the client,
/// e.g. `HANABI_URL=https://hanabi.example.com trunk build --release`.
/// Without it the card is shown greyed out.
const HANABI_URL: Option<&str> = option_env!("HANABI_URL");

pub fn menu_screen(app: App) -> impl IntoView {
    menu_view(app, HANABI_URL)
}

pub fn menu_view(app: App, hanabi_url: Option<&'static str>) -> impl IntoView {
    let colonists = view! {
        <button class="game-card" on:click=move |_| app.screen.set(Screen::Home)>
            {card_body(
                colonists_icon().into_any(),
                "Colonists",
                "2–4 players",
                "Build roads and settlements, trade resources and race to 10 points on a random island.",
                "Play",
            )}
        </button>
    };

    let hanabi_body = |footer: &'static str| {
        card_body(
            hanabi_icon().into_any(),
            "Hanabi",
            "2–5 players",
            "A cooperative card game: build the fireworks in order, but you can't see your own hand.",
            footer,
        )
    };
    let hanabi = match hanabi_url {
        Some(url) => view! { <a class="game-card" href=url>{hanabi_body("Play")}</a> }.into_any(),
        None => view! {
            <div class="game-card disabled" aria-disabled="true">{hanabi_body("Not connected yet")}</div>
        }
        .into_any(),
    };

    view! {
        <div class="menu">
            <h1>"Pick a game"</h1>
            <p class="muted">"Play with friends in your browser: no accounts, just a room code."</p>
            <div class="game-grid">{colonists}{hanabi}</div>
        </div>
    }
}

fn card_body(
    icon: AnyView,
    title: &'static str,
    players: &'static str,
    blurb: &'static str,
    footer: &'static str,
) -> impl IntoView {
    view! {
        <div class="game-icon">{icon}</div>
        <h2>{title}</h2>
        <div class="game-players">{players}</div>
        <p class="game-blurb">{blurb}</p>
        <div class="game-cta">{footer}</div>
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
fn colonists_icon() -> impl IntoView {
    view! {
        <svg viewBox="0 0 64 64" role="img" aria-label="Colonists">
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
