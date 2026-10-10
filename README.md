# Catan + Hanabi + It's a Wonderful World

Three multiplayer, browser-based games in Rust, served by one server and played from
one page: Catan, the Hanabi clone from
[McM1k/hanabii](https://github.com/McM1k/hanabii) (merged from commit `c2d5fa6`), and a
card-drafting empire builder ("It's a Wonderful World", with an original placeholder card
set, see below).
All use the same rooms: one player creates a room and gets a 4-letter code, the
others join with it, the host starts the game.

| Crate         | What it is |
|---------------|------------|
| `engine`      | Catan rules: board, game, actions, per-player views. No I/O. |
| `hanabi-core` | Hanabi rules, imported from the Hanabi repo's `game-core` (variants, "hanabii" mode, redacted per-player view). No I/O. |
| `wonderful-core` | It's a Wonderful World rules: the card table, drafting, planning, production, scoring, redacted per-player view. No I/O. |
| `protocol`    | The WebSocket messages shared by every game: create / join / start / leave, the lobby, and a wrapper for each game's moves and views. |
| `server`      | Axum server: the shared room system, one WebSocket per player, serves the client. |
| `client`      | Leptos (CSR/WASM) front end: game menu, the shared create/join screen, and each game's own screens. |

The server is authoritative. Clients send moves; the server validates them with the
game's rules crate and sends every seat its own filtered view (other players' hands
and development cards in Catan, your own cards in Hanabi, the hand you are drafting
from, your pick and your drafted cards in It's a Wonderful World, are never sent to the
other seats).

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
you back in your seat automatically (a token is kept in the tab's `sessionStorage`, so
two tabs of one browser are two players).

Env vars: `PORT` (default 3000), `STATIC_DIR` (default `client/dist`).

Development with live reload: run `cargo run -p server` and, in `client/`,
`trunk serve` (its `Trunk.toml` proxies `/ws` to the server on port 3000).

### Make sure you are building the right folder

Unzip into a new, empty folder (not on top of an older copy of the project), `cd`
into it, and check before building:

```sh
ls client/src/hanabi                      # board.rs  look.rs  mod.rs  screens.rs
ls client/src/wonderful                   # board.rs  look.rs  mod.rs  screens.rs
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
the game. It now runs on the room system Catan already had:

* **Create → code.** The game is picked in the menu; the server makes a free 4-letter
  code (no `I` or `O`). **Join** takes the code, and the code decides which game you
  end up in, whatever you picked in the menu.
* **Host.** The first seat is the host. Only the host starts the game, and in Hanabi
  only the host picks the variant rules (everyone sees them, locked, as they change).
  It's a Wonderful World has no options: the host just starts.
* **Seats and reconnecting.** Every seat has a token kept in the browser tab. If the
  connection drops during a game the seat stays yours, shown as "offline" to the
  others, and a refresh or a reconnect takes it back. In a lobby, leaving frees the seat;
  a dropped connection (a reload, a phone switching apps) keeps it for 2 minutes first.
* **Names** are trimmed, at most 20 characters, and unique in a room (ignoring case).
* **Limits.** Catan 2–4 players, Hanabi 2–5, It's a Wonderful World 2–5. Joining a running game is refused
  unless you hold its token. A room nobody has connected to for 30 minutes is removed.
* **Seat order.** Catan shuffles the seats when the game starts. Hanabi keeps join
  order, so the host plays first and the order round the table is who joined when.
  It's a Wonderful World shuffles the seats like Catan; there are no turns, so the
  seat only decides who passes cards to whom, and which Empire you play.

Rooms live in memory only; a server restart ends every game.

## Tests

```sh
cargo test --workspace
```

* `engine` (17): board shape, setup order, production, robber/discard, trading, ports,
  longest road (incl. cut roads), largest army, dev cards, winning, hidden info.
* `hanabi-core` (133): the original rules tests, unchanged apart from one assertion
  rewritten the way clippy prefers.
* `wonderful-core` (55, plus one ignored): the shape of the card table, the draft (hand
  sizes, which way the hands go in each round, secret picks, the discards with two
  players), planning (build, recycle, scrap, cubes that must fit where they go),
  production (the races and their ties, the characters, the Science choice, Krystallium,
  cubes that are lost, the wrap-up step), scoring and the tie-breaks, what a view hides,
  JSON round trips, and whole games from a seed for 2 to 5 players. The ignored
  `economy_report` prints how a simple bot fares with the cards (see "The cards are a
  placeholder set" below).
* `protocol` (7): every message survives JSON, including the Hanabi and Wonderful views
  (the Hanabi maps are keyed by seat and color), and rule payloads from before "hanabii"
  existed still parse.
* `server` (34): the room flows for all three games, host-only start and rules, seat order,
  fullness, reconnecting with a token, stale disconnects, lobby seats kept through a short
  drop, moves for the wrong game, a
  whole Hanabi game and a whole Wonderful game to their end, every seat's own hand (and
  nobody else's) in Wonderful, and the Hanabi rule tests that used to live in the old
  server (hanabii locks the other options, rules freeze once the game starts, …).
* `client` (83): native render tests that build real games and render every screen to
  HTML: all Catan phases, the menu, the lobbies, the Hanabi board for every seat,
  player count and rule set, whole games rendered move by move, the clue
  buttons, rings, drop zones, game-over line, flashes; and It's a Wonderful World's
  home, waiting room and board, for every seat, phase and player count of a whole
  game, plus the pure helpers behind what a click means. They check what the first
  paint looks like. Animations, drag and drop and the CSS need a browser (see below).

## What has not been run

The WASM target couldn't be installed in the environment this was built in, so the
client was compiled and tested natively but **never opened in a browser**. The server
was exercised over real WebSockets with a script for both games. Before relying on it,
build it with Trunk and play a round of each game, looking in particular at:

* Hanabi's hand-slide animation, drag and drop onto the play/discard zones, and the
  flashes and spinning rings (these live in effects that only run in a browser);
* the two stylesheets: Catan's is scoped under `.theme-catan` and Hanabi's
  (`client/hanabi.css`) under `.hanabi`, so they can't restyle each other, but that
  scoping was done mechanically;
* the tab title (it follows the game, and Hanabi's "Hanabii" mode);
* It's a Wonderful World: the click handlers (picking a card, Build / Recycle / Scrap and
  the dialog that asks where a recycled cube goes, picking a piece in the tray and a space
  for it), the tray that sticks to the bottom of the screen, and `client/wonderful.css`
  on a real phone. Its pages were rendered natively and the HTML was looked at in
  headless Chromium at desktop and phone widths (no sideways scrolling), but nothing
  there was clicked. On the server side whole games for 2, 3 and 5 players were played
  over real WebSockets with a script, with one player's connection dropping and coming
  back with its token in each.

## Game menu

The first screen is a picker (`client/src/menu.rs`). A card leads to that game's
create/join screen; "← All games" and leaving a game bring you back.

## Catan

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
`server/src/rooms.rs` (look for `Running::Catan`). Structural tweaks go in
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

## It's a Wonderful World

A card-drafting empire builder for 2–5 players, played over four rounds, where
**everybody acts at the same time**: there are no turns, so the game only ever waits for the
slowest player. Each round has three phases.

1. **Draft.** You are dealt 7 cards (10 with two players, where the 3 cards left at the end
   are discarded). Each step you keep one and pass the rest on: to the next seat in rounds 1
   and 3, to the previous one in rounds 2 and 4. Picks are secret until everybody has picked.
2. **Planning.** Every card you kept is either **built** (it goes under construction) or
   **recycled** for the cube it gives, which goes on a building that still needs that
   resource or on your Empire. A building under construction can be **scrapped**: what was
   placed on it is lost, and you get its recycling cube.
3. **Production**, one resource after the other: Materials, Energy, Science, Gold, Exploration.
   Your Empire and your finished cards make cubes of that resource; you place them on
   buildings or on the Empire, and **cubes you don't place are lost**. Whoever makes strictly
   the most of a resource takes a character: a General for Materials or Energy, a Financier for
   Gold or Exploration, and for Science the winner chooses. A tie gives nothing. Characters
   fill the character spaces of buildings. Five cubes on the Empire make a Krystallium, a
   wildcard cube that stands in for any resource (not for a character). A last "wrap-up" step
   is there to place Krystallium and characters; it is skipped when nobody can.

A building with every space filled is finished: from then on it produces every round, and some
cards hand over a one-off bonus (a cube, a Krystallium or a character). After round 4 you score
the printed points of your finished cards, the points that depend on the cards of a type or the
characters you hold, and 1 for each General and each Financier. Ties go to whoever finished most
cards, then to whoever holds most characters, and are shared beyond that.

### The cards are a placeholder set

The published game has 150 cards, and their text wasn't available while this was built, so
**the catalogue here is an original placeholder set**: 75 designs × 2 copies = 150 cards that
have the shape of the real ones (five types, costs in cubes and characters, production,
a recycling cube, one-off bonuses, points that depend on types or characters) but none of the
numbers, names or art are the real cards'. The five Empires are placeholders too. How the set
plays has only been checked with simple bots (`cargo test -p wonderful-core economy_report --
--ignored --nocapture`), not with people.

The whole catalogue is one table, `designs()` in `wonderful-core/src/cards.rs`; to play with
other cards, replace its rows (and keep `DESIGNS` and `COPIES` in step), and the `EMPIRES`
table below it. Nothing else has to change.

### What isn't in it

Only the base game: there is no Corruption & Ascension option in the lobby (it is dropped
until its rules and cards are available), no solo play, and no turn timer.

### Where it lives

Rules in `wonderful-core` (pure and deterministic: the shuffle is a small generator seeded by
the server, so there is no `rand` and a game can be replayed from its seed); server side in
`server/src/rooms.rs` (`Running::Wonderful`); screens in `client/src/wonderful/` (`screens.rs`
for the page, create/join and the waiting room, `board.rs` for the game, `look.rs` for the pure
display helpers and what a click means, `mod.rs` for the interface state that outlives a
redraw); styles in `client/wonderful.css`, all scoped under `.wonderful`.

Moves are `ClientMsg::Wonderful(wonderful_core::Action)` and the game arrives as
`ServerMsg::WonderfulState`. Because everybody moves at once, the server checks each move
against the rules and the seat of the connection that sent it, never against a turn, and
sends every seat its own view after every move.

## Known gaps

* Rooms live in memory only; a server restart ends all games.
* No chat, spectators or turn timers; a finished game ends the room's play (leave and
  create a new room for another round).
* Catan always uses the random standard layout.
* It's a Wonderful World uses a placeholder card set and has no Corruption & Ascension
  option (see its section above).
