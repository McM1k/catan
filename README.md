# Colonists + Hanabi

Two multiplayer, browser-based games in Rust, served by one server and played from
one page: a Catan-style board game ("Colonists") and the Hanabi clone from
[McM1k/hanabii](https://github.com/McM1k/hanabii) (merged from commit `c2d5fa6`).
Both use the same rooms: one player creates a room and gets a 4-letter code, the
others join with it, the host starts the game.

| Crate         | What it is |
|---------------|------------|
| `engine`      | Colonists rules: board, game, actions, per-player views. No I/O. |
| `hanabi-core` | Hanabi rules, imported from the Hanabi repo's `game-core` (variants, "hanabii" mode, redacted per-player view). No I/O. |
| `protocol`    | The WebSocket messages shared by every game: create / join / start / leave, the lobby, and a wrapper for each game's moves and views. |
| `server`      | Axum server: the shared room system, one WebSocket per player, serves the client. |
| `client`      | Leptos (CSR/WASM) front end: game menu, the shared create/join screen, and each game's own screens. |

The server is authoritative. Clients send moves; the server validates them with the
game's rules crate and sends every seat its own filtered view (other players' hands
and development cards in Colonists, your own cards in Hanabi, are never sent).

## Run it

You need Rust, the WASM target and Trunk:

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk --locked

cd client && trunk build --release && cd ..   # outputs client/dist
cargo run -p server --release                  # http://localhost:3000
```

Open the page in several browser windows (or on several machines), pick a game, create
a room in one and join with the 4-letter code in the others. Refreshing the page puts
you back in your seat automatically (a token is kept in `localStorage`).

Env vars: `PORT` (default 3000), `STATIC_DIR` (default `client/dist`).

Development with live reload: run `cargo run -p server` and, in `client/`,
`trunk serve` (its `Trunk.toml` proxies `/ws` to the server on port 3000).

### Make sure you are building the right folder

Unzip into a new, empty folder (not on top of an older copy of the project), `cd`
into it, and check before building:

```sh
ls client/src/hanabi                      # board.rs  look.rs  mod.rs  screens.rs
grep -rn "Not connected yet" client/src   # must print nothing
```

If the `grep` prints a line, that folder still holds the old menu (the one from before
Hanabi was merged, where its card was greyed out) and a build of it will show that menu
whatever you clean or rebuild. After building, `grep -c "Not connected yet" client/dist/*.wasm`
must print `0`.

Also: if `trunk build` fails it leaves the previous `client/dist` in place, so an old
page can keep being served. Delete `client/dist` before building, so a failed build
shows up as a missing page instead.

## Rooms (the part that changed for Hanabi)

Hanabi used to have its own server with a "type a name and a room code" join screen,
where anyone typing the same code landed in the same room and any seat could start
the game. It now runs on the room system Colonists already had:

* **Create → code.** The game is picked in the menu; the server makes a free 4-letter
  code (no `I` or `O`). **Join** takes the code, and the code decides which game you
  end up in, whatever you picked in the menu.
* **Host.** The first seat is the host. Only the host starts the game, and in Hanabi
  only the host picks the variant rules (everyone sees them, locked, as they change).
* **Seats and reconnecting.** Every seat has a token kept in the browser. If the
  connection drops during a game the seat stays yours, shown as "offline" to the
  others, and a refresh or a reconnect takes it back. In a lobby, leaving frees the seat.
* **Names** are trimmed, at most 20 characters, and unique in a room (ignoring case).
* **Limits.** Colonists 2–4 players, Hanabi 2–5. Joining a running game is refused
  unless you hold its token. A room nobody has connected to for 30 minutes is removed.
* **Seat order.** Colonists shuffles the seats when the game starts. Hanabi keeps join
  order, so the host plays first and the order round the table is who joined when.

Rooms live in memory only; a server restart ends every game.

## Tests

```sh
cargo test --workspace
```

* `engine` (17): board shape, setup order, production, robber/discard, trading, ports,
  longest road (incl. cut roads), largest army, dev cards, winning, hidden info.
* `hanabi-core` (133): the original rules tests, unchanged apart from one assertion
  rewritten the way clippy prefers.
* `protocol` (6): every message survives JSON, including the Hanabi view (its maps are
  keyed by seat and color), and rule payloads from before "hanabii" existed still parse.
* `server` (25): the room flows for both games, host-only start and rules, seat order,
  fullness, reconnecting with a token, stale disconnects, moves for the wrong game, a
  whole Hanabi game to its end, and the Hanabi rule tests that used to live in the old
  server (hanabii locks the other options, rules freeze once the game starts, …).
* `client` (46): native render tests that build real games and render every screen to
  HTML: all Colonists phases, the menu, both lobbies, and the Hanabi board for every
  seat, player count and rule set, whole games rendered move by move, the clue
  buttons, rings, drop zones, game-over line, flashes. They check what the first paint
  looks like. Animations, drag and drop and the CSS need a browser (see below).

## What has not been run

The WASM target couldn't be installed in the environment this was built in, so the
client was compiled and tested natively but **never opened in a browser**. The server
was exercised over real WebSockets with a script for both games. Before relying on it,
build it with Trunk and play a round of each game, looking in particular at:

* Hanabi's hand-slide animation, drag and drop onto the play/discard zones, and the
  flashes and spinning rings (these live in effects that only run in a browser);
* the two stylesheets: Colonists' is scoped under `.theme-colonists` and Hanabi's
  (`client/hanabi.css`) under `.hanabi`, so they can't restyle each other, but that
  scoping was done mechanically;
* the tab title (it follows the game, and Hanabi's "Hanabii" mode).

## Game menu

The first screen is a picker (`client/src/menu.rs`). A card leads to that game's
create/join screen; "← All games" and leaving a game bring you back.

## Colonists

### Look and feel

* Tiles are original SVG illustrations (`client/src/art.rs`): pine forest, clay hills
  with brick stacks, sheep meadow, wheat fields, snowy mountains, desert with cactus.
  Alternate tiles are mirrored so neighbours of one terrain differ. No image files.
* 2:1 harbours are drawn in the colour of their resource, 3:1 harbours in neutral.
* Anything that just changed on the board (a new road, settlement, city upgrade, or the
  robber's new tile) gets an ember contour that flickers and fades over 2.4 s
  (`EMBER_MS` in `client/src/state.rs`, animation in `client/style.css`). Changes are
  found by diffing each new game state against the previous one on the client, so every
  player sees them. The first state after joining or reconnecting isn't highlighted.

### Rules implemented

Standard 19-hex island with 9 ports, snake-order setup, dice production (bank
shortages handled), robber + stealing + discard on 7, roads/settlements/cities with
piece limits and the distance rule, bank/port trades, player-to-player offers
(proposer confirms with whoever accepted), development cards (Knight, Road Building,
Year of Plenty, Monopoly, Victory Point; not playable the turn bought, one per turn),
Longest Road, Largest Army, first to 10 points wins.

### Adding your tweaks

Numbers live in `engine::Rules` (`victory_points`, `discard_above`, `bank_ratio`,
`longest_road_min`, `largest_army_min`) and are passed to `Game::new` in
`server/src/rooms.rs` (look for `Running::Colonists`). Structural tweaks go in
`engine/src/game.rs`:

* new costs / pieces: `build_*` functions and the `MAX_*` constants
* new action: add a variant to `Action`, handle it in `apply_current`
* new board layout: `Board::generate` in `engine/src/board.rs`
* anything the UI must show: add it to `GameView` (`view_for`) and render it in
  `client/src/game.rs`

Because the engine has no I/O, new rules are easy to unit-test (see the tests at the
bottom of `game.rs`).

## Hanabi

A cooperative card game for 2–5 players: build the fireworks in order, but you can't
see your own hand. Everything of the original is kept: the variant rules the host
picks in the lobby (multicolor, black powder, extra colors, six-card suits, the
"short" 1-of-each versions, and the "hanabii" preset), the clue buttons with their
touch preview, drag a card onto the play or discard zone, the received-clue badge, the
highlights for new cards and touched cards, the turn-ordered hands that slide to their
new place, and the fireworks that fill in as a suit is built.

Where it lives: rules in `hanabi-core`, server side in `server/src/rooms.rs`
(`Running::Hanabi`), screens in `client/src/hanabi/` (`screens.rs` for the page, create/
join and the lobby with its rules picker, `board.rs` for the game, `look.rs` for the
pure display helpers), styles in `client/hanabi.css`.

What differs from the stand-alone app:

* the room system above (host, generated codes, tokens), in place of a free-form code
  where any seat could change rules and start;
* a seat that disconnects is kept and marked "offline" instead of leaving the game;
* a "Room ABCD / Leave game" bar above the board;
* a rejoin mid-game doesn't flash every firework and the discard pile as if they had
  just changed;
* moves the rules refuse come back as a short message in a toast at the top of the page.

The wire protocol is serde's default JSON for `protocol::ClientMsg` / `ServerMsg`;
Hanabi moves are `ClientMsg::Hanabi(hanabi_core::Action)` and the game arrives as
`ServerMsg::HanabiState`.

## Known gaps

* Rooms live in memory only; a server restart ends all games.
* No chat, spectators or turn timers; a finished game ends the room's play (leave and
  create a new room for another round).
* Colonists always uses the random standard layout.
* "Catan" is a trademark; keep your own name and artwork if you publish this.
