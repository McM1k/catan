//! The It's a Wonderful World screens around the board: the page frame,
//! create/join, and the waiting room.

use super::look;
use crate::state::{create_room, join_room, leave, send, App, Screen};
use leptos::prelude::*;
use protocol::{ClientMsg, GameKind, LobbyPlayer};
use wonderful_core::Res;

/// The title of the browser tab.
pub const TAB_TITLE: &str = "It's a Wonderful World";

/// The frame every screen of the game sits in: its own look (see
/// `wonderful.css`, everything in it is scoped under `.wonderful`).
pub fn shell(content: impl IntoView + 'static) -> impl IntoView {
    view! {
        <div class="wonderful">
            <main class="app">{content}</main>
        </div>
    }
}

/// The five resources as cubes: the game's mark.
fn mark() -> impl IntoView {
    let cubes = Res::ALL
        .iter()
        .map(|&res| {
            view! { <span class=format!("cube {}", look::res_class(res))>{look::res_letter(res)}</span> }
        })
        .collect_view();
    view! { <div class="mark" aria-hidden="true">{cubes}</div> }
}

/// Name + create a room, or join one with its code.
pub fn home_screen(app: App) -> impl IntoView {
    let create = move |_| create_room(app, GameKind::Wonderful);
    let join = move |_| join_room(app);

    view! {
        <div class="entry">
            {mark()}
            <h1 class="game-title">"It's a Wonderful World"</h1>
            <p class="lede">
                "Draft cards, build an empire and out-produce your rivals over four rounds. For 2 to 5 players, and everybody plays at once."
            </p>
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
                <button class="primary" on:click=create disabled=move || !app.online.get()>
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
            </div>
        </div>
    }
}

/// The waiting room: who is here, and the host's start button. The game has
/// no options to pick.
pub fn lobby_screen(
    app: App,
    room: String,
    players: Vec<LobbyPlayer>,
    you: usize,
    is_host: bool,
) -> impl IntoView {
    let can_start = players.len() >= GameKind::Wonderful.min_players();
    let rows = players
        .iter()
        .enumerate()
        .map(|(i, p)| {
            view! {
                <li>
                    <span class="seat-name">{p.name.clone()}{(i == you).then_some(" (you)")}</span>
                    {(i == 0).then(|| view! { <span class="tag">"host"</span> })}
                    {(!p.connected).then(|| view! { <span class="tag tag-off">"offline"</span> })}
                </li>
            }
        })
        .collect_view();

    let start = if is_host {
        view! {
            <button class="primary" disabled=!can_start on:click=move |_| send(app, &ClientMsg::Start)>
                "Start game"
            </button>
            <p class="hint">
                {if can_start {
                    "Everyone who's here is in. Seats are drawn when you start."
                } else {
                    "Needs at least 2 players."
                }}
            </p>
        }
        .into_any()
    } else {
        view! { <p class="hint">"Waiting for the host to start the game…"</p> }.into_any()
    };

    view! {
        <div class="entry">
            {mark()}
            <h1 class="game-title">"It's a Wonderful World"</h1>
            <div class="panel">
                <h2>"Waiting for players"</h2>
                <p class="hint">"Share this code with everyone you're playing with:"</p>
                <div class="room-code">{room}</div>
                <ul class="roster">{rows}</ul>
                {start}
                <button class="link" on:click=move |_| leave(app)>
                    "Leave room"
                </button>
            </div>
        </div>
    }
}
