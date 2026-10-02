# Hand-off: where the work stands

Last updated: 2026-10-01 7:45 PM MT (cloud session).

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
