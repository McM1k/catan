# Colonists

A multiplayer, browser-based Catan-style board game written in Rust.

| Crate    | What it is |
|----------|------------|
| `engine` | Pure rules engine + shared types (board, game, wire protocol). No I/O. |
| `server` | Axum server: rooms, WebSocket protocol, per-player filtered views, serves the client. |
| `client` | Leptos (CSR/WASM) front end: lobby, SVG board, hand, trading, dev cards. |

The server is authoritative. Clients send `Action`s; the server validates them with
the engine and broadcasts a personalised `GameView` to each seat (other players'
hands and development cards are never sent).

## Run it

You need Rust, the WASM target and Trunk:

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk --locked

cd client && trunk build --release && cd ..   # outputs client/dist
cargo run -p server --release                  # http://localhost:3000
```

Open the page in several browser windows (or on several machines), create a room in
one, and join with the 4-letter code in the others. Refreshing the page rejoins your
seat automatically (token kept in `localStorage`).

Env vars: `PORT` (default 3000), `STATIC_DIR` (default `client/dist`).

Development with live reload: run `cargo run -p server` and, in `client/`,
`trunk serve` (its `Trunk.toml` proxies `/ws` to the server on port 3000).

## Tests

```sh
cargo test --workspace
```

* `engine`: board shape, setup order, production, robber/discard, trading, ports,
  longest road (incl. cut roads), largest army, dev cards, winning, hidden info.
* `client`: native render tests that build real game states and render every screen
  to HTML to catch panics. They don't replace testing in a browser.

## Game menu

The first screen is a game picker (`client/src/menu.rs`). Colonists opens the usual
name / create / join screen; "← All games" and leaving a game bring you back to the menu.

The Hanabi card is a plain link. Build the client with the address of your Hanabi app
and the card becomes clickable; without it the card is greyed out:

```sh
HANABI_URL=https://hanabi.example.com trunk build --release
```

To host both games from this one server instead, the Hanabi rules would have to move
into this workspace (an engine crate plus a room type in the server), then the menu
card can open an in-app screen like Colonists does.

## Look and feel

* Tiles are original SVG illustrations (`client/src/art.rs`): pine forest, clay hills
  with brick stacks, sheep meadow, wheat fields, snowy mountains, desert with cactus.
  Alternate tiles are mirrored so neighbours of one terrain differ. No image files.
* Anything that just changed on the board (a new road, settlement, city upgrade, or the
  robber's new tile) gets an ember contour that flickers and fades over 2.4 s
  (`EMBER_MS` in `client/src/state.rs`, animation in `client/style.css`). Changes are
  found by diffing each new game state against the previous one on the client, so every
  player sees them. The first state after joining or reconnecting isn't highlighted.

## Rules implemented

Standard 19-hex island with 9 ports, snake-order setup, dice production (bank
shortages handled), robber + stealing + discard on 7, roads/settlements/cities with
piece limits and the distance rule, bank/port trades, player-to-player offers
(proposer confirms with whoever accepted), development cards (Knight, Road Building,
Year of Plenty, Monopoly, Victory Point; not playable the turn bought, one per turn),
Longest Road, Largest Army, first to 10 points wins.

## Adding your tweaks

Numbers live in `engine::Rules` (`victory_points`, `discard_above`, `bank_ratio`,
`longest_road_min`, `largest_army_min`) and are passed to `Game::new` in
`server/src/main.rs`. Structural tweaks go in `engine/src/game.rs`:

* new costs / pieces: `build_*` functions and the `MAX_*` constants
* new action: add a variant to `Action`, handle it in `apply_current`
* new board layout: `Board::generate` in `engine/src/board.rs`
* anything the UI must show: add it to `GameView` (`view_for`) and render it in
  `client/src/game.rs`

Because the engine has no I/O, new rules are easy to unit-test (see the tests at the
bottom of `game.rs`).

## Known gaps

* Rooms live in memory only; a server restart ends all games.
* No chat, spectators, or turn timers; the 2–4 player limit is fixed.
* The board always uses the random standard layout.
* "Catan" is a trademark; keep your own name and artwork if you publish this.
