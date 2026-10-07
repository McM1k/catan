mod art;
mod board;
mod game;
mod home;
mod menu;
mod state;

use leptos::prelude::*;
use state::{connect, App, Screen};

#[component]
fn Root() -> impl IntoView {
    let app = App::new();
    connect(app);

    view! {
        <div class="app">
            {move || {
                let err = app.error.get();
                err.map(|e| view! { <div class="toast">{e}</div> })
            }}
            {move || (!app.online.get()).then(|| view! { <div class="offline-banner">"Connecting to server…"</div> })}
            {move || match app.screen.get() {
                Screen::Menu => menu::menu_screen(app).into_any(),
                Screen::Home => home::home_screen(app).into_any(),
                Screen::Lobby { room, players, is_host } => home::lobby_screen(app, room, players, is_host).into_any(),
                Screen::Game { view, connected, room } => game::game_screen(app, *view, connected, room).into_any(),
            }}
        </div>
    }
}

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(Root);
}

/// Native render smoke tests: build real game states with the engine and
/// render every screen to HTML, catching panics (bad indexing, unwraps)
/// that the type checker can't see. These do not replace a browser test.
#[cfg(test)]
mod render_tests {
    use super::*;
    use engine::{Action, Game, Phase, Rules, SetupExpect};
    use leptos::tachys::view::RenderHtml;
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

    #[test]
    fn menu_lists_both_games() {
        let owner = Owner::new();
        owner.with(|| {
            let app = App::new();
            // Starts on the menu; Colonists opens its room screen, Hanabi is greyed out until configured.
            assert!(matches!(app.screen.get_untracked(), Screen::Menu));
            let html = menu::menu_view(app, None).to_html();
            assert!(html.contains("Colonists") && html.contains("Hanabi"));
            assert!(html.contains("Not connected yet"));
            assert!(!html.contains("href="));

            let html = menu::menu_view(app, Some("https://example.com/hanabi")).to_html();
            assert!(html.contains("href=\"https://example.com/hanabi\""));
            assert!(!html.contains("Not connected yet"));
        });
    }

    #[test]
    fn home_and_lobby_render() {
        let owner = Owner::new();
        owner.with(|| {
            let app = App::new();
            assert!(home::home_screen(app).to_html().contains("Create a room"));
            let players = vec![
                engine::protocol::LobbyPlayer { name: "Ann".into(), connected: true },
                engine::protocol::LobbyPlayer { name: "Bob".into(), connected: false },
            ];
            let html = home::lobby_screen(app, "WXYZ".into(), players, true).to_html();
            assert!(html.contains("WXYZ") && html.contains("Start game"));
        });
    }
}
