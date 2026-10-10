# It's a Wonderful World: design notes

What the README doesn't say: the rule calls that were made without the rulebook, why the
code is shaped the way it is, what is known to be wrong, and what is left. The README's
"It's a Wonderful World" section has the rules as played and where the code lives.

Status (2026-10-10): the base game runs end to end in tests, from the lobby to the final
standings, for 2 to 5 players. **It has never been played in a browser.** The cards and the
Empires are the published base game's (side A of the Empires), taken from Game Park's online
version ([gamepark/its-a-wonderful-world](https://github.com/gamepark/its-a-wonderful-world),
`rules/src/material/Developments.ts` and `Empires.ts`).

## Rules: what is confirmed and what was chosen

Confirmed against a third-party rules summary (Ultraboardgames): four rounds of draft,
planning and production; 7 cards per hand, or 10 with two players, where the 3 left over are
discarded without their recycling bonus; hands pass to the next seat in rounds 1 and 3 and
to the previous one in rounds 2 and 4; a recycled card's cube goes on a building or the
Empire; five Empire cubes make one Krystallium, which replaces any resource cube but never a
character; the strictly highest producer takes the character; the score is printed points,
type combos and 1 per character; Krystallium and unfinished cards score nothing; ties go to
most finished cards, then most characters, then are shared.

Confirmed against Game Park's rules code and Board Game Arena's game help: Materials and Gold
give a Financier, Energy and Exploration a General, Science the winner's choice (both sources
agree; the first version of this engine had Materials and Exploration the wrong way round);
a tie for the most gives nothing; production is counted at the start of each step;
construction bonuses are only characters and Krystallium, kept until used; some cost spaces
take Krystallium and nothing else; Krystallium and characters can be placed in Planning and in
any production step; a scrapped building's recycling cube goes on the Empire.

Where the engine fills a gap or picks a convenience:

| Topic | What the engine does | Status |
|---|---|---|
| Wrap-up step | A sixth production step after Exploration to place Krystallium and characters still held. Skipped when nobody holds one that fits a free space. | Not in the published game, where these can be placed during any step before you end it. Needed here because a player whose cubes are all placed is done with a step at once. |
| Scrap | Allowed in Planning or any production step. Everything on the building is lost, Krystallium and characters included; its recycling cube goes on the Empire. | Follows the rules summaries (Ultraboardgames, BGA). Game Park gives Krystallium and characters back, and treats a card drafted this round as if it were being recycled. |
| Empires | Side A of the five base Empires, each with its production and its end-game bonus. Seat *i* plays Empire *i*; seats are shuffled at Start. | Side B (the same production for everybody, no bonus) would need a per-game lobby option. |
| New cards producing | Production is counted at the start of each step: a card finished in step *k* first produces in step *k+1*; one finished in Planning produces in Materials the same round. | Confirmed (Game Park). |
| Ending a step | A player is done when their pool is empty and no choice is pending, or when they press Done (which drops unplaced cubes). A step where nobody has anything to place is skipped. | Pace: nobody waits on an empty step. |

Edge cases:

* Everyone producing zero of a resource means no supremacy and no log line; a tie for the
  most also gives nothing.
* The Science winner must choose General or Financier before Done is accepted.
* The tie-break counts unspent characters only; one placed on a building no longer counts.
* The deck is never reshuffled: five players use 140 of the 150 cards.
* The round counter stays at 4 when the game is over.

## Why the code is shaped this way

The shape copies Hanabi's: a pure rules crate, three protocol additions, one `Running` arm on
the server, one module on the client. Where nothing was specified, the nearest existing game
was copied: Hanabi for the crate and protocol, Catan for the screen lifecycle, Hanabi's
scoped stylesheet for the CSS.

**Rules crate.** `wonderful-core` depends on serde alone. The only randomness is the opening
shuffle, so the server passes a seed and `apply` takes no RNG (Catan differs because it
rolls dice mid-game). The shuffle is a SplitMix64 in `state.rs` rather than `rand`, which keeps
the crate light for the protocol and client and makes any game replayable from its seed; most
tests rely on that. Only `View` is serializable, not `State`, since rooms live in memory.

**The engine moves the game on, not the server.** Every accepted move ends in `settle()`,
which keeps advancing while everyone is done with the current step (next draft pick, Planning
to Production, next step, next round, the end). The server needs no timers, no "next" message
and no phase logic: it applies a move and broadcasts. The price is that nothing hurries a slow
player.

**Hidden information stays in `State`.** Each client gets only `view_for(seat)`. Other players
appear as counts; a draft step's picks are revealed together when the last one arrives. The
view carries each player's production per resource and current points so the client doesn't
redo production or scoring.

**Server.** The seat comes from the connection's token, never from the message. A refused move
sends an error to the sender only. An accepted one sends every connected seat its full view
(not a diff), so a reconnect is just a rejoin. Seats are shuffled at Start (like Catan,
unlike Hanabi) because seat order decides passing direction and Empire; after Start, seat 0
need not be the host.

**Protocol.**

* `ClientMsg::Wonderful(Action)` wraps the crate's own `Action`, as the other games do.
* `ServerMsg::WonderfulState { view, names, connected }`: names and connection flags travel
  beside the view, because the rules crate knows nothing about people.
* Cards travel as `CardId`s and each side looks them up in its own copy of the catalogue, so
  server and client must be built from the same one.
* Views contain no maps (vectors and arrays indexed by seat or resource), because JSON keys
  must be strings.
* No lobby options: `Lobby.rules` and `SetRules` are typed for Hanabi, so a Wonderful lobby
  sends `rules: None` and answers `SetRules` with "This game has no options to set." If
  Corruption & Ascension ever needs an option, the rules payload has to become per-game.

**Client.**

* The screen is rebuilt on every message: `Screen::WonderfulGame` carries the view, as
  Catan does, rather than Hanabi's signals (which exist to protect animations Wonderful
  doesn't have). Interface state that must outlive a rebuild (the piece picked in the tray, a
  recycled card waiting for a target) lives in `App.wonderful` and is re-checked against each
  new view: `effective_held` falls back to the first piece held when the chosen one is gone,
  and `CubeFrom::valid` hides the dialog once its card is gone.
* Browser code is kept thin. What a click means is decided by pure functions in `look.rs`
  with native tests (`pick_for_space`, `pick_for_empire`, `cube_targets`); the closures in
  `board.rs` only call `act(action)`.
* Placing: clicking a free space uses the plain piece for it (that resource's cube, a
  General, a Financier, Krystallium on a Krystallium space); picking a piece in the tray first
  overrides that. On a resource's space Krystallium is used only when picked, since it fits
  anything and is easy to waste. Recycle opens a dialog for the cube's target, because the
  target is part of the move; Scrap's dialog only confirms, its cube always goes on the Empire.
* `client/wonderful.css` scopes every rule under `.wonderful`; generic element rules sit in
  `:where()` so the prefix adds no specificity. Checked rendered (not clicked) at 1280, 390,
  360 and 320 px; below 560 px the standings table stacks, because a scrolling table hid the
  Total column. The only change to the other games' look: the menu's max width went from
  760 to 1000 px so three cards fit in a row.

## Known issues

* **"Leave game" strands the seat.** The client forgets its token, but the server keeps the
  seat, so nobody can take it back and everyone waits at the next step. The room is cleared
  30 minutes after the last player disconnects. The other games stall the same way, but only
  on that player's turn.
* **An offline player blocks everyone.** Nothing plays for a dropped seat and the server has
  no timer or kick. A host-only "skip this seat" or a timeout would both need the engine to
  play a step on a seat's behalf, which it can't do today. Ask before building either.
* **Only bots have played it.** The ignored `economy_report` test has two simple bots finish
  about 8 cards and 15 to 30 points with the real cards; people score far more.
* **Dialog accessibility.** The recycle/scrap dialog has `role="dialog"` and `aria-modal`
  but no Escape key and no focus handling.

## Left to do, in order

1. **Play it in a browser** (checklist below). Everything after assumes it works. Look at
   the long names ("Gardens of the Hesperides") and the Krystallium spaces in particular.
2. **Dialog:** Escape closes it, focus moves in on open and back on close.
3. **Absent players:** decide what should happen (see Known issues).
4. **Lobby options:** Empire side B, then the expansions (Corruption & Ascension, War or
   Peace), all of which need a per-game lobby rules payload; the expansions also need their
   cards and rules (Game Park has both). Not started. Solo mode isn't planned either.

### The browser pass

* Menu → It's a Wonderful World. Create a room in one tab, join from a second tab or a phone,
  start. Two players is quickest.
* Draft: click a card; the heading becomes "Your pick is in". The next hand arrives when
  everyone has picked, passing to the next seat in rounds 1 and 3, the previous in 2 and 4.
* Planning: Build and "Recycle for" on each card. Recycling opens a dialog listing buildings
  that still need that cube, plus the Empire. "Done planning" stays disabled until every card
  is decided. Try Scrap on a building (its dialog only confirms: the cube goes to the Empire).
* Production: the step bar and race list. Click a free space (it lights up when what you hold
  fits). Pick another piece in the tray first to override. Try "Put on your Empire" and
  "Done: drop N cubes". A Science win shows "Take a General" / "Take a Financier" and blocks
  Done until you choose. Wrap-up appears only when someone can use it.
* Reload mid-game: you return to your seat with your own cards.
* A real phone: the sticky tray, tap targets, stacked standings, fonts.
* Other players' moves while you scroll or have the dialog open: does the page jump, does the
  dialog stay?

## Traps

* **Four bots drive the whole-game tests**: `bot` and `smart_bot` in
  `wonderful-core/src/tests.rs`, `wonderful_move` in `server/src/rooms.rs`, `next_move` in
  `client/src/wonderful/board.rs`. After a rules change a bot may stall or loop instead of
  failing clearly. Suspect the bots first.
* **Every new kind of move must go through `apply`**, which ends in `settle()`. Anything else
  can leave the game with everyone done and nobody advancing.
* **Anything added to `PlayerView` goes to every seat.** The hidden-info tests
  (`wonderful_hides_picks_until_everybody_has_picked`, `nothing_of_the_others_hands_is_drawn`)
  only catch what they know to look for.
* **Render tests are substring checks** on Leptos's server-side markup (attribute order, `<!>`
  between text nodes). After a markup tweak a failure is more often a stale assertion than a
  bug. Scope assertions to an element: the race line says "takes a General", so a bare
  `contains("General")` passes for the wrong reason.
* **A page-level overflow check misses inner scrollers**: the phone standings table hid its
  Total column inside a scroll box while the page never scrolled sideways.
