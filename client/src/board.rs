use crate::art::{gradient_id, tile_art, DEFS};
use crate::state::{now_ms, send, App, FreshKind, Mode, EMBER_MS};
use engine::protocol::ClientMsg;
use engine::{Action, GameView, Phase, Resource};
use leptos::prelude::*;

pub const PLAYER_COLORS: [&str; 4] = ["#e63946", "#3a86ff", "#f4a261", "#2ec4b6"];

/// (fill, text) colours of a harbour token. 2:1 harbours use their resource's
/// colour (the same as the hand cards in the sidebar); 3:1 stays neutral.
fn port_colors(kind: Option<Resource>) -> (&'static str, &'static str) {
    match kind {
        None => ("#fdf6e3", "#3d3322"),
        Some(Resource::Wood) => ("#2d6a4f", "#ffffff"),
        Some(Resource::Brick) => ("#c1553b", "#ffffff"),
        Some(Resource::Sheep) => ("#7bc99b", "#08210f"),
        Some(Resource::Wheat) => ("#e0b94f", "#2b2100"),
        Some(Resource::Ore) => ("#8d99ae", "#10161d"),
    }
}

fn f(x: f64) -> String {
    format!("{x:.3}")
}

/// The static part of the board (tiles, ports, pieces) plus a reactive
/// overlay of clickable targets that follows the current build mode.
pub fn board_svg(app: App, v: &GameView) -> impl IntoView {
    let board = &v.board;

    let tiles = board
        .tiles
        .iter()
        .enumerate()
        .map(|(ti, t)| {
            let (cx, cy) = t.center;
            let corner = |scale: f64| {
                t.vertices
                    .iter()
                    .map(|&vi| {
                        let p = board.vertices[vi].pos;
                        format!("{},{}", f(cx + (p.0 - cx) * scale), f(cy + (p.1 - cy) * scale))
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            };
            let number = t.number.map(|n| {
                let hot = n == 6 || n == 8;
                view! {
                    <g>
                        <circle cx=f(cx) cy=f(cy) r="0.38" fill="#f6efdd" stroke="#6b5b3e" stroke-width="0.03" />
                        <text x=f(cx) y=f(cy + 0.12) text-anchor="middle" font-size="0.42" font-weight="700"
                              fill=if hot { "#c1121f" } else { "#222" }>{n.to_string()}</text>
                    </g>
                }
            });
            view! {
                <g>
                    <polygon points=corner(1.0) fill=format!("url(#{})", gradient_id(t.terrain)) stroke="#fdf6e3" stroke-width="0.06" />
                    <polygon points=corner(0.9) fill="#ffffff" opacity="0.10" />
                    {tile_art(t.terrain, cx, cy, ti)}
                    {number}
                </g>
            }
        })
        .collect_view();

    let ports = board
        .ports
        .iter()
        .map(|p| {
            let e = &board.edges[p.edge];
            let (a, b) = (board.vertices[e.a].pos, board.vertices[e.b].pos);
            let (mx, my) = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
            let len = (mx * mx + my * my).sqrt().max(0.001);
            let (px, py) = (mx + mx / len * 0.75, my + my / len * 0.75);
            let (fill, ink) = port_colors(p.kind);
            // 2:1 harbours get the resource name on a second line so it fits the token.
            let name = p.kind.map(|r| r.name());
            view! {
                <g>
                    <line x1=f(a.0) y1=f(a.1) x2=f(px) y2=f(py) stroke="#7a6a4a" stroke-width="0.04" stroke-dasharray="0.08 0.06" />
                    <line x1=f(b.0) y1=f(b.1) x2=f(px) y2=f(py) stroke="#7a6a4a" stroke-width="0.04" stroke-dasharray="0.08 0.06" />
                    <circle cx=f(px) cy=f(py) r="0.34" fill=fill stroke="#7a6a4a" stroke-width="0.04" />
                    <text x=f(px) y=f(py + if name.is_some() { -0.02 } else { 0.07 }) text-anchor="middle"
                          font-size="0.2" font-weight="700" fill=ink>{if name.is_some() { "2:1" } else { "3:1" }}</text>
                    {name.map(|n| view! {
                        <text x=f(px) y=f(py + 0.15) text-anchor="middle" font-size="0.13" font-weight="600" fill=ink>{n}</text>
                    })}
                </g>
            }
        })
        .collect_view();

    let roads = v
        .roads
        .iter()
        .enumerate()
        .filter_map(|(ei, r)| r.map(|owner| (ei, owner)))
        .map(|(ei, owner)| {
            let e = &board.edges[ei];
            let (a, b) = (board.vertices[e.a].pos, board.vertices[e.b].pos);
            let color = PLAYER_COLORS[owner % 4];
            view! {
                <g>
                    <line x1=f(a.0) y1=f(a.1) x2=f(b.0) y2=f(b.1) stroke="#222" stroke-width="0.2" stroke-linecap="round" />
                    <line x1=f(a.0) y1=f(a.1) x2=f(b.0) y2=f(b.1) stroke=color stroke-width="0.13" stroke-linecap="round" />
                </g>
            }
        })
        .collect_view();

    let buildings = v
        .buildings
        .iter()
        .enumerate()
        .filter_map(|(vi, b)| b.map(|b| (vi, b)))
        .map(|(vi, b)| {
            let (x, y) = board.vertices[vi].pos;
            let color = PLAYER_COLORS[b.owner % 4];
            let shape = if b.city {
                "-0.26,0.18 0.26,0.18 0.26,-0.06 0.08,-0.06 0.08,-0.28 -0.08,-0.28 -0.08,-0.06 -0.26,-0.06"
            } else {
                "-0.17,0.15 0.17,0.15 0.17,-0.05 0,-0.22 -0.17,-0.05"
            };
            view! {
                <polygon points=shape fill=color stroke="#222" stroke-width="0.05"
                         transform=format!("translate({} {})", f(x), f(y)) />
            }
        })
        .collect_view();

    let (rx, ry) = {
        let c = board.tiles[v.robber].center;
        (c.0 + 0.0, c.1 + 0.42)
    };
    let robber = view! {
        <g>
            <ellipse cx=f(rx) cy=f(ry + 0.06) rx="0.17" ry="0.24" fill="#222" stroke="#fdf6e3" stroke-width="0.04" />
            <circle cx=f(rx) cy=f(ry - 0.2) r="0.12" fill="#222" stroke="#fdf6e3" stroke-width="0.04" />
        </g>
    };

    // Ember contours: roads glow from underneath (so the road stays visible
    // inside its outline), pieces and the robber get a ring on top.
    let ember_board = board.clone();
    let ember_buildings = v.buildings.clone();
    let ember_under = {
        let board = ember_board.clone();
        move || {
            let now = now_ms();
            app.fresh
                .get()
                .into_iter()
                .filter(|f| now - f.at < EMBER_MS)
                .filter_map(|f| match f.kind {
                    FreshKind::Road(e) => Some((e, (now - f.at).max(0.0))),
                    _ => None,
                })
                .map(|(e, age)| {
                    let ed = &board.edges[e];
                    let (a, b) = (board.vertices[ed.a].pos, board.vertices[ed.b].pos);
                    view! {
                        <line class="ember" x1=f(a.0) y1=f(a.1) x2=f(b.0) y2=f(b.1)
                              stroke-width="0.36" stroke-linecap="round"
                              style=format!("animation-delay:-{age:.0}ms") />
                    }
                })
                .collect_view()
        }
    };
    let ember_over = {
        let board = ember_board;
        move || {
            let now = now_ms();
            app.fresh
                .get()
                .into_iter()
                .filter(|f| now - f.at < EMBER_MS)
                .filter_map(|fresh| {
                    let age = (now - fresh.at).max(0.0);
                    match fresh.kind {
                        FreshKind::Piece(vi) => {
                            let city = matches!(ember_buildings.get(vi), Some(Some(b)) if b.city);
                            let (x, y) = board.vertices[vi].pos;
                            let r = if city { 0.44 } else { 0.37 };
                            Some(
                                view! {
                                    <circle class="ember" cx=f(x) cy=f(y - 0.03) r=f(r) stroke-width="0.07"
                                            style=format!("animation-delay:-{age:.0}ms") />
                                }
                                .into_any(),
                            )
                        }
                        FreshKind::Robber(ti) => {
                            let c = board.tiles[ti].center;
                            Some(
                                view! {
                                    <ellipse class="ember" cx=f(c.0) cy=f(c.1 + 0.4) rx="0.3" ry="0.46" stroke-width="0.07"
                                             style=format!("animation-delay:-{age:.0}ms") />
                                }
                                .into_any(),
                            )
                        }
                        FreshKind::Road(_) => None,
                    }
                })
                .collect_view()
        }
    };

    // Clickable targets depend on phase and the player's chosen build mode.
    let view_for_overlay = v.clone();
    let overlay = move || {
        let v = &view_for_overlay;
        let board = &v.board;
        let my_turn = v.seat == Some(v.current);
        if !my_turn {
            return ().into_any();
        }
        let mode = app.ui.mode.get();
        let (settle, city, road, tiles): (&[usize], &[usize], &[usize], &[usize]) = match &v.phase {
            Phase::Setup { .. } => (&v.legal.settlements, &[], &v.legal.roads, &[]),
            Phase::RoadBuilding { .. } => (&[], &[], &v.legal.roads, &[]),
            Phase::MoveRobber { .. } => (&[], &[], &[], &v.legal.robber_tiles),
            Phase::Main => match mode {
                Mode::Settlement => (&v.legal.settlements, &[], &[], &[]),
                Mode::City => (&[], &v.legal.cities, &[], &[]),
                Mode::Road => (&[], &[], &v.legal.roads, &[]),
                Mode::None => (&[], &[], &[], &[]),
            },
            _ => (&[], &[], &[], &[]),
        };

        let tile_targets = tiles
            .iter()
            .map(|&ti| {
                let t = &board.tiles[ti];
                let pts = t
                    .vertices
                    .iter()
                    .map(|&vi| {
                        let p = board.vertices[vi].pos;
                        format!("{},{}", f(p.0), f(p.1))
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                let victims = robber_victims(v, ti);
                view! {
                    <polygon points=pts class="target tile-target"
                        on:click=move |_| {
                            if victims.len() > 1 {
                                app.ui.robber_tile.set(Some(ti));
                            } else {
                                send(app, &ClientMsg::Act(Action::MoveRobber { tile: ti, victim: victims.first().copied() }));
                            }
                        } />
                }
            })
            .collect_view();

        let edge_targets = road
            .iter()
            .map(|&ei| {
                let e = &board.edges[ei];
                let (a, b) = (board.vertices[e.a].pos, board.vertices[e.b].pos);
                view! {
                    <line x1=f(a.0) y1=f(a.1) x2=f(b.0) y2=f(b.1) class="target edge-target"
                        on:click=move |_| {
                            send(app, &ClientMsg::Act(Action::BuildRoad { edge: ei }));
                            app.ui.mode.set(Mode::None);
                        } />
                }
            })
            .collect_view();

        let vertex_targets = settle
            .iter()
            .map(|&vi| (vi, false))
            .chain(city.iter().map(|&vi| (vi, true)))
            .map(|(vi, is_city)| {
                let (x, y) = board.vertices[vi].pos;
                view! {
                    <circle cx=f(x) cy=f(y) r="0.24" class="target vertex-target"
                        on:click=move |_| {
                            let a = if is_city {
                                Action::BuildCity { vertex: vi }
                            } else {
                                Action::BuildSettlement { vertex: vi }
                            };
                            send(app, &ClientMsg::Act(a));
                            app.ui.mode.set(Mode::None);
                        } />
                }
            })
            .collect_view();

        view! { <g>{tile_targets}{edge_targets}{vertex_targets}</g> }.into_any()
    };

    view! {
        <svg class="board" viewBox="-5.6 -5.0 11.2 10.0">
            <defs inner_html=DEFS />
            <rect x="-5.6" y="-5.0" width="11.2" height="10.0" fill="#2f6f8f" rx="0.4" />
            {ports}
            {tiles}
            {ember_under}
            {roads}
            {buildings}
            {robber}
            {ember_over}
            {overlay}
        </svg>
    }
}

/// Opponents with a building on `tile` that have cards to steal.
pub fn robber_victims(v: &GameView, tile: usize) -> Vec<usize> {
    let mut out: Vec<usize> = Vec::new();
    for &vi in &v.board.tiles[tile].vertices {
        if let Some(b) = v.buildings[vi] {
            if Some(b.owner) != v.seat && !out.contains(&b.owner) && v.players[b.owner].hand_count > 0 {
                out.push(b.owner);
            }
        }
    }
    out
}
