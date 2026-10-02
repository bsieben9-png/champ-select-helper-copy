# Testing

Four layers, fastest first. Everything except the manual checklist runs on
Linux without League.

| Layer | Command | Network | Time |
|---|---|---|---|
| Unit + LCU-mock end-to-end tests | `cd src-tauri && cargo test` | no | ~2 s (after build) |
| Lints / types | `cargo clippy --all-targets -- -D warnings` · `npm run check` | no | ~10 s |
| Live data sweep (every champion) | `cargo test live_sweep -- --ignored --nocapture` | yes | ~1.5 min (20 s cached) |
| Real-app smoke test (Linux) | `scripts/smoke/run.sh` | yes | ~1 min + build |
| Manual checklist (Windows + League) | see the end of this file | yes | ~20 min |

## 1. Unit tests

`cd src-tauri && cargo test` — parsers, role inference, import helpers, u.gg
selection logic, against the trimmed real responses in
`src-tauri/tests/fixtures/`. No network, no League. CI runs this plus clippy.

## 2. End-to-end tests against a fake League client

`cd src-tauri && cargo test e2e::` (part of the normal `cargo test`).

- `src-tauri/src/e2e/fake_lcu.rs` — a fake League client: a small HTTP server
  on 127.0.0.1 serving gameflow phase/session, current summoner, a **scripted
  champ select** (`Draft::ranked().intent(83).hover(83).lock().enemy_locks(0, 887)`,
  `Draft::blind()`, `Draft::aram(450, 83, bench)`), rune pages with the
  client's editable/deletable/current flags and a slot limit, the perks
  inventory and item sets. It **records every request**, checks the HTTP Basic
  password (401 otherwise), validates created rune pages like the client
  (4 primary rows incl. keystone in slot order, 2 secondary rows, shards, ≤ 25
  character name) and can crash mid-request (`crash_after`) or answer an error
  once (`fail_once`).
- `src-tauri/src/e2e/scenarios.rs` — drives the **real background watcher**
  (`Watcher::tick`, one tick = one 1-second poll) and the **real Tauri
  commands** in a mock Tauri app (`tauri` `test` feature), with u.gg / Data
  Dragon served offline from the fixtures. Events sent to the UI are captured.

Covered:
- Ranked: intent → hover → lock-in → enemy top locks later → user edits pages
  → finalization → game: exactly one auto-import (one rune POST, one item-set
  PUT), nothing after; never touches summoner spells / champ select.
- Champion trade after lock-in: no re-import. Blind pick: most-played role build.
- Auto-import off → zero writes. Runes + item set both off → no writes and no
  "nothing imported" event.
- Rune pages full, no `CSH:` page → nothing deleted, `needs_confirmation` =
  your current page; confirm → only that page replaced. Existing `CSH:` page →
  replaced, your pages untouched. Default pages never offered or touched.
- Item sets: your sets (unknown fields, unicode, big numbers, other champions'
  `CSH:` sets, lowercase `csh:`) preserved byte-for-byte; ours replaced, never
  duplicated; empty item-set document for new accounts.
- Dodge → new champ select → imports again, once (matchup build if the
  opponent is known at lock-in).
- ARAM, ARAM Mayhem (2400, 2450): once per champion; bench swap re-imports.
- League closes mid-select, crashes during the import, or restarts with a new
  password → no panic, status "not connected", champ select cleared. A
  dropped connection after lock-in + reconnect into the same champ select →
  no second import over the user's edits.
- One error answer after lock-in (gameflow-phase 503, champ-select session
  404) → no second import, and the UI isn't told "champ select over" (it
  would lose the "Import matchup build?" hint). League restarting mid champ
  select (phase "None" for a while, then the same champ select) → no second
  import.
- **u.gg slow or hanging** (test hook `Ugg::set_stalled`): every poll still
  returns at once and the UI keeps getting `champ-select` / `lcu-status`
  updates; the import lands once when u.gg answers, with the opponent known at
  lock-in. A build that arrives after a dodge, or once the next champ select
  started, writes nothing; ARAM swap while loading → only the new champion;
  trade while loading → nothing. A failed build fetch is retried after 15 s,
  not before. Hanging champion roles → champ select still shown (no lane
  guesses); lock-in waits up to 5 s for them.
- u.gg moves to a new patch while the app runs → champion roles reloaded
  (checked at most hourly).
- **App restart** mid champ select after the import (marker file
  `last_import.json`) → no second import; also when auto-import was off at
  lock-in. A different champ select after a restart imports once.
- Every UI `invoke()` call with the exact argument names of `src/lib/api.ts`
  through Tauri's IPC layer (catches camelCase/snake_case mismatches, e.g.
  `overwritePageId`).

The fake client gives every new champ select its own `gameId` (as League
does); the watcher uses it to tell a new champ select from the same one seen
again.

## 3. Live data sweep

```sh
cd src-tauri
cargo test live_sweep -- --ignored --nocapture
```

For **every** Data Dragon champion: the general build, then for each role u.gg
has data for: the role build, counters and matchups; plus every role's tier
list, 30 random matchup builds and 20 ARAM / ARAM Mayhem builds. Checks: no
errors; rune pages have 6 perks + 3 shards, styles exist, keystone + one per
primary row, 2 secondary perks from different rows, shards valid for their
row; spells and items exist in Data Dragon; every rate in [0, 1]; skill
letters Q/W/E/R; counters/tier lists sorted, no duplicates, known champions;
Mayhem augments named with a valid rarity. Prints every problem with
champion/role and a summary.

Env: `CSH_SWEEP_CACHE` (default `<tmp>/csh-live-sweep`, reused),
`CSH_SWEEP_LIMIT=10` (quick run), `CSH_SWEEP_CONCURRENCY` (default 4),
`CSH_SWEEP_SEED` (repeat a run's random picks).

Last run (2026-10-02, patch 16_19): 173 champions, 1088 builds, 865 counter
and 865 matchup lists, 6 tier lists — **0 problems**. 170 of 173 u.gg rune
pages list perks out of slot order (now reordered on import).

Other live checks: `cargo test live_ -- --ignored --nocapture`.

## 4. Real-app smoke test (Linux)

Builds the actual app, runs it on a virtual screen and clicks through it with
WebDriver against live u.gg data (League not running):

```sh
sudo apt install xvfb imagemagick webkit2gtk-driver
cargo install tauri-driver --locked
scripts/smoke/run.sh            # SKIP_BUILD=1 to reuse the last build
```

Steps: app starts and loads champion data (Live shows "League client isn't
running") → Lookup: Yorick build → Yorick vs Gwen matchup build → every
item/spell/rune icon of that build loads → Who beats Yorick → Matchups → Tier
list → ARAM Mayhem build with augments → Pool: add two champions → Settings:
change rank + minimum games → settings file written → **relaunch** → settings
and pool still there. Screenshots: `docs/screenshots/real-*.png`. Your own
settings file is moved aside and restored. The app log is in
`src-tauri/target/smoke-app.log`.

## 5. Manual checklist (Windows PC with League)

Takes about 20 minutes. Write down anything that looks wrong, with a
screenshot and the time.

**Before you start**
1. Start League and log in. Start Champ Select Helper.
2. Top right should say you're connected (with your name). If it says "League
   not running" for more than 10 seconds, write that down.
3. Settings: Auto-import **on**, Import rune page **on**, Import item set **on**.

**A. Normal game (Draft or Ranked)**
1. Queue up. In champ select, hover a champion (don't lock yet).
   The app shows the build. Nothing is imported yet.
2. Lock in. Within ~2 seconds a message says runes and item set were imported.
3. In the client: the selected rune page is called `CSH: <Champion> <Role>`
   (or `CSH: <Champion> vs <Enemy>`) and looks complete (no red warning).
4. Your summoner spells did **not** change.
5. Change one rune on that `CSH:` page and save. Wait 30 seconds. It must stay
   the way you changed it.
6. When the enemy laner locks in later, the app shows "<Enemy> locked in —
   Import matchup build?". Nothing changes unless you click it.
7. In game, open the shop: the item set `CSH: …` is there, next to your own sets.

**B. Rune pages full**
1. Make sure every rune page slot is used by your own pages and none is called
   `CSH:` (delete the `CSH:` page if needed).
2. Lock in a champion. The app asks whether to replace your current page.
3. Click **Cancel**: none of your pages change.
4. Next game, click **Overwrite**: only that one page is replaced; your others stay.

**C. Dodge**
If a champ select ends early (someone dodges), the next champ select imports
once again when you lock in.

**D. ARAM / ARAM Mayhem**
1. Join an ARAM or ARAM Mayhem game. When champ select starts, the app imports
   `CSH: <Champion> ARAM` without you doing anything.
2. Reroll or swap with the bench: it imports for the new champion.
3. ARAM Mayhem: the augment list stays visible in the app during the game.

**E. Item sets**
In the client (Collection → Items, or the in-game shop) your own item sets are
all still there, and there is only one `CSH:` set per champion.

**F. Client closes**
During champ select, close League completely. The app says "League not
running" and keeps working (Lookup still works). Start League again: the app
reconnects by itself within ~10 seconds.

**G. Lookup with League closed**
Close League. In the app: Lookup → pick a champion and an opponent → Build,
Who beats …, Matchups, Tier list all show data. Settings → change the rank,
close the app, open it again: the setting is kept.
