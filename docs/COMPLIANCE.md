# Riot / Vanguard / League compliance

Plain-English answer to **"can using Champ Select Helper get my account banned?"**,
checked against Riot's own published rules on 2026-10-02. Re-check this page when
Riot changes its third-party rules (links at the bottom).

## Verdict

**Low risk, same category as Blitz, Porofessor, U.GG and Mobalytics' champ-select
features, with one gray area you should know about (auto-import, see below).**

- The app only talks to the League client's **official local API** (the "LCU", the
  same API the client's own screens use) and downloads **public** data (u.gg stats,
  Riot's Data Dragon). It never touches the game (`League of Legends.exe`), never
  reads or writes any program's memory, never injects anything, installs no driver,
  draws no overlay, sends no keyboard or mouse input and never reads the screen.
  Those are the things Vanguard and Riot's rules are about.
- It only uses information the client already shows you, and it never reveals other
  players' names.
- It never accepts a queue, picks, bans, locks in, dodges, or changes summoner spells.
- Its only writes into the client are a rune page and an item set, both named
  `CSH: …`, which is exactly what the mainstream apps do.

**Gray area (owner's decision, not changed here): auto-import at lock-in.** Riot lists
"Taking actions on your behalf (botting or scripting)" as prohibited. Writing a rune
page and item set into the client *in champ select* is a convenience action that every
mainstream app does (Blitz, Porofessor, U.GG, Mobalytics, OP.GG) and that Riot has not
acted against in years, but those apps are **registered/approved** with Riot and this
app is not. With **Auto-import off**, the app only writes when *you* press **Import**,
which removes the "on your behalf" question entirely. The setting is in
Settings → Import into the client → Auto-import (default: on).

Nothing here is a guarantee: Riot decides. But the app stays well inside the lines Riot
has drawn publicly, and does strictly less than the big approved apps.

## Riot's rules, and what the app does

### Riot Support: "Third Party Applications" (last updated Feb 10, 2025)

<https://support.riotgames.com/en-us/riot/events/third-party-applications>

> "No software should compromise game integrity or negatively impact the in-game
> player experience."

> "If a third party application is approved and built using the official Riot API,
> players should rarely have an issue."

| Riot prohibits | What Champ Select Helper does |
|---|---|
| "Exposing information that's intentionally obfuscated" | Never reads or shows other players' names, Riot IDs, PUUIDs, summoner ids, ranks or match history. Ranked champ select hides teammates' names; the app doesn't try to recover them. It shows only what the champ select screen shows you: your own pick and role, allies' picks/hovers, **locked** enemy champions, bans. Enemy *roles* are a guess from champion statistics (which lane each champion is usually played in), not from any hidden data; the UI marks them as guesses. Your own name (connection pill) comes from `/lol-summoner/v1/current-summoner` (you). |
| "Taking actions on your behalf (botting or scripting)" | No queue accept, pick, ban, lock-in, dodge, chat, or spell change, ever. It writes only a rune page + an item set: **once** when you lock in (Auto-import, can be turned off) or when you press Import. See the gray area above. |
| "Drawing conclusions for you during gameplay (ie we want to see you play the game first, then analyze and reflect later!)" | Nothing runs during the game except a cheap "which screen is the client on?" check to the client (not to the game). The app reads no live game data at all (no Live Client Data API, port 2999, no memory, no screen). In ARAM Mayhem the app keeps showing the build and u.gg's **static** augment ranking for your champion (the same list u.gg's website shows), loaded before the game. It doesn't know which augments you're offered and doesn't react to the game. Minor gray area: if you want to be strict, look at the augment list before the game starts. |
| "Altering your field of intelligence (zoomhacks or global ult alerts)" | Nothing of the kind: no overlay, no timers, no cooldown tracking, no in-game alerts. |
| Skin hacks / unauthorized services | None. |

### Riot Developer Portal: General Policies (last updated May 29, 2025)

<https://developer.riotgames.com/policies/general>

| Policy ("Game Integrity") | App |
|---|---|
| "Products cannot create an unfair advantage for players, like a cheating program…" | Shows public aggregate statistics anyone can look up on u.gg. |
| "Products should increase, and not decrease the diversity of game decisions" / "should not remove game decisions, but may highlight decisions that are important and give multiple choices" | Shows options (several counter picks, 4th/5th/6th item options, a tier list); you pick your champion and play your game. Rune import is the same convenience the approved apps offer. Spells are a recommendation only. |
| "Products cannot de-anonymize players who cannot reasonably be identified from visible information." | Reads no other player identity at all (see above). |
| "Products cannot create alternatives for official skill ranking systems … MMR or ELO calculators." | No MMR/ELO/player ratings. |
| Legal boilerplate ("[Your product] isn't endorsed by Riot Games …") must be shown in a readily visible location | In the README, the release notes and the app's Settings page (full text). |
| Products using the League Client API should be registered on the Developer Portal | **Not registered.** This is a personal tool, not a public product. If the owner ever shares it publicly, register it first (owner decision, see "Open decisions"). |

### Riot Developer Portal: League of Legends docs

<https://developer.riotgames.com/docs/lol>

- League Client API: "This service is not officially supported for use with third party
  applications" (no guarantees of uptime or change notices; no ban language). Riot asks
  apps using it to tell them ("we need to know about it"), same registration point as above.
- Game integrity: "Products must not use or incorporate information not present in the
  game client that would give players a competitive edge (e.g., automatically or manually
  allowing tracking enemy ultimate cooldowns)" (the March 2025 enemy-ult-timer rule).
  The app has no timers or cooldown tracking of any kind.
- "Products cannot identify or analyze players who are deliberately hidden by the game."
  The app reads no player identities.

### Champ select anonymity (patch 12.22, Oct 2022, still in force)

- Riot (dev post): <https://www.leagueoflegends.com/en-au/news/dev/matchmaking-and-champion-select-fall-2022/>
  hides all summoner names in Ranked Solo/Duo champ select; Riot staff:
  "Apps that reveal player names or stats in Champ Select will be asked to remove this
  feature immediately"
  (<https://devtrackers.gg/leagueoflegends/p/749a443a-apps-that-reveal-player-names-or-stats-in-champ-select-need-to-remove-these-informations-starting-patch-12-22>).
- The app shows no names or player stats for anyone but you. (Code check:
  `parse_champ_select` in `src-tauri/src/lcu.rs` reads only `cellId`, `championId`,
  `championPickIntent`, `assignedPosition`, actions and bans; no name/puuid/summonerId
  field of any other player is read, stored, sent to the UI or logged.)

### Vanguard

- Riot DevRel, "Vanguard FAQ for Third Party Applications" (Apr 1, 2024):
  <https://www.riotgames.com/en/DevRel/vanguard-faq>:
  "Apps developed using the LCU and in-game APIs are still expected to work."
  "External tools reading memory will no longer work." "There is absolutely no allow list
  for Vanguard."
- Riot DevRel, "Vanguard updates" (Feb 7, 2024): <https://www.riotgames.com/en/DevRel/vanguard>:
  "We expect this will only impact around 1% of third-party applications",
  "External tools reading memory will not be allowed".
- Riot Vanguard FAQ (League): <https://support.riotgames.com/league-of-legends/performance/riot-vanguard-faq-league-of-legends>
  (Vanguard blocks known-vulnerable *drivers* and cheats; it doesn't list LCU apps).

What Vanguard cares about, and the app:

| Vanguard target | App |
|---|---|
| Kernel drivers / vulnerable drivers | None. A normal user-mode program. |
| Reading/writing the game's memory, injection, hooks | None. Never opens any process. |
| Overlays drawn over the game | None. A normal separate window. |
| Input automation, macros | None. |
| Screen reading | None. |

**How the app finds League (since commit 3230aec):** it reads two small files League
itself writes for this purpose: Riot's install record
`C:\ProgramData\Riot Games\Metadata\league_of_legends.live\league_of_legends.live.product_settings.yaml`
(to learn where League is installed) and League's `lockfile` in that folder (port and
password of the local API, present only while the client runs), with
`C:\Riot Games\League of Legends\lockfile` as a fallback. Verified in this review: there is
no process listing, no process handle, no command-line read and no memory read anywhere
in the code; the `sysinfo` crate is gone from `Cargo.lock`; no Windows process,
registry or debugging API is called (the only `winreg` in `Cargo.lock` is a *build-time*
dependency of the icon/version-info compiler and is not in the app). It also means the
app never needs administrator rights, even if League runs as administrator.

## Every League client call the app makes

All calls go to `https://127.0.0.1:<port>` (the local League client, never the internet),
authenticated with the lockfile password. Source: `src-tauri/src/lcu.rs`.

| Method | Path | When | Why |
|---|---|---|---|
| GET | `/lol-gameflow/v1/gameflow-phase` | every poll (see "Request rate") | Which screen the client is on (home, lobby, champ select, in game…) |
| GET | `/lol-summoner/v1/current-summoner` | once per client connection, and at each import | **Your own** display name and summoner id (the item-set path needs it) |
| GET | `/lol-champ-select/v1/session` | every poll during champ select only | Your pick/role, allies' picks, locked enemy picks, bans |
| GET | `/lol-gameflow/v1/session` | every poll during champ select only | Queue id (ranked / normal / ARAM Mayhem) |
| GET | `/lol-perks/v1/pages` | at import | Find the app's old `CSH:` page(s) |
| DELETE | `/lol-perks/v1/pages/{id}` | at import | Delete the app's **own** old `CSH:` page. A page of yours only after you confirmed it in the "rune pages full" dialog. |
| POST | `/lol-perks/v1/pages` | at import | Create the `CSH: …` rune page (or put your page back if replacing it failed) |
| GET | `/lol-perks/v1/inventory` | at import, only if creating the page failed | Check whether all rune page slots are used |
| GET | `/lol-item-sets/v1/item-sets/{summonerId}/sets` | at import | Read your item sets |
| PUT | `/lol-item-sets/v1/item-sets/{summonerId}/sets` | at import | Write them back with the app's `CSH:` set for this champion added/replaced (your own sets unchanged; never written if they couldn't be read) |

Never called (verified by grep): ready check / matchmaking (`/lol-matchmaking/…`),
champ select actions or my-selection (`/lol-champ-select/v1/session/actions/…`,
`…/my-selection`: picks, bans, lock-in, **summoner spells**), lobby (`/lol-lobby/…`),
login/quit/process control, chat, ranked/match history of anyone, and the in-game Live
Client Data API (port 2999).

Write safety (checked in code and tests): only pages whose name starts with `CSH:` and
that the client marks deletable are deleted without asking; a user page is replaced only
with your confirmation, only when every slot is full, never a default/preset page, and it
is restored if creating the new page fails. Item sets: only `CSH:` sets for the same
champion are replaced. Tests: `src-tauri/src/lcu.rs` (mock client) and
`src-tauri/src/e2e/scenarios.rs` (fake League client; every scenario asserts spells and
default pages are never touched).

### Request rate

While League is closed the app looks for the lockfile every 3 s (two small file reads, no
network). While connected it asks the client one cheap question per poll; in champ select
it adds the two champ-select reads. That is roughly 3 small local requests per second in
champ select, the same order as the client's own UI and the mainstream apps (most of which
use a WebSocket subscription instead). See "Changes" for the slower idle polling.

### During the game

Only `GET /lol-gameflow/v1/gameflow-phase` to the **client** (not the game). No game data
is read; the ARAM Mayhem augment list is u.gg's static per-champion ranking downloaded
before the game.

## Compared with the mainstream apps

| | This app | Blitz / Porofessor / U.GG / Mobalytics desktop apps |
|---|---|---|
| Data from the client | LCU only | LCU + Live Client Data API, and in-game overlays |
| Rune page import at lock-in | Yes (toggle) | Yes |
| Item sets | Yes (toggle) | Yes |
| Summoner spells | Recommendation only | Many set them automatically |
| Auto-accept / auto-pick / auto-ban | No | Some offer auto-accept |
| In-game overlay | No | Yes (Blitz, Porofessor, Mobalytics) |
| Player lookups (ranks, match history) | No | Yes (after champ select / in game) |
| Registered with Riot | **No** (personal tool) | Yes |

The app does a strict subset of what the approved apps do. The only real difference is
registration.

## Residual risks

1. **Not registered with Riot.** Riot's policies ask LCU apps to register. Riot has not
   banned players for personal LCU tools that only import runes, but it reserves the right.
   Riot's own words: approved apps built on the official API "should rarely have an issue".
2. **Auto-import is an automatic action** (see the gray area). Turning it off makes every
   write a direct result of your click.
3. **The LCU is unsupported.** Riot can change it any time; the app would then stop
   working, not get you banned.
4. **u.gg's data is unofficial**, used without an agreement with u.gg. That's a
   u.gg-terms question, not a Riot ban risk.
5. **Korea.** Rules on third-party programs are stricter on the Korean server; this review
   covers the global rules only.

## Open decisions for the owner

- Auto-import default (on now). Off = no automatic actions at all.
- Register the app on the Riot Developer Portal if it's ever shared beyond personal use.

## Changes made in this review

(filled in below as they land)

## Sources (checked 2026-10-02)

- Riot Support, Third Party Applications (Feb 10, 2025): <https://support.riotgames.com/en-us/riot/events/third-party-applications>
- Riot Developer Portal, General Policies (May 29, 2025): <https://developer.riotgames.com/policies/general>
- Riot Developer Portal, League of Legends docs (LCU, game integrity): <https://developer.riotgames.com/docs/lol>
- Riot DevRel, Vanguard FAQ for third-party apps (Apr 1, 2024): <https://www.riotgames.com/en/DevRel/vanguard-faq>
- Riot DevRel, Vanguard updates (Feb 7, 2024): <https://www.riotgames.com/en/DevRel/vanguard>
- Riot Vanguard FAQ (League): <https://support.riotgames.com/league-of-legends/performance/riot-vanguard-faq-league-of-legends>
- Champ select anonymity (Fall 2022 dev update): <https://www.leagueoflegends.com/en-au/news/dev/matchmaking-and-champion-select-fall-2022/>
- Riot staff on apps showing names/stats in champ select (patch 12.22): <https://devtrackers.gg/leagueoflegends/p/749a443a-apps-that-reveal-player-names-or-stats-in-champ-select-need-to-remove-these-informations-starting-patch-12-22>
- Enemy ultimate timers banned for third-party apps from March 13, 2025 (reported):
  <https://mein-mmo.de/lol-riot-verbietet-ultimate-time-tracker/>; now part of the
  game-integrity text on <https://developer.riotgames.com/docs/lol>
- Overwolf's Riot compliance summary (champ select "Ally #" rule, no ult/summoner timers):
  <https://dev.overwolf.com/ow-native/guides/game-compliance/riot-games>
