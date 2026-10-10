// The views nest deeply; the compiler needs room to work out their types.
#![recursion_limit = "256"]

mod art;
mod board;
mod game;
mod hanabi;
mod home;
mod menu;
mod state;
mod wonderful;

use leptos::prelude::*;
use protocol::GameKind;
use state::{connect, App, Screen};

/// A Catan screen. Its stylesheet is scoped under `.theme-catan`, so
/// the games can't restyle each other.
fn catan(content: impl IntoView + 'static) -> AnyView {
    view! { <div class="theme-catan">{content.into_any()}</div> }.into_any()
}

/// A Hanabi screen: the same, under `.hanabi` (see `hanabi::screens::shell`).
fn hanabi_page(app: App, content: impl IntoView + 'static) -> AnyView {
    hanabi::screens::shell(app, content.into_any()).into_any()
}

/// An It's a Wonderful World screen: the same, under `.wonderful` (see
/// `wonderful::screens::shell`).
fn wonderful_page(content: impl IntoView + 'static) -> AnyView {
    wonderful::screens::shell(content.into_any()).into_any()
}

/// Whatever the current screen says should be shown.
fn current_screen(app: App) -> AnyView {
    match app.screen.get() {
        Screen::Menu => menu::menu_screen(app).into_any(),
        Screen::Home(GameKind::Catan) => catan(home::home_screen(app)),
        Screen::Home(GameKind::Hanabi) => hanabi_page(app, hanabi::screens::home_screen(app)),
        Screen::Home(GameKind::Wonderful) => wonderful_page(wonderful::screens::home_screen(app)),
        Screen::Lobby {
            room,
            game: GameKind::Catan,
            players,
            is_host,
            ..
        } => catan(home::lobby_screen(app, room, players, is_host)),
        Screen::Lobby {
            room,
            game: GameKind::Hanabi,
            players,
            you,
            is_host,
        } => hanabi_page(app, hanabi::screens::lobby_screen(app, room, players, you, is_host)),
        Screen::Lobby {
            room,
            game: GameKind::Wonderful,
            players,
            you,
            is_host,
        } => wonderful_page(wonderful::screens::lobby_screen(app, room, players, you, is_host)),
        Screen::Game { view, connected, room } => catan(game::game_screen(app, *view, connected, room)),
        Screen::HanabiGame => hanabi_page(app, hanabi::board::board(app)),
        Screen::WonderfulGame { view, names, connected, room } => {
            wonderful_page(wonderful::board::board(app, *view, names, connected, room))
        }
    }
}

/// The title of the browser tab: the game on screen, or just "Games".
fn page_title(app: App) -> &'static str {
    let game = app.screen.with(|screen| match screen {
        Screen::Menu => None,
        Screen::Home(game) | Screen::Lobby { game, .. } => Some(*game),
        Screen::Game { .. } => Some(GameKind::Catan),
        Screen::HanabiGame => Some(GameKind::Hanabi),
        Screen::WonderfulGame { .. } => Some(GameKind::Wonderful),
    });
    match game {
        None => "Games",
        Some(GameKind::Catan) => "Catan",
        Some(GameKind::Hanabi) => hanabi::screens::tab_title(app),
        Some(GameKind::Wonderful) => wonderful::screens::TAB_TITLE,
    }
}

#[component]
fn Root() -> impl IntoView {
    let app = App::new();
    connect(app);

    Effect::new(move |_| document().set_title(page_title(app)));

    view! {
        <div class="root">
            {move || {
                let err = app.error.get();
                err.map(|e| view! { <div class="toast">{e}</div> })
            }}
            {move || (!app.online.get()).then(|| view! { <div class="offline-banner">"Connecting to server…"</div> })}
            {move || current_screen(app)}
        </div>
    }
}

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(Root);
}

/// Native render smoke tests: build real game states with the engines and
/// render every screen to HTML, catching panics (bad indexing, unwraps)
/// that the type checker can't see. These do not replace a browser test.
#[cfg(test)]
mod render_tests {
    use super::*;
    use engine::{Action, Game, Phase, Rules, SetupExpect};
    use hanabi_core::{GameRules, GameState, PlayerId};
    use leptos::tachys::view::RenderHtml;
    use protocol::LobbyPlayer;
    use rand::{rngs::StdRng, SeedableRng};

    fn game_after_setup(n: usize) -> (Game, StdRng) {
        let mut rng = StdRng::seed_from_u64(42);
        let names = (0..n).map(|i| format!("P{i}")).collect();
        let mut g = Game::new(Rules::default(), names, &mut rng);
        while let Phase::Setup { expect, .. } = g.phase.clone() {
            let s = g.current;
            let a = match expect {
                SetupExpect::Settlement => Action::BuildSettlement { vertex: g.legal_settlements(s, true)[0] },
                SetupExpect::Road => Action::BuildRoad { edge: g.legal_roads(s, g.setup_vertex)[0] },
            };
            g.apply(s, a, &mut rng).unwrap();
        }
        (g, rng)
    }

    fn render(g: &Game, seat: Option<usize>) -> String {
        let owner = Owner::new();
        owner.with(|| {
            let app = App::new();
            let view = g.view_for(seat);
            let n = view.players.len();
            game::game_screen(app, view, vec![true; n], "ABCD".into()).to_html()
        })
    }

    #[test]
    fn renders_every_phase_for_every_seat() {
        let (mut g, mut rng) = game_after_setup(3);
        for seat in [Some(0), Some(1), Some(2), None] {
            let html = render(&g, seat);
            assert!(html.contains("Roll dice") == (seat == Some(0)));
        }
        g.do_roll(2, 3, &mut rng).unwrap();
        g.players[0].hand = engine::Hand([4, 4, 4, 4, 4]);
        g.players[0].dev = vec![engine::DevCard::Knight, engine::DevCard::Monopoly];
        for seat in [Some(0), Some(1), None] {
            let html = render(&g, seat);
            assert!(html.contains("<svg"));
        }
        let html = render(&g, Some(0));
        assert!(html.contains("End turn"));
        assert!(html.contains("Trade with the bank"));
        assert!(html.contains("Knight"));

        // Robber, discard and open-trade states.
        g.players[1].hand = engine::Hand([3, 3, 3, 0, 0]);
        g.phase = Phase::Roll;
        g.do_roll(3, 4, &mut rng).unwrap();
        for seat in [Some(0), Some(1), Some(2)] {
            let html = render(&g, seat);
            assert!(!html.is_empty());
        }
        assert!(render(&g, Some(1)).contains("A 7 was rolled"));
        g.phase = Phase::MoveRobber { back_to_roll: false };
        assert!(render(&g, Some(0)).contains("target"));
        g.phase = Phase::Main;
        g.apply(
            0,
            Action::ProposeTrade { give: engine::Hand([1, 0, 0, 0, 0]), want: engine::Hand([0, 0, 1, 0, 0]) },
            &mut rng,
        )
        .unwrap();
        assert!(render(&g, Some(1)).contains("Trade offer"));
        assert!(render(&g, Some(0)).contains("Your offer"));
        g.phase = Phase::Finished { winner: 2 };
        assert!(render(&g, Some(2)).contains("You win"));
    }

    #[test]
    fn board_shows_terrain_art_and_fresh_embers() {
        let (g, _) = game_after_setup(3);
        let owner = Owner::new();
        owner.with(|| {
            let app = App::new();
            let view = g.view_for(Some(0));
            let n = view.players.len();
            let render = |app: App, view: engine::GameView| {
                game::game_screen(app, view, vec![true; n], "ABCD".into()).to_html()
            };

            // Art for every terrain, drawn from gradients + a hex clip.
            let plain = render(app, view.clone());
            for marker in [
                "g-forest", "g-hills", "g-pasture", "g-fields", "g-mountains", "g-desert", "hexclip",
                "#13422a", "#6e2815", "#8a8a80", "#a8801e", "#4a5568", "#2f5e2c",
            ] {
                assert!(plain.contains(marker), "missing {marker}");
            }
            assert_eq!(plain.matches("class=\"ember\"").count(), 0);

            // Harbours: one coloured 2:1 token per resource, four neutral 3:1 tokens.
            for color in ["#2d6a4f", "#c1553b", "#7bc99b", "#e0b94f", "#8d99ae"] {
                assert!(plain.contains(&format!("fill=\"{color}\"")), "no harbour in {color}");
            }
            assert_eq!(plain.matches("fill=\"#fdf6e3\"").count(), 4);

            // One ember per fresh item; expired ones are not drawn.
            let road = view.roads.iter().position(|r| r.is_some()).unwrap();
            let piece = view.buildings.iter().position(|b| b.is_some()).unwrap();
            let now = state::now_ms();
            app.fresh.set(vec![
                state::Fresh { kind: state::FreshKind::Road(road), at: now - 100.0 },
                state::Fresh { kind: state::FreshKind::Piece(piece), at: now - 500.0 },
                state::Fresh { kind: state::FreshKind::Robber(view.robber), at: now },
                state::Fresh { kind: state::FreshKind::Piece(piece), at: now - 5000.0 },
            ]);
            let lit = render(app, view);
            assert_eq!(lit.matches("class=\"ember\"").count(), 3);
            assert!(lit.contains("animation-delay:-"));
        });
    }

    fn players(names: &[&str]) -> Vec<LobbyPlayer> {
        names
            .iter()
            .map(|n| LobbyPlayer { name: n.to_string(), connected: true })
            .collect()
    }

    /// Whether `html` shows `text`, however the renderer escaped its quotes.
    fn shows(html: &str, text: &str) -> bool {
        html.contains(text) || html.contains(&text.replace('\'', "&#39;"))
    }

    #[test]
    fn menu_lists_every_game_and_leads_to_their_rooms() {
        let owner = Owner::new();
        owner.with(|| {
            let app = App::new();
            assert!(matches!(app.screen.get_untracked(), Screen::Menu));
            let html = current_screen(app).to_html();
            assert!(html.contains("Pick a game") && html.contains("menu-page"));
            assert!(html.contains("Catan") && html.contains("Hanabi"));
            assert!(shows(&html, "It's a Wonderful World"));
            // Catan seats 2-4; Hanabi and It's a Wonderful World seat 2-5.
            assert_eq!(html.matches("2\u{2013}4 players").count(), 1);
            assert_eq!(html.matches("2\u{2013}5 players").count(), 2);
            // Every game is playable here: no greyed-out card, no outside link.
            assert!(!html.contains("disabled") && !html.contains("href="));
            assert_eq!(page_title(app), "Games");

            app.screen.set(Screen::Home(GameKind::Catan));
            let html = current_screen(app).to_html();
            assert!(html.contains("theme-catan") && html.contains("Create a room"));
            assert!(!html.contains("class=\"hanabi\"") && !html.contains("class=\"wonderful\""));
            assert_eq!(page_title(app), "Catan");

            app.screen.set(Screen::Home(GameKind::Hanabi));
            let html = current_screen(app).to_html();
            assert!(html.contains("class=\"hanabi\"") && html.contains("Create a room"));
            assert!(html.contains("Join room") && html.contains("All games"));
            assert!(!html.contains("theme-catan") && !html.contains("class=\"wonderful\""));
            assert_eq!(page_title(app), "Hanabi");

            app.screen.set(Screen::Home(GameKind::Wonderful));
            let html = current_screen(app).to_html();
            assert!(html.contains("class=\"wonderful\"") && html.contains("Create a room"));
            assert!(html.contains("Join room") && html.contains("All games"));
            assert!(shows(&html, "It's a Wonderful World"));
            assert!(!html.contains("theme-catan") && !html.contains("class=\"hanabi\""));
            assert_eq!(page_title(app), "It's a Wonderful World");
        });
    }

    #[test]
    fn the_room_code_decides_which_lobby_you_land_in() {
        let owner = Owner::new();
        owner.with(|| {
            let app = App::new();
            let lobby = |game| Screen::Lobby {
                room: "WXYZ".into(),
                game,
                players: players(&["Ann", "Bob"]),
                you: 1,
                is_host: false,
            };

            // Picked Catan in the menu, but the code belongs to a Hanabi room.
            app.screen.set(Screen::Home(GameKind::Catan));
            app.screen.set(lobby(GameKind::Hanabi));
            let html = current_screen(app).to_html();
            assert!(html.contains("WXYZ") && html.contains("Hanabii mode"));
            assert!(html.contains("Waiting for the host to pick the rules"));
            assert!(html.contains("Bob") && html.contains("(you)"));
            assert!(!html.contains("Start game"));

            app.screen.set(lobby(GameKind::Catan));
            let html = current_screen(app).to_html();
            assert!(html.contains("WXYZ") && html.contains("Waiting for the host to start"));
            assert!(!html.contains("Hanabii mode"));

            // ...or to It's a Wonderful World, which has no rules to pick.
            app.screen.set(lobby(GameKind::Wonderful));
            let html = current_screen(app).to_html();
            assert!(html.contains("WXYZ") && html.contains("Waiting for the host to start"));
            assert!(html.contains("class=\"wonderful\"") && html.contains("(you)"));
            assert!(!html.contains("Hanabii mode") && !html.contains("theme-catan"));
            assert!(!html.contains("Start game"));
            assert_eq!(page_title(app), "It's a Wonderful World");
        });
    }

    #[test]
    fn catan_lobby_renders() {
        let owner = Owner::new();
        owner.with(|| {
            let app = App::new();
            assert!(home::home_screen(app).to_html().contains("Create a room"));
            let players = vec![
                protocol::LobbyPlayer { name: "Ann".into(), connected: true },
                protocol::LobbyPlayer { name: "Bob".into(), connected: false },
            ];
            let html = home::lobby_screen(app, "WXYZ".into(), players, true).to_html();
            assert!(html.contains("WXYZ") && html.contains("Start game"));
            assert!(html.contains("offline"));
        });
    }

    #[test]
    fn hanabi_lobby_lets_only_the_host_pick_rules_and_start() {
        let owner = Owner::new();
        owner.with(|| {
            let app = App::new();
            let seats = players(&["Ann", "Bob", "Cy"]);
            let host = hanabi::screens::lobby_screen(app, "WXYZ".into(), seats.clone(), 0, true).to_html();
            assert!(host.contains("WXYZ") && host.contains("Start game"));
            assert!(host.contains("Hanabii mode") && host.contains("Multicolor suit"));
            assert!(host.contains("Black powder suit") && host.contains("Six-card suits"));
            assert!(host.contains("Ann") && host.contains("(you)") && host.contains("host"));
            assert!(host.contains("Leave room"));
            // The start button is enabled with three players...
            assert!(host.contains("Everyone who's here is in"));
            assert!(!host.contains("Needs at least 2 players"));
            // ...and the rules are editable. Only the "1 of each card" sub-options
            // of suits that aren't switched on are locked.
            assert_eq!(host.matches("disabled").count(), 3, "{host}");

            // Alone, the host can't start yet.
            let alone = hanabi::screens::lobby_screen(app, "WXYZ".into(), players(&["Ann"]), 0, true).to_html();
            assert!(alone.contains("Needs at least 2 players"));

            // A guest sees the same rules, locked, and no start button.
            let guest = hanabi::screens::lobby_screen(app, "WXYZ".into(), seats, 2, false).to_html();
            assert!(guest.contains("Hanabii mode") && guest.contains("Cy") && guest.contains("(you)"));
            assert!(!guest.contains("Start game"));
            assert!(guest.contains("Waiting for the host to pick the rules and start the game"));
            // Every control is locked for them (the mode, both suits and their
            // sub-options, the extra-colors count and its sub-option, six cards).
            assert_eq!(guest.matches("disabled").count(), 8, "{guest}");

            // A seat whose connection dropped says so.
            let mut seats = players(&["Ann", "Bob"]);
            seats[1].connected = false;
            let html = hanabi::screens::lobby_screen(app, "WXYZ".into(), seats, 0, true).to_html();
            assert!(html.contains("offline"));
        });
    }

    #[test]
    fn wonderful_lobby_lets_only_the_host_start() {
        let owner = Owner::new();
        owner.with(|| {
            let app = App::new();
            let seats = players(&["Ann", "Bob", "Cy"]);
            let host = wonderful::screens::lobby_screen(app, "WXYZ".into(), seats.clone(), 0, true).to_html();
            assert!(host.contains("WXYZ") && host.contains("Start game"));
            assert!(host.contains("Ann") && host.contains("(you)") && host.contains("host"));
            assert!(host.contains("Leave room"));
            // With three players the start button is enabled.
            assert!(shows(&host, "Everyone who's here is in"));
            assert!(!host.contains("Needs at least 2 players") && !host.contains("disabled"));

            // Alone, the host can't start yet.
            let alone = wonderful::screens::lobby_screen(app, "WXYZ".into(), players(&["Ann"]), 0, true).to_html();
            assert!(alone.contains("Needs at least 2 players") && alone.contains("disabled"));

            // A guest waits for the host.
            let guest = wonderful::screens::lobby_screen(app, "WXYZ".into(), seats, 2, false).to_html();
            assert!(guest.contains("Cy") && guest.contains("(you)"));
            assert!(guest.contains("Waiting for the host to start the game"));
            assert!(!guest.contains("Start game"));

            // A seat whose connection dropped says so.
            let mut seats = players(&["Ann", "Bob"]);
            seats[1].connected = false;
            let html = wonderful::screens::lobby_screen(app, "WXYZ".into(), seats, 0, true).to_html();
            assert!(html.contains("offline"));
        });
    }

    #[test]
    fn a_wonderful_game_is_drawn_in_its_own_frame_and_titles_the_page() {
        let owner = Owner::new();
        owner.with(|| {
            let app = App::new();
            let state = wonderful_core::State::new(3, 7);
            app.screen.set(Screen::WonderfulGame {
                view: Box::new(state.view_for(1)),
                names: vec!["Ann".into(), "Bob".into(), "Cy".into()],
                connected: vec![true; 3],
                room: "WXYZ".into(),
            });
            let html = current_screen(app).to_html();
            assert!(html.contains("class=\"wonderful\"") && html.contains("WXYZ"));
            assert!(html.contains("Draft") && html.contains("Leave game"));
            assert!(!html.contains("theme-catan") && !html.contains("class=\"hanabi\""));
            assert_eq!(page_title(app), "It's a Wonderful World");
        });
    }

    #[test]
    fn hanabii_mode_locks_the_other_rules_and_retitles_the_page() {
        let owner = Owner::new();
        owner.with(|| {
            let app = App::new();
            let ordinary = hanabi::screens::shell(app, hanabi::screens::lobby_screen(app, "WXYZ".into(), players(&["A", "B"]), 0, true)).to_html();
            assert!(ordinary.contains("<h1>Hanabi</h1>") && !ordinary.contains("mode-popover"));
            assert!(!ordinary.contains("rule-group-locked"));
            app.screen.set(Screen::HanabiGame);
            assert_eq!(page_title(app), "Hanabi");

            app.hanabi.rules.set(GameRules { hanabii: true, ..Default::default() }.normalized());
            let html = hanabi::screens::shell(app, hanabi::screens::lobby_screen(app, "WXYZ".into(), players(&["A", "B"]), 0, true)).to_html();
            assert!(html.contains("<h1>Hanabii</h1>") && html.contains("mode-popover"));
            assert!(html.contains("rule-group-locked"));
            assert_eq!(page_title(app), "Hanabii");

            // Once the game runs, what it is played with decides.
            let state = GameState::new(2, 3, GameRules::default());
            app.hanabi.view.set(Some(state.view_for(PlayerId(0))));
            assert_eq!(page_title(app), "Hanabi");
        });
    }
}
