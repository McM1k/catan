use crate::board::{board_svg, robber_victims, PLAYER_COLORS};
use crate::state::{leave, send, App, Mode, Picker};
use protocol::ClientMsg;
use engine::{Action, DevCard, GameView, Hand, Phase, Resource, SetupExpect};
use leptos::prelude::*;

fn act(app: App, a: Action) {
    send(app, &ClientMsg::Catan(a));
}

fn sum(h: [u8; 5]) -> u32 {
    h.iter().map(|&n| n as u32).sum()
}

fn status_text(v: &GameView) -> String {
    let cur = v.players[v.current].name.clone();
    let mine = v.seat == Some(v.current);
    match &v.phase {
        Phase::Setup { expect, .. } => {
            let what = match expect {
                SetupExpect::Settlement => "settlement",
                SetupExpect::Road => "road",
            };
            if mine {
                format!("Your turn: place a {what}.")
            } else {
                format!("{cur} is placing a {what}…")
            }
        }
        Phase::Roll => {
            if mine {
                "Your turn — roll the dice.".into()
            } else {
                format!("{cur} is about to roll.")
            }
        }
        Phase::Main => {
            if mine {
                "Build, trade, or end your turn.".into()
            } else {
                format!("{cur} is taking their turn.")
            }
        }
        Phase::Discard { pending } => {
            let names: Vec<String> = pending.iter().map(|(s, _)| v.players[*s].name.clone()).collect();
            format!("A 7! Waiting for {} to discard.", names.join(", "))
        }
        Phase::MoveRobber { .. } => {
            if mine {
                "Move the robber: click a tile.".into()
            } else {
                format!("{cur} is moving the robber.")
            }
        }
        Phase::RoadBuilding { left } => {
            if mine {
                format!("Road Building: place {left} more road(s).")
            } else {
                format!("{cur} is building roads.")
            }
        }
        Phase::Finished { winner } => format!("{} wins the game!", v.players[*winner].name),
    }
}

pub fn game_screen(app: App, v: GameView, connected: Vec<bool>, room: String) -> impl IntoView {
    let board = board_svg(app, &v);
    let status = status_text(&v);
    let dice = v
        .dice
        .map(|(a, b)| format!("🎲 {a} + {b} = {}", a + b))
        .unwrap_or_default();
    let my_vp = v.me.as_ref().map(|m| m.vp).unwrap_or(0);
    let modal_view = v.clone();

    view! {
        <div class="game">
            <header class="topbar">
                <div class="brand">"Catan"</div>
                <div class="room">"Room " <b>{room}</b></div>
                <div class="status">{status}</div>
                <div class="dice">{dice}</div>
                <div class="myvp">"Your points: " <b>{my_vp}</b> " / " {v.rules.victory_points}</div>
                <button class="link" on:click=move |_| leave(app)>"Leave"</button>
            </header>
            <div class="layout">
                <div class="board-wrap">{board}</div>
                <aside class="sidebar">
                    {players_panel(&v, &connected)}
                    {hand_panel(app, &v)}
                    {actions_panel(app, &v)}
                    {trade_panel(app, &v)}
                    {log_panel(&v)}
                </aside>
            </div>
            {move || modals(app, &modal_view)}
        </div>
    }
}

fn players_panel(v: &GameView, connected: &[bool]) -> impl IntoView {
    let rows = v
        .players
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let is_cur = i == v.current && !matches!(v.phase, Phase::Finished { .. });
            let you = v.seat == Some(i);
            let online = connected.get(i).copied().unwrap_or(true);
            view! {
                <div class="player" class:current=is_cur>
                    <span class="swatch" style=format!("background:{}", PLAYER_COLORS[i % 4]) />
                    <div class="pname">
                        {if is_cur { "▶ " } else { "" }}
                        {p.name.clone()}
                        {you.then(|| view! { <span class="badge">"you"</span> })}
                        {(!online).then(|| view! { <span class="badge off">"offline"</span> })}
                        {(v.longest_road == Some(i)).then(|| view! { <span class="badge gold">"Longest Road"</span> })}
                        {(v.largest_army == Some(i)).then(|| view! { <span class="badge gold">"Largest Army"</span> })}
                    </div>
                    <div class="pstats">
                        <span title="Victory points (not counting hidden cards)">{format!("{} VP", p.public_vp)}</span>
                        <span title="Resource cards">{format!("🂠 {}", p.hand_count)}</span>
                        <span title="Development cards">{format!("★ {}", p.dev_count)}</span>
                        <span title="Knights played">{format!("⚔ {}", p.knights)}</span>
                        <span title="Longest road">{format!("🛣 {}", p.road_length)}</span>
                    </div>
                </div>
            }
        })
        .collect_view();
    view! { <section class="panel"><h3>"Players"</h3>{rows}</section> }
}

fn hand_panel(app: App, v: &GameView) -> AnyView {
    let Some(me) = &v.me else { return ().into_any() };

    let chips = (0..5)
        .map(|i| {
            view! {
                <div class=format!("hand-card res-{i}")>
                    <div class="n">{me.hand.0[i]}</div>
                    <div class="l">{Resource::ALL[i].name()}</div>
                </div>
            }
        })
        .collect_view();

    let can_play_now = !v.dev_played && v.seat == Some(v.current);
    let in_main = matches!(v.phase, Phase::Main);
    let in_roll = matches!(v.phase, Phase::Roll);

    let kinds = [
        (DevCard::Knight, "Knight"),
        (DevCard::RoadBuilding, "Road Building"),
        (DevCard::YearOfPlenty, "Year of Plenty"),
        (DevCard::Monopoly, "Monopoly"),
        (DevCard::VictoryPoint, "Victory Point"),
    ];
    let dev_rows = kinds
        .iter()
        .filter_map(|&(kind, label)| {
            let playable = me.dev.iter().filter(|c| **c == kind).count();
            let fresh = me.new_dev.iter().filter(|c| **c == kind).count();
            if playable + fresh == 0 {
                return None;
            }
            let can_play = can_play_now
                && playable > 0
                && match kind {
                    DevCard::Knight => in_main || in_roll,
                    DevCard::VictoryPoint => false,
                    _ => in_main,
                };
            let on_play = move |_| match kind {
                DevCard::Knight => act(app, Action::PlayKnight),
                DevCard::RoadBuilding => act(app, Action::PlayRoadBuilding),
                DevCard::YearOfPlenty => {
                    app.ui.picks.set(vec![]);
                    app.ui.picker.set(Some(Picker::YearOfPlenty));
                }
                DevCard::Monopoly => app.ui.picker.set(Some(Picker::Monopoly)),
                DevCard::VictoryPoint => {}
            };
            Some(view! {
                <div class="dev-row">
                    <span>{format!("{label} ×{playable}")}
                        {(fresh > 0).then(|| view! { <span class="muted">{format!(" (+{fresh} new)")}</span> })}
                    </span>
                    {(kind != DevCard::VictoryPoint).then(|| view! {
                        <button class="small" disabled=!can_play on:click=on_play>"Play"</button>
                    })}
                </div>
            })
        })
        .collect_view();

    view! {
        <section class="panel">
            <h3>"Your hand"</h3>
            <div class="hand">{chips}</div>
            <div class="devs">{dev_rows}</div>
        </section>
    }
    .into_any()
}

fn actions_panel(app: App, v: &GameView) -> AnyView {
    let (Some(me), true) = (&v.me, v.seat == Some(v.current)) else {
        return ().into_any();
    };
    match &v.phase {
        Phase::Roll => view! {
            <section class="panel">
                <button class="primary big" on:click=move |_| act(app, Action::Roll)>"Roll dice"</button>
            </section>
        }
        .into_any(),
        Phase::Main => {
            let p = &v.players[v.current];
            let has = |items: &[(Resource, u8)]| me.hand.contains(&Hand::of(items));
            let road_ok = has(&[(Resource::Wood, 1), (Resource::Brick, 1)]) && p.roads_left > 0 && !v.legal.roads.is_empty();
            let settle_ok = has(&[
                (Resource::Wood, 1),
                (Resource::Brick, 1),
                (Resource::Sheep, 1),
                (Resource::Wheat, 1),
            ]) && p.settlements_left > 0
                && !v.legal.settlements.is_empty();
            let city_ok = has(&[(Resource::Wheat, 2), (Resource::Ore, 3)])
                && p.cities_left > 0
                && !v.legal.cities.is_empty();
            let dev_ok = has(&[(Resource::Sheep, 1), (Resource::Wheat, 1), (Resource::Ore, 1)]) && v.deck_left > 0;

            let toggle = move |m: Mode| {
                app.ui.mode.update(|cur| *cur = if *cur == m { Mode::None } else { m });
            };
            view! {
                <section class="panel">
                    <h3>"Build"</h3>
                    <div class="btn-row">
                        <button disabled=!road_ok title="1 Wood + 1 Brick"
                            class:active=move || app.ui.mode.get() == Mode::Road
                            on:click=move |_| toggle(Mode::Road)>"Road"</button>
                        <button disabled=!settle_ok title="1 Wood + 1 Brick + 1 Sheep + 1 Wheat"
                            class:active=move || app.ui.mode.get() == Mode::Settlement
                            on:click=move |_| toggle(Mode::Settlement)>"Settlement"</button>
                        <button disabled=!city_ok title="2 Wheat + 3 Ore"
                            class:active=move || app.ui.mode.get() == Mode::City
                            on:click=move |_| toggle(Mode::City)>"City"</button>
                        <button disabled=!dev_ok title="1 Sheep + 1 Wheat + 1 Ore"
                            on:click=move |_| act(app, Action::BuyDevCard)>
                            {format!("Dev card ({})", v.deck_left)}
                        </button>
                    </div>
                    <p class="muted small-text">"Costs: road = wood+brick · settlement = wood+brick+sheep+wheat · city = 2 wheat+3 ore · dev card = sheep+wheat+ore"</p>
                    <button class="primary" on:click=move |_| {
                        app.ui.mode.set(Mode::None);
                        act(app, Action::EndTurn);
                    }>"End turn"</button>
                </section>
            }
            .into_any()
        }
        _ => ().into_any(),
    }
}

fn trade_panel(app: App, v: &GameView) -> AnyView {
    let (Some(me), Phase::Main) = (&v.me, &v.phase) else {
        return ().into_any();
    };
    let seat = v.seat.unwrap();
    let my_turn = seat == v.current;

    if let Some(t) = &v.trade {
        let summary = format!(
            "{} offers {} for {}",
            v.players[t.from].name,
            describe(&t.give),
            describe(&t.want)
        );
        if t.from == seat {
            let others = v
                .players
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != seat)
                .map(|(i, p)| {
                    let accepted = t.accepted.contains(&i);
                    let declined = t.declined.contains(&i);
                    view! {
                        <div class="trade-reply">
                            <span>{p.name.clone()}</span>
                            {if accepted {
                                view! { <button class="small primary" on:click=move |_| act(app, Action::ConfirmTrade { with: i })>"Trade!"</button> }.into_any()
                            } else if declined {
                                view! { <span class="muted">"declined"</span> }.into_any()
                            } else {
                                view! { <span class="muted">"thinking…"</span> }.into_any()
                            }}
                        </div>
                    }
                })
                .collect_view();
            return view! {
                <section class="panel">
                    <h3>"Your offer"</h3>
                    <p>{summary}</p>
                    {others}
                    <button class="small" on:click=move |_| act(app, Action::CancelTrade)>"Cancel offer"</button>
                </section>
            }
            .into_any();
        }
        let affordable = me.hand.contains(&t.want);
        let accepted = t.accepted.contains(&seat);
        let declined = t.declined.contains(&seat);
        return view! {
            <section class="panel">
                <h3>"Trade offer"</h3>
                <p>{summary}</p>
                <div class="btn-row">
                    <button class="primary" class:active=accepted disabled=!affordable
                        on:click=move |_| act(app, Action::RespondTrade { accept: true })>"Accept"</button>
                    <button class:active=declined
                        on:click=move |_| act(app, Action::RespondTrade { accept: false })>"Decline"</button>
                </div>
                {(!affordable).then(|| view! { <p class="muted">"You don't have the requested cards."</p> })}
            </section>
        }
        .into_any();
    }

    if !my_turn {
        return ().into_any();
    }

    let hand_max = me.hand.0;
    let ratios = me.ratios;
    let give_row = (0..5)
        .map(|i| {
            let ok = me.hand.0[i] >= ratios[i];
            view! {
                <button class=format!("res-btn res-{i}") disabled=!ok
                    class:active=move || app.ui.bank_give.get() == Some(i)
                    on:click=move |_| app.ui.bank_give.set(Some(i))>
                    {format!("{} {}:1", Resource::ALL[i].name(), ratios[i])}
                </button>
            }
        })
        .collect_view();
    let get_row = (0..5)
        .map(|i| {
            view! {
                <button class=format!("res-btn res-{i}")
                    class:active=move || app.ui.bank_get.get() == Some(i)
                    on:click=move |_| app.ui.bank_get.set(Some(i))>
                    {Resource::ALL[i].name()}
                </button>
            }
        })
        .collect_view();

    view! {
        <section class="panel">
            <h3>"Trade with the bank"</h3>
            <div class="label">"Give"</div><div class="btn-row wrap">{give_row}</div>
            <div class="label">"Get"</div><div class="btn-row wrap">{get_row}</div>
            <button disabled=move || {
                    let (g, r) = (app.ui.bank_give.get(), app.ui.bank_get.get());
                    g.is_none() || r.is_none() || g == r
                }
                on:click=move |_| {
                    if let (Some(g), Some(r)) = (app.ui.bank_give.get_untracked(), app.ui.bank_get.get_untracked()) {
                        act(app, Action::BankTrade { give: Resource::ALL[g], get: Resource::ALL[r] });
                        app.ui.bank_give.set(None);
                        app.ui.bank_get.set(None);
                    }
                }>"Trade with bank"</button>
        </section>
        <section class="panel">
            <h3>"Offer a trade to players"</h3>
            <div class="two-col">
                <div><div class="label">"You give"</div>{hand_editor(app.ui.give, hand_max)}</div>
                <div><div class="label">"You want"</div>{hand_editor(app.ui.want, [9; 5])}</div>
            </div>
            <button disabled=move || {
                    let (g, w) = (app.ui.give.get(), app.ui.want.get());
                    sum(g) == 0 || sum(w) == 0 || (0..5).any(|i| g[i] > 0 && w[i] > 0)
                }
                on:click=move |_| {
                    let (g, w) = (app.ui.give.get_untracked(), app.ui.want.get_untracked());
                    act(app, Action::ProposeTrade { give: Hand(g), want: Hand(w) });
                    app.ui.give.set([0; 5]);
                    app.ui.want.set([0; 5]);
                }>"Offer trade"</button>
        </section>
    }
    .into_any()
}

fn describe(h: &Hand) -> String {
    let parts: Vec<String> = Resource::ALL
        .iter()
        .filter(|r| h.get(**r) > 0)
        .map(|r| format!("{} {}", h.get(*r), r.name()))
        .collect();
    parts.join(", ")
}

fn hand_editor(sig: RwSignal<[u8; 5]>, max: [u8; 5]) -> impl IntoView {
    (0..5)
        .map(|i| {
            view! {
                <div class="editor-row">
                    <span class=format!("chip res-{i}")>{Resource::ALL[i].name()}</span>
                    <button class="step" on:click=move |_| sig.update(|h| h[i] = h[i].saturating_sub(1))>"−"</button>
                    <span class="count">{move || sig.get()[i]}</span>
                    <button class="step" disabled={move || sig.get()[i] >= max[i]}
                        on:click=move |_| sig.update(|h| h[i] += 1)>"+"</button>
                </div>
            }
        })
        .collect_view()
}

fn log_panel(v: &GameView) -> impl IntoView {
    let lines = v
        .log
        .iter()
        .rev()
        .take(14)
        .map(|l| view! { <li>{l.clone()}</li> })
        .collect_view();
    view! { <section class="panel"><h3>"Log"</h3><ul class="log">{lines}</ul></section> }
}

fn modals(app: App, v: &GameView) -> AnyView {
    let Some(me) = &v.me else { return ().into_any() };
    let seat = v.seat.unwrap();

    // Winner.
    if let Phase::Finished { winner } = &v.phase {
        let name = v.players[*winner].name.clone();
        let you = *winner == seat;
        return view! {
            <div class="modal-bg"><div class="modal card">
                <h2>{if you { "🏆 You win!".to_string() } else { format!("🏆 {name} wins!") }}</h2>
                <button class="primary" on:click=move |_| leave(app)>"Back to start"</button>
            </div></div>
        }
        .into_any();
    }

    // Discard after a 7.
    if let Phase::Discard { pending } = &v.phase {
        if let Some(&(_, need)) = pending.iter().find(|(s, _)| *s == seat) {
            let need = need as u32;
            let max = me.hand.0;
            return view! {
                <div class="modal-bg"><div class="modal card">
                    <h2>"A 7 was rolled"</h2>
                    <p>{format!("You hold more than {} cards. Discard {need}.", v.rules.discard_above)}</p>
                    {hand_editor(app.ui.discard, max)}
                    <button class="primary"
                        disabled=move || sum(app.ui.discard.get()) != need
                        on:click=move |_| {
                            act(app, Action::Discard(Hand(app.ui.discard.get_untracked())));
                            app.ui.discard.set([0; 5]);
                        }>
                        {move || format!("Discard ({}/{need})", sum(app.ui.discard.get()))}
                    </button>
                </div></div>
            }
            .into_any();
        }
    }

    // Choose whom to steal from.
    if let (Phase::MoveRobber { .. }, Some(tile), true) =
        (&v.phase, app.ui.robber_tile.get(), v.seat == Some(v.current))
    {
        let buttons = robber_victims(v, tile)
            .into_iter()
            .map(|p| {
                let name = v.players[p].name.clone();
                view! {
                    <button on:click=move |_| {
                        act(app, Action::MoveRobber { tile, victim: Some(p) });
                        app.ui.robber_tile.set(None);
                    }>
                        <span class="swatch" style=format!("background:{}", PLAYER_COLORS[p % 4]) />
                        {name}
                    </button>
                }
            })
            .collect_view();
        return view! {
            <div class="modal-bg"><div class="modal card">
                <h2>"Steal from…"</h2>
                <div class="btn-row wrap">{buttons}</div>
                <button class="link" on:click=move |_| app.ui.robber_tile.set(None)>"Cancel"</button>
            </div></div>
        }
        .into_any();
    }

    // Year of Plenty / Monopoly picker.
    if let Some(kind) = app.ui.picker.get() {
        let title = match kind {
            Picker::YearOfPlenty => "Year of Plenty: pick 2 resources from the bank",
            Picker::Monopoly => "Monopoly: pick a resource to take from everyone",
        };
        let buttons = (0..5)
            .map(|i| {
                view! {
                    <button class=format!("res-btn res-{i}") on:click=move |_| match kind {
                        Picker::Monopoly => {
                            act(app, Action::PlayMonopoly(Resource::ALL[i]));
                            app.ui.picker.set(None);
                        }
                        Picker::YearOfPlenty => {
                            let mut picks = app.ui.picks.get_untracked();
                            picks.push(i);
                            if picks.len() == 2 {
                                act(app, Action::PlayYearOfPlenty(Resource::ALL[picks[0]], Resource::ALL[picks[1]]));
                                app.ui.picker.set(None);
                                app.ui.picks.set(vec![]);
                            } else {
                                app.ui.picks.set(picks);
                            }
                        }
                    }>{Resource::ALL[i].name()}</button>
                }
            })
            .collect_view();
        return view! {
            <div class="modal-bg"><div class="modal card">
                <h2>{title}</h2>
                <div class="btn-row wrap">{buttons}</div>
                <button class="link" on:click=move |_| { app.ui.picker.set(None); app.ui.picks.set(vec![]); }>"Cancel"</button>
            </div></div>
        }
        .into_any();
    }

    ().into_any()
}
