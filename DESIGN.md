# Champ Select Helper — Design

A small, fast Windows app for League of Legends champ select. It recommends
runes, summoner spells, items and skill order, both in general and **for
your specific lane matchup** (e.g. Yorick vs Gwen). It also suggests easy
counter-picks when you see an enemy champion, before you've picked.

## Decisions (from the owner)

| Topic | Decision |
|---|---|
| Tech | **Tauri 2** (Rust backend + Svelte 5/TypeScript UI). Target: Windows 10/11 (WebView2). |
| Import into client | **Auto-import** (toggle on/off in settings) **and** a manual **Import** button. |
| Window | Normal window. |
| Distribution | **Portable `.exe` only**, no installers (GitHub Release + CI artifact). |
| Stats | **Emerald+**, **World** by default; both changeable in Settings. |
| Counters | Highest win rate vs the enemy in that role, ignoring matchups below a **minimum games** threshold. |
| Jungle / Support | **Role vs role** (jungle vs enemy jungler, support vs enemy support). |
| Modes | Ranked Solo/Duo, Normal Draft/Flex (use ranked solo data), and **ARAM Mayhem** (the only ARAM mode the owner plays). Mayhem means builds **plus an augment ranking**: augments are picked in-game, so the app keeps the Mayhem build and augments visible during the game. Normal ARAM is low priority. |
| Items | Full build (starting, core, 4th/5th/6th options, skill order) **plus** push an in-game **item set** to the client. |
| Look | League-style dark: deep navy, gold accents, champion/rune/item icons. |
| Extras | **My champion pool** (counters from your pool shown first/highlighted); **Manual lookup** mode (works with League closed). |
| Rune pages full | **Ask first**: never overwrite a user page silently. The app's own `CSH:` pages are reused freely. |
| Import timing | Auto-import (on/off toggle) runs **exactly once, at the moment you lock in**, using the lane opponent known then. It never re-imports automatically, so your own edits after lock-in are safe. Manual **Import** button any time (the UI suggests it if the lane opponent appears later). |
| Blind / first pick | Show the **tier list for my role** until the lane opponent is visible. |
| Summoner spells | **Recommendation only.** The app never changes your spells (Flash on D vs F is personal). |
| Priority | **Working properly over new features.** Modes that matter: **Ranked, Normals, ARAM Mayhem**. |
| Data sources | u.gg now. Later: a **source dropdown** (Lolalytics first). A "Consensus" entry may come after that. Builds carry a `source` field. |

Tier list: `{base}/champion_ranking/{regionKey}/{patch}/{queue}/{rankKey}/{ver}.json`. Note this one uses
**string** keys (`world`, `emerald_plus`), not numeric ids. Format: `[ {roleName: [row...]}, {champId: bans}, "timestamp", totalMatches ]`,
row = `["champId", [[oppId, wins, games] x10], wins, games, ...]`.

## How it works

```
 ┌──────────────── Windows PC ────────────────┐
 │  League Client ──(local LCU API, HTTPS)──┐ │
 │                                           ▼ │       stats2.u.gg (JSON)
 │  Champ Select Helper  ─ Rust backend ────────────►  ddragon (names/icons)
 │        ▲                    │               │
 │        └── Svelte UI ◄──────┘ events        │
 └─────────────────────────────────────────────┘
```

1. **Find the client.** Read the `lockfile` in the League install folder
   (`LeagueClient:pid:port:password:https`); the folder comes from Riot's
   `C:\ProgramData\Riot Games\Metadata\league_of_legends.live\league_of_legends.live.product_settings.yaml`
   (`product_install_full_path`), falling back to `C:\Riot Games\League of Legends`.
   **Never** list, open or read other processes (no command lines, no memory):
   see [docs/COMPLIANCE.md](docs/COMPLIANCE.md).
2. **Watch champ select.** Poll `GET /lol-gameflow/v1/gameflow-phase` every 1 s in
   and near champ select (lobby, queue, ready check), every 5 s elsewhere (home
   screen, in game). In champ select also `GET /lol-champ-select/v1/session` and
   `GET /lol-gameflow/v1/session` (queue id). Build a `ChampSelectState`, emit it
   to the UI as the `champ-select` event when it changes.
3. **Work out the lane opponent.** Your role comes from `assignedPosition`.
   Enemy roles are hidden in champ select, so assign enemy champions to roles
   using u.gg `primary_roles` (each champ's roles in order of popularity), with
   a best-fit assignment across the 5 enemies. The user can override by
   clicking an enemy.
4. **Fetch data from u.gg**, cache it on disk per patch.
5. **Auto-import** (when on at lock-in): once your champion is **locked**, push
   the rune page and item set — exactly once per champ select (see the owner
   table: no re-import when the lane opponent appears later, the UI only
   suggests a manual import; summoner spells are never written).
   The u.gg work (build, champion roles) runs in the background so the 1 s
   poll never waits for u.gg; right before writing, the import checks it's
   still the same champ select (session `gameId`) and champion, else it
   writes nothing. "Imported" is remembered in `last_import.json` (app cache
   dir, ignored after 2 h) so an app restart mid champ select doesn't import
   again. Champion roles are re-checked hourly and reloaded on a new u.gg patch.

## u.gg data (unofficial — may change without notice)

Base: `https://stats2.u.gg/lol/1.5`. No auth; send a normal browser `User-Agent`.
Missing files return **403** (treat as "no data").

- **Versions**: `https://static.bigbrain.gg/assets/lol/riot_patch_update/prod/ugg/ugg-api-versions.json`
  → `{ "16_19": { "overview": "1.5.0", "matchups": "1.5.0", "primary_roles": "1.5.0", ... } }`.
  The latest patch is the highest `major_minor` key.
- **Build (general)**: `{base}/overview/{patch}/{queue}/{champId}/{ver}.json`
- **Build (vs a specific opponent)**: `{base}/overview/{patch}/{queue}/matchups/{champId}_{oppId}/{ver}.json`
  (only for `ranked_solo_5x5` / `ranked_flex_sr`).
- **Matchups** (win rate vs every opponent): `{base}/matchups/{patch}/{queue}/{champId}/{ver}.json`
- **Primary roles**: `{base}/primary_roles/{patch}/{ver}.json` → `{ "champId": [roleId, ...] }` most-played first.

Queues: `ranked_solo_5x5`, `ranked_flex_sr`, `normal_draft_5x5`, `normal_blind_5x5`,
`normal_aram`. (`aram_mayhem` / `aram_mayhem_classic` exist in u.gg's code but have
no stats2 files — see [ARAM Mayhem](#aram-mayhem).)
LCU queue ids: 420 solo, 440 flex, 400 draft, 430 blind, 450 ARAM, 2400 ARAM Mayhem,
2450 ARAM Mayhem "Classic", 490 quickplay.

All stat files are nested `data[regionId][rankId][roleId]`, keys are strings.

- Region ids: 1 na1, 2 euw1, 3 kr, 4 eun1, 5 br1, 6 la1, 7 la2, 8 oc1, 9 ru, 10 tr1, 11 jp1, **12 world**, 13 ph2, 14 sg2, 15 th2, 16 tw2, 17 vn2, 18 me1
- Rank ids: 1 challenger, 2 master, 3 diamond, 4 platinum, 5 gold, 6 silver, 7 bronze, **8 overall (all ranks)**, 10 platinum_plus, 11 diamond_plus, 12 iron, 13 grandmaster, 14 master_plus, 15 diamond_2_plus, 16 emerald, **17 emerald_plus**
- Role ids: **1 jungle, 2 support, 3 adc, 4 top, 5 mid**, 6 ARAM (ARAM files only have rank `8` and role `6`)

### Overview entry — `data[r][k][role] = [stats, "timestamp"]`
`stats` indices:

| # | Meaning | Shape |
|---|---|---|
| 0 | Runes | `[games, wins, primaryStyleId, subStyleId, [6 perk ids]]` |
| 1 | Summoner spells | `[games, wins, [spell1, spell2]]` |
| 2 | Starting items | `[games, wins, [itemIds]]` |
| 3 | Core items (3) | `[games, wins, [itemIds]]` |
| 4 | Skill order | `[games, wins, ["Q","E",...], "QEW" (max order)]` |
| 5 | Item options | `[[4th: [id, wins, games]...], [5th...], [6th...], [other...], [], []]` |
| 6 | Overall | `[wins, games]` |
| 7 | (flag) | bool |
| 8 | Stat shards | `[games, wins, ["5008","5008","5001"]]` (strings!) |

### Matchups entry — `data[r][k][role] = [[row...], "timestamp"]`
Each row: `[opponentId, wins, games, ...lane stats]` — `wins` are **this
champion's** wins vs that opponent. So a counter's win rate vs the enemy is
`1 - wins/games` when reading the *enemy's* matchups file.

Sample (Emerald+, World, top): Yorick vs Gwen = 327 wins / 602 games (54.3%).

### ARAM Mayhem

Found 2026-10-02 (patch 16_19) by loading `https://u.gg/lol/champions/aram-mayhem/yorick-aram-mayhem`
in a browser: its `window.__SSR_DATA__` blob is keyed by the URLs the server fetched, and the
JS (`static.bigbrain.gg/lol/static/js/main.*.js`, `components-Champions-Overview.*.js`) confirms it.

**Build (runes, spells, items, skill order): the normal ARAM overview.** There is no Mayhem
stats file — every `aram_mayhem` / `aram_mayhem_classic` variant on stats2 (`overview`,
`ct-overview`, `builds`, `rankings`, `champion_ranking`, any patch) is 403. u.gg's Mayhem
page hard-codes its build section to `{queue: normal_aram, region: world, rank: overall,
role: none}`, i.e. `{base}/overview/{patch}/normal_aram/{champId}/{ver}.json` →
`data["12"]["8"]["6"]`. OP.GG's Mayhem page also uses its plain ARAM data (`type: "aram"`).
So `Queue::AramMayhem.ugg_queue()` is `normal_aram`. (u.gg's Mayhem page doesn't show
runes; we import the normal ARAM runes from the same entry.)

**Augments: static JSON on `https://static.bigbrain.gg/custom-aram-mayhem`** (`{m}` below).
Plain GET with a browser `User-Agent`, no Cloudflare. Missing files are **404** (not 403).
Keyed by the u.gg patch (`16_19`, same as `ugg-api-versions.json`; no version key of its own);
the patch appears twice in the URL. 16_17–16_19 exist; 16_20 is 404 until u.gg publishes it.

| File | URL | Format |
|---|---|---|
| Per-champion augment ranking | `{m}/{patch}/tierlist-per-champion-augments-rarity-{patch}/tierlist-augments-{champId}-{patch}.json` | `{"rarities": {"kPrismatic": [id…], "kGold": [id…], "kSilver": [id…]}, "lastUpdated": "2026-09-28T16:20:05+00:00"}` |
| Augment names | `{m}/{patch}/aram-mayhem-augment-manifest-{patch}.json` | `{"1361": "Icathia's Fall", …}` — 554 entries, Arena augments included |
| Champion tier list (unused) | `{m}/{patch}/tierlist-champions-{patch}.json` | `{"tiers": {"S+": [champId…], "S": […], "A", "B", "C", "D"}, "lastUpdated": …}` |
| Augment icon | `https://static.bigbrain.gg/cdragon-custom/{patch}/augments/{id}.webp` | webp, served as `binary/octet-stream` (fine in `<img>`) |

- The ranking is **only an order**: ids best first within each rarity. There are no games, win
  rates, pick rates, tiers or descriptions anywhere in u.gg's Mayhem data. u.gg shows every id
  in order, grouped by rarity, and its page text calls the first 3 prismatic ones "S+ picks".
  Yorick 16_19: 46 prismatic, 48 gold, 29 silver; top 3 = Icathia's Fall, Omni Soul, En Passant.
- Rarity keys `kPrismatic` / `kGold` / `kSilver` match CommunityDragon's `rarity` field.
  Mayhem augment ids are 1001–2999 plus a few 12xxx (e.g. 12317 Pandora's Box); they share
  an id space with Arena augments (Riot's `cherry-augments`).
- What the app does (`ugg.rs`): `Build.augments` = up to 10 per rarity, prismatic → gold →
  silver, in u.gg's order; ids missing from the manifest are skipped; `description` is empty
  and `games`/`win_rate`/`pick_rate` are 0 (the UI should hide them). The ranking is tried on the
  newest patch, then the previous one (patch day); icon URLs use the ranking's patch. If the
  augment files fail, the build is still returned without augments.
- Queue ids: u.gg's code has `ARAM_MAYHEM = 2400` (`aram_mayhem`) and `ARAM_MAYHEM_CLASSIC = 2450`
  (`aram_mayhem_classic`; OP.GG calls it "ARAM: Mayhem Classic-ish"). No site has separate data
  for 2450, so both map to `Queue::AramMayhem`.
- Fixtures: `src-tauri/tests/fixtures/mayhem/` (Yorick ranking, manifest, champion tier list —
  all real 16_19 files).

**Other sources checked** (for the augment stats/descriptions u.gg lacks):
- **OP.GG** `https://op.gg/lol/modes/aram-mayhem/{champ}/augments`: plain curl works (no bot wall),
  but the data is only inside the Next.js RSC payload of the ~800 KB HTML (`self.__next_f.push`
  chunks): `{"data": [{"id": 1104, "tier": 0, "performance": 81.1, "popular": 5.98, "name":
  "Minionmancer", "key": "ARAM_Minionmancer", "largeIcon", "smallIcon", "rarity": 4, "desc",
  "tooltip"}, …]}` (192 for Yorick). `popular` = pick share in % (sums to 100), `performance` =
  opaque score (75–85), `tier` 0–5, `rarity` 1 silver / 4 gold / 8 prismatic, `desc` has Riot
  markup and `?` placeholders; some entries have only id/tier/performance/popular. No games or
  win rate. Sample: `tests/fixtures/other_sources/opgg_mayhem_augments_83.json`. Its JSON API
  doesn't serve Mayhem: `lol-api-champion.op.gg/api/global/champions/aram/{id}/none` is plain
  ARAM, `/api/global/champions/{mode}/{id}/augments` returns **Arena** augments (with
  `first_place`/`total_place`) for any mode, `aram_mayhem` → "Mode was invalid",
  `aram_mayhem_classic` → valid but empty.
- **CommunityDragon** `https://raw.communitydragon.org/latest/plugins/rcp-be-lol-game-data/global/default/v1/cherry-augments.json`:
  every Mayhem augment (`augmentNameId: "ARAM_…"`, `nameTRA`, `rarity`, `augmentSmallIconPath`);
  no descriptions, no stats.
- **Lolalytics**: no Mayhem pages (404). **Mobalytics**, **METAsrc**: bot wall (403).

Tip: to find new u.gg data URLs, load a u.gg page in a real browser (u.gg itself is behind
Cloudflare; `stats2.u.gg` and `static.bigbrain.gg` are not) and grep `window.__SSR_DATA__` for `https://`.

## Riot static data (Data Dragon, official)

- Versions: `https://ddragon.leagueoflegends.com/api/versions.json` (first = latest)
- Champions: `/cdn/{v}/data/en_US/champion.json` (`key` is the numeric id as a string, `id` is e.g. `"MonkeyKing"`)
- Items: `/cdn/{v}/data/en_US/item.json` · Runes: `/cdn/{v}/data/en_US/runesReforged.json` · Spells: `/cdn/{v}/data/en_US/summoner.json`
- Images: `/cdn/{v}/img/champion/{id}.png`, `/cdn/{v}/img/item/{itemId}.png`, `/cdn/{v}/img/spell/{spellId}.png`, runes `/cdn/img/{icon}`

## League client (LCU) calls used

| Purpose | Call |
|---|---|
| Champ select state | `GET /lol-champ-select/v1/session` |
| Queue id | `GET /lol-gameflow/v1/session` → `gameData.queue.id` |
| Summoner id | `GET /lol-summoner/v1/current-summoner` |
| Rune pages | `GET /lol-perks/v1/pages`, `DELETE /lol-perks/v1/pages/{id}`, `POST /lol-perks/v1/pages` `{name, primaryStyleId, subStyleId, selectedPerkIds[9], current:true}` |
| Item set | `GET`/`PUT /lol-item-sets/v1/item-sets/{summonerId}/sets` |

Auth: HTTP Basic `riot:<password>`. The client uses a self-signed Riot
certificate that rustls can't verify (old v1 root, IP not in SAN), so the LCU
HTTP client accepts invalid certs. It only ever talks to 127.0.0.1, never via a proxy,
never follows redirects, and the password is never logged or sent to the UI.
Spells are never written (recommendation only). Any new LCU call must be added to the
endpoint table in [docs/COMPLIANCE.md](docs/COMPLIANCE.md) (and the Riot registration
text there); never call champ-select actions, matchmaking/ready-check, lobby or
login endpoints.
Rune pages and item sets the app creates are named with the prefix
**`CSH: `** so it only ever replaces its own.

## Code layout

```
src/                      Svelte UI
  lib/types.ts            ← shared contract (mirrors src-tauri/src/model.rs)
  lib/api.ts              invoke()/listen() wrappers (+ mock data outside Tauri)
src-tauri/src/
  model.rs                ← shared contract (Rust types sent to the UI)
  lib.rs                  Tauri setup + commands
  ugg.rs  ddragon.rs      data sources (+ disk cache)
  lcu.rs                  League client connection + import
  watcher.rs              background champ-select loop + auto-import
  settings.rs             settings load/save (JSON in app config dir)
src-tauri/tests/fixtures/ trimmed real u.gg responses for tests
```
