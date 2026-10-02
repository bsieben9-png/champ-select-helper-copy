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
| "Taking actions on your behalf (botting or scripting)" | No queue accept, pick, ban, lock-in, dodge, chat, or spell change, ever. It writes only a rune page + an item set: **one attempt** when you lock in (Auto-import, can be turned off; never retried, a failure only tells you to press Import, commit 6e411c7) or when you press Import. See the gray area above. |
| "Drawing conclusions for you during gameplay (ie we want to see you play the game first, then analyze and reflect later!)" | Nothing runs during the game except a cheap "which screen is the client on?" check to the client (not to the game), every 5 s. The app reads no live game data at all (no Live Client Data API, port 2999, no memory, no screen) and shows nothing new during the game. ARAM / ARAM Mayhem support (and with it the augment list) is being removed by owner decision (code kept on branch `saved/aram-mayhem`); supported modes are Ranked and Normals. |
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
| "All products must be registered in, and audited by Riot Games through the Developer Portal" | **Not registered yet.** Ready-to-paste registration text: see [Registering with Riot](#registering-with-riot). This is the one formal gap. |

### Riot Developer Portal: League of Legends docs

<https://developer.riotgames.com/docs/lol>

- League Client API: "This service is not officially supported for use with third party
  applications" (no guarantees of uptime or change notices; no ban language), and:
  "Whether you're combining the Riot Games API and League Client API, or doing something by
  only using the League Client endpoints, we need to know about it. Either create a new
  application or leave a note on your existing application in the Developer Portal. We need
  to know which endpoints you're using and how you're using them…" See
  [Registering with Riot](#registering-with-riot).
- Game integrity: "Products must not use or incorporate information not present in the
  game client that would give players a competitive edge (e.g., automatically or manually
  allowing tracking enemy ultimate cooldowns)" (the March 2025 enemy-ult-timer rule).
  The app has no timers or cooldown tracking of any kind.
- "Products cannot identify or analyze players who are deliberately hidden by the game."
  The app reads no player identities.
- "Products cannot display win rates for Augments or Arena Mode items. This applies to all
  websites, applications and overlays." The app shows **no augments at all**: ARAM / ARAM
  Mayhem support is being removed (owner decision, 2026-10-02; code kept on branch
  `saved/aram-mayhem`). Before that, verified on main: the augment panel
  (`src/lib/components/Augments.svelte`, fixed in b59c3a2) showed only u.gg's order (rank,
  icon, name), the backend never filled augment stats (`games`, `win_rate`, `pick_rate`
  always 0 in `src-tauri/src/ugg.rs`; u.gg publishes none) and no other component showed
  augment numbers. If ARAM Mayhem ever comes back from that branch, keep it ranking-only.
  Arena (queues 1700/1710) isn't supported at all (`Queue::from_lcu_queue_id` maps it to
  nothing, so no Arena data is fetched or shown).

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
| GET | `/lol-gameflow/v1/session` | every poll during champ select only | Queue id (ranked / normal) |
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

| Situation | Requests to the client |
|---|---|
| League closed | none (looks for the lockfile every 3 s: two small file reads) |
| Home screen, in game, end of game | 1 cheap `gameflow-phase` request every **5 s** |
| Lobby, in queue, ready check | 1 `gameflow-phase` request per second (champ select can start any moment) |
| Champ select | 3 small requests per second (phase, champ select session, queue) |
| Lock-in / Import button | a handful of rune page / item set requests, once |

That is the same order as the client's own UI and the mainstream apps (most of which keep a
WebSocket subscription open instead). The first few polls after leaving champ select stay
at 1 s so a dodge is noticed and the next champ select imports again.

### During the game

Only `GET /lol-gameflow/v1/gameflow-phase` to the **client** (not the game), every 5 s. No
game data is read and nothing new is shown during the game.

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

1. **Not registered with Riot (yet).** Riot's policies require registration ("All products
   must be registered in, and audited by Riot Games through the Developer Portal"). Riot's
   own words: approved apps built on the official API "should rarely have an issue". See
   [Registering with Riot](#registering-with-riot).
2. **Auto-import is an automatic action** (see the gray area). Turning it off makes every
   write a direct result of your click.
3. **The LCU is unsupported.** Riot can change it any time; the app would then stop
   working, not get you banned.
4. **u.gg's data is unofficial**, used without an agreement with u.gg. That's a
   u.gg-terms question, not a Riot ban risk.
5. **Korea.** Rules on third-party programs are stricter on the Korean server; this review
   covers the global rules only.

## Registering with Riot

Riot's policies say every product must be registered and audited on the Developer Portal,
and LCU apps must tell Riot which client endpoints they use. Registration is free and is
the one formal gap. Steps:

1. Sign in at <https://developer.riotgames.com> with your Riot account.
2. Click **Register Product** and choose **Personal** ("for products that are intended for
   just the developer or a small private community"; the app needs no API key, since it
   uses only the local client and the public Data Dragon).
3. Paste the text below into the form (adjust anything that changed), then complete the
   verification step Riot asks for.
4. If Riot replies with concerns in the portal, the usual fix is a setting (for example
   turning auto-import off by default). Riot's portal is also where any later change has to
   be "audited through the product's page".

**Ready-to-paste text** (plain text, keep the endpoint list in sync with
[the table above](#every-league-client-call-the-app-makes)):

```text
Product name: Champ Select Helper

Short description:
A personal, non-commercial Windows desktop tool for League of Legends champion
select (Ranked and Normal games). It shows public aggregate build statistics
(runes, items, skill order, summoner spell recommendation, counter picks) for my
champion and lane matchup, and can save a rune page and an item set into my
League client.

Product type: Personal project, for my own use (no public distribution, no ads,
no payments, no accounts, no data collection). Not affiliated with Riot.

How it works:
- Runs as a normal desktop app next to the League client. It never touches the
  game process: no memory reading or writing, no injection, no drivers, no
  overlay, no keyboard/mouse automation, no screen reading, no Live Client Data
  API.
- Finds the client only by reading League's lockfile (install folder taken from
  Riot's product_settings.yaml). It does not inspect any process.
- Uses only information shown in the client: my own pick and assigned role,
  allies' picks, locked enemy champions and bans. It never reads, stores or
  displays other players' names, Riot IDs, PUUIDs, ranks or match history. No
  augment or Arena data at all.
- Never accepts queues, picks, bans, locks in, dodges, or changes summoner
  spells. The only writes are one rune page and one item set, both named
  "CSH: ...": a single attempt when I lock in (can be turned off, never
  retried) or when I press Import. It only replaces its own "CSH:" pages/sets,
  and only replaces one of my own rune pages after I confirm it in a dialog
  (when all rune page slots are full).
- Polls the local client once per second in and right before champion select,
  every 5 seconds otherwise (one gameflow-phase request); nothing during the
  game beyond that.

Data sources:
- League Client API (local, 127.0.0.1), endpoints listed below
- Riot Data Dragon (champion/item/rune/spell names and icons)
- u.gg public statistics files (aggregate builds, matchups, tier lists)

League Client API endpoints used:
- GET    /lol-gameflow/v1/gameflow-phase              which screen the client is on
- GET    /lol-gameflow/v1/session                     queue id during champ select
- GET    /lol-champ-select/v1/session                 my pick/role, picks, bans
- GET    /lol-summoner/v1/current-summoner            my own name and summoner id
- GET    /lol-perks/v1/pages                          find the app's own "CSH:" page
- GET    /lol-perks/v1/inventory                      check if rune page slots are full
- POST   /lol-perks/v1/pages                          create the "CSH:" rune page
- DELETE /lol-perks/v1/pages/{id}                     remove the app's old "CSH:" page
                                                      (or my page, only after I confirm)
- GET    /lol-item-sets/v1/item-sets/{summonerId}/sets   read my item sets
- PUT    /lol-item-sets/v1/item-sets/{summonerId}/sets   save them with the "CSH:" set

Legal: "Champ Select Helper isn't endorsed by Riot Games and doesn't reflect the
views or opinions of Riot Games or anyone officially involved in producing or
managing Riot Games properties. Riot Games, and all associated properties are
trademarks or registered trademarks of Riot Games, Inc." is shown in the app.
```

Pending branches that add League client calls (e.g. `wip/modes`, which writes the rune
choice of a Swiftplay/Quickplay lobby slot via `GET`/`PUT
/lol-lobby/v1/lobby/members/localMember/player-slots` and reads `GET /lol-lobby/v2/lobby`)
must be added to this list and to the endpoint table when they are merged. That slot write
must keep changing only the slot's `perks` field (never the champion, skin or spells).

## Open decisions for the owner

- **Auto-import default** (on now). Off = the app never acts without a click from you.
- **Register the app** on the Developer Portal (text above). Strongly recommended.

## Changes made in this review

- Verified commit 3230aec (lockfile-only discovery): complete. No process listing, handles,
  command lines or memory reads remain; stale comments that still described the old
  command-line method were corrected (`src-tauri/src/lcu.rs`).
- The League-client connection can no longer be redirected anywhere: redirects are never
  followed, so the password and the certificate-check exception only ever apply to
  `127.0.0.1:<port>` (`src-tauri/src/lcu.rs`, test `redirects_are_never_followed`).
- Slower polling away from champ select: 5 s on the home screen and in game instead of
  1 s (`src-tauri/src/watcher.rs`, e2e test `polls_slowly_away_from_champ_select`). Owner
  rules (import once at lock-in, never re-import, spells untouched) unchanged; all tests
  green.
- Settings page shows Riot's legal boilerplate in full (it was shortened).
- README: no longer claims the import sets summoner spells; no longer tells you to run the
  app as administrator; new "Will this get me banned?" section.
- Not changed (owner decision): the auto-import default.
- Done elsewhere (main, same day, owner decisions): augment win rates never shown
  (b59c3a2), then ARAM / ARAM Mayhem removed entirely (saved on `saved/aram-mayhem`);
  auto-import is exactly one attempt at lock-in, never retried (6e411c7).

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
