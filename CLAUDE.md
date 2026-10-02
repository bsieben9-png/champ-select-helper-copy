# Champ Select Helper — notes for Claude

Lightweight Windows app (Tauri 2 + Rust + Svelte 5/TS) for League of Legends
champ select: runes/spells/items/skill order per champion and per lane
matchup, counter-pick suggestions, auto-import into the League client.
Data comes from u.gg's unofficial JSON (see DESIGN.md) and Riot Data Dragon.

**Read DESIGN.md first** — it records the owner's decisions and the reverse-
engineered u.gg data formats.

## Commands

- `npm install` — once
- `npm run tauri dev` — run the app (Windows: needs Rust + WebView2; League client optional)
- `npm run dev` — UI only in a browser with mock data (no Rust needed)
- `npm run build` / `npm run check` — frontend build / type check
- `cd src-tauri && cargo test` — backend tests (use fixtures in `src-tauri/tests/fixtures`, no network)
- `npm run tauri build` — Windows installer (`src-tauri/target/release/bundle/`)
- CI (`.github/workflows/build.yml`) builds the Windows installer on every push.

## Conventions

- `src-tauri/src/model.rs` and `src/lib/types.ts` are the shared contract — change both together.
- Tauri command names/args are snake_case on both sides (`invoke("get_build", { championId, ... })` — Tauri converts camelCase args to snake_case).
- Rune pages / item sets the app creates are prefixed `CSH: ` — only ever modify/delete those.
- Keep it lightweight: no heavy UI frameworks, no extra runtime deps without reason.

## Ideas for later (not planned)

- **Multi-source compare** (next after the core app): adapters converted into the same `Build`
  type; "consensus" view with per-site agreement dots and game-weighted win rates (don't add
  games across sites — they share Riot's match data). Feasibility checked 2026-10-02
  (samples in `src-tauri/tests/fixtures/other_sources/`):
  - **Lolalytics — easy.** `https://a1.lolalytics.com/mega/?ep=build-full&v=1&patch=16.19&c=yorick&lane=top&tier=emerald_plus&queue=ranked&region=all`
    → JSON (header{wr,n,counters}, runes, spells, startItem/startSet, item1..item5, boots,
    skillOrder, popularItem vs winningItem, enemy). No bot wall. Matchup param unknown (`c2=` ignored).
  - **OP.GG — easy.** `https://lol-api-champion.op.gg/api/global/champions/ranked/{champId}/{lane}?tier=emerald_plus`
    → JSON (rune_pages, runes, summoner_spells, core_items, boots, starter_items, last_items,
    skills, counters). Matchup param unknown (`target_champion=` ignored). `lol-web-api.op.gg` blocked here.
  - **League of Graphs, Mobalytics — hard.** Cloudflare challenge on every page; would need a browser. Skip.
  - Numbers differ by site (Yorick top E+ 16.19: Lolalytics 51.1% / 48k games, OP.GG 49.0% / 46k).
  - Plan: add a `source` field to `Build` + a source setting; Lolalytics adapter first.
- **Patch-day fallback**: if the newest patch has too few games for a champion, use the
  previous patch until the sample grows.
- **Own scoring**: "highest win rate" option for items/runes with a minimum-games filter,
  shown alongside u.gg's pick.

## Status / where we left off

Update this section at the end of every work session so work can continue
on another machine (PC ⇄ cloud).

- [x] Repo, design doc, shared contract, skeleton (cloud session, 2026-10-02)
- [ ] u.gg + Data Dragon data layer (`ugg.rs`, `ddragon.rs`)
- [ ] League client connection + import + auto-import (`lcu.rs`, `watcher.rs`)
- [ ] UI (`src/`)
- [x] Windows CI build + README + cloud session hook (`.github/workflows/`, `.claude/hooks/`)
- [ ] ARAM Mayhem data (u.gg has it, file location not found yet — page is server-rendered).
      Leads: u.gg queue key `aram_mayhem` (LCU queue 2400) / `aram_mayhem_classic` (2450);
      `overview/{patch}/aram_mayhem/...` returns 403. u.gg JS (`static.bigbrain.gg/lol/static/js/*.js`)
      mentions `champion_overview_aram_mayhem`, `aram-mayhem-augment-manifest-{patch}.json`,
      augment icons `static.bigbrain.gg/cdragon-custom/{patch}/augments/{id}.webp`. Next step: load
      `https://u.gg/lol/champions/aram-mayhem/yorick-aram-mayhem` in a real browser and search the
      server-rendered HTML for the stats URL. Until then ARAM Mayhem falls back to normal ARAM data.
