# Hand-off: where the work stands

Last updated: 2026-10-02 1:30 AM MT (cloud session). Current release: v0.1.1 (portable .exe).

## Done (merged on `main`)
- Design (`DESIGN.md`), shared contract (`model.rs` / `types.ts`), Tauri + Svelte skeleton
- Data layer: u.gg builds / matchup builds / counters / matchups / tier list, Data Dragon static
  data, disk + memory cache, patch-day fallback (`ugg.rs`, `ddragon.rs`, `http_cache.rs`)
- League client: discovery, champ select parsing + enemy role inference, rune page + item set
  import (only `CSH:` pages; asks before overwriting a user page), auto-import ONCE at lock-in,
  spells never touched (`lcu.rs`, `watcher.rs`)
- CI: Linux tests + Windows installer build on every push; release workflow; cloud SessionStart hook
- **UI merged** (Live / Lookup / Pool / Settings, mock mode: `npm run dev` + `?mock=champselect|counters|blind|locked|fullpages|late|mayhem|ingame|lobby|disconnected`, `?view=lookup|pool|settings`). Screenshots in `docs/screenshots/`.
- 62 Rust tests pass; `npm run check` + `npm run build` clean

## In progress (NOT on main yet): snapshots pushed as branches
- ~~`wip/ui`~~: MERGED into main (ignore this branch). Original notes: the Svelte UI (Live / Lookup / Champion Pool / Settings, League-style theme,
  mock mode via `npm run dev`). It was still being built: finish it, check that
  `npm run build` and `npm run check` pass, then merge it into `main`. It must include: rune page
  overwrite confirmation modal (`ImportResult.needs_confirmation`, `import_build(build, overwritePageId)`),
  tier list when lane opponent unknown (`get_tier_list`), "Import matchup build?" hint when the
  opponent appears after auto-import, source shown as "via u.gg", **ARAM Mayhem** queue +
  **Augments** section (`Build.augments`), Mayhem build kept visible while the game is InProgress.
- ~~`wip/mayhem`~~: MERGED into main (ignore the branch). Mayhem builds come from u.gg's normal_aram file (that's what u.gg's own Mayhem page uses); augments are a per-rarity ranking from `static.bigbrain.gg/custom-aram-mayhem/` (no win rates published). See DESIGN.md "ARAM Mayhem". Original notes: Goal: find u.gg's (or
  Lolalytics / OP.GG) ARAM Mayhem build + augment data, add `AugmentOption` + `Build.augments`
  (shape in `docs/HANDOFF.md` → below), map LCU queue 2400 (+2450) → Mayhem data. Leads are in CLAUDE.md.
  `AugmentOption { id, name, icon, rarity, description, games, win_rate, pick_rate }`.
  Augment icons will need the CSP `img-src` in `tauri.conf.json` extended to their host.

- **Code review + end-to-end tests** were started at 7:40 PM MT (cloud). If their results aren't on
  `main`, check the branches `wip/review` and `wip/tests`; otherwise redo them: (a) adversarial review of
  rune-page/item-set safety, the once-at-lock-in rule, and the UI<->backend command/arg/event names;
  (b) a fake League client (LCU mock) test suite, a live u.gg sweep over all champions, and a real-app
  smoke test under xvfb; write `docs/TESTING.md` with a manual Windows + League checklist.

- **Code review: MERGED.** 7 bugs fixed (re-import after a client hiccup, toggling auto-import on
  after lock-in, item-set wipe on an odd GET, perk order for the client, static data needing u.gg,
  wrong queue in the overwrite dialog, Import enabled while loading). Leftovers worth doing:
  the watcher's u.gg calls can stall champ-select updates for 8-15 s when u.gg hangs (move build
  fetching off the poll loop); champion
  roles never reload after a patch change; "already imported" isn't persisted across app restarts.

- **End-to-end tests: MERGED** (`src-tauri/src/e2e/`: fake League client + scenarios; live sweep of all
  173 champions = 0 problems; real-app smoke test `scripts/smoke/run.sh` 13/13; see `docs/TESTING.md`).
  The bug it found (one error reply from League after lock-in caused a second import) is FIXED:
  the import memory is only forgotten after 3 polls in a row outside champ select. 90 tests pass.

- **OWNER PRIORITY (8:45 PM MT): the app working properly > new features. Modes that matter:
  Ranked, Normals (Quickplay/Swiftplay/Draft), ARAM Mayhem.** Lolalytics + augment descriptions are
  PAUSED (not wanted now).
- **In progress (cloud):** `wip/watcher` (champ select never stalls on a slow u.gg, roles reload
  after a patch, no re-import after an app restart) and `wip/modes` (end-to-end correctness for
  Ranked / Normals incl. Quickplay's lobby champion picks / ARAM Mayhem bench+rerolls; per-mode
  manual checklist in docs/TESTING.md) and `wip/efficiency` (measure + cut memory/CPU/startup/IPC/
  network/exe size; owner wants it as lightweight as possible) and `wip/security` (security review +
  anything that looks suspicious to antivirus/SmartScreen; `docs/SECURITY.md`). If not merged on main, finish them
  from those branches.

- **OWNER PRIORITIES (2026-10-02): #1 Riot/Vanguard/League compliance (don't get banned), #2 app
  security (incl. antivirus/SmartScreen), then reliability/modes, then efficiency.**
  DONE: League is found via its lockfile only (commit 3230aec): no process list, handles, command
  line or memory reads; sysinfo removed. Released as v0.1.1.
  RUNNING (cloud): `wip/compliance` (Riot policy audit → docs/COMPLIANCE.md, then security →
  docs/SECURITY.md), `wip/watcher`, `wip/modes` (resumed after the usage cap). PAUSED: `wip/efficiency`.
  Work-saving: one or few agents at a time, commit every step, 2-minute snapshots to wip/*.

## Next steps after that
1. Merge the review/test results (branches `wip/review`, `wip/tests` if not on main yet). Optional: fill augment descriptions/pick rates from OP.GG's Mayhem page (sample in `src-tauri/tests/fixtures/other_sources/opgg_mayhem_augments_83.json`).
2. Test on the Windows PC with League running (see the list in the LCU agent notes below).
3. Lolalytics as a second source behind a source dropdown (CLAUDE.md "Ideas for later").

## Things only a real PC + League can verify
TLS to the client; reading the client's command line (League running as admin → lockfile fallback);
own-team hover field; ARAM/Mayhem pick actions; "rune pages full" detection; 25-char page name
limit; item set PUT accepted; blind-pick enemy team shape.

## Owner decisions (summary; full table in DESIGN.md)
Auto-import on/off, ONCE at lock-in only, never re-imports. Spells are recommendation only. Rune
pages full → ask first. Blind pick → tier list for my role. Emerald+ / World. ARAM = **Mayhem only**
(augments matter most). Multi-source later as a dropdown (Lolalytics first).
