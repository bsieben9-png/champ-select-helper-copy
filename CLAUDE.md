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

## Status / where we left off

Update this section at the end of every work session so work can continue
on another machine (PC ⇄ cloud).

- [x] Repo, design doc, shared contract, skeleton (cloud session, 2026-10-02)
- [ ] u.gg + Data Dragon data layer (`ugg.rs`, `ddragon.rs`)
- [ ] League client connection + import + auto-import (`lcu.rs`, `watcher.rs`)
- [ ] UI (`src/`)
- [ ] Windows CI build + README
- [ ] ARAM Mayhem data (u.gg has it, file location not found yet — page is server-rendered)
