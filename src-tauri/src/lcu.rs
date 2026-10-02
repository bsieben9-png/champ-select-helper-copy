//! League Client (LCU) connection: discovery, champ select parsing, import.
//! OWNER: client agent. Public signatures are a contract — don't change them.
//!
//! ## TLS
//! The client serves HTTPS on 127.0.0.1 with a certificate issued by Riot's
//! own CA (`riotgames.pem`). That CA is an X.509 **v1** certificate, the leaf
//! is issued for the IP via the legacy Common Name only, and rustls/webpki
//! (which we use — no OpenSSL/SChannel) rejects v1 certificates and never
//! falls back to the CN. Trusting `riotgames.pem` therefore does not work
//! reliably with rustls, so the LCU HTTP client — and only that client — is
//! built with `danger_accept_invalid_certs(true)`. It only ever connects to
//! `127.0.0.1` (never through a proxy), and the password it sends is already
//! readable by any local process from the lockfile, so
//! verification would not add meaningful protection here.
//!
//! ## Finding the client (Riot / Vanguard / antivirus friendly)
//! The port + password come ONLY from League's `lockfile`, located via Riot's
//! own install metadata. The app never lists, opens or reads other processes
//! (no command lines, no memory), never touches the game process, and only
//! talks to the client's official local API. All other HTTP
//! (u.gg, Data Dragon) uses normal certificate checks.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, bail, Context};
use base64::Engine as _;
use reqwest::{header, Method, StatusCode};
use serde_json::{json, Value};

use crate::model::*;

/// Default League install folder (lockfile fallback).
const DEFAULT_INSTALL_DIR: &str = r"C:\Riot Games\League of Legends";
/// Prefix of rune pages / item sets this app creates (only those are ever replaced).
const PREFIX: &str = "CSH:";
/// Max rune page name length we create.
const RUNE_PAGE_NAME_MAX: usize = 25;
/// Summoner's Rift / Howling Abyss map ids for item sets.
const MAP_SUMMONERS_RIFT: u32 = 11;
const MAP_HOWLING_ABYSS: u32 = 12;

#[derive(Clone)]
pub struct LcuClient {
    http: reqwest::Client,
    /// e.g. "https://127.0.0.1:54321"
    base: String,
    /// Summoner display name, cached once known (a new client instance is
    /// created whenever the League client restarts / the account changes).
    summoner_name: Arc<Mutex<Option<String>>>,
}

/// Connection info found in the process command line or the lockfile.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Credentials {
    port: u16,
    token: String,
}

#[derive(Debug)]
enum RunesOutcome {
    /// Page created; extra notes for the user.
    Imported(Vec<String>),
    /// Every slot is used and there's no `CSH:` page to reuse. Carries the
    /// user's current editable page to ask about (None if none is suitable).
    NoFreeSlot(Option<RunePageRef>),
}

impl LcuClient {
    /// Find a running League client (lockfile). None if not running.
    ///
    /// Blocking but cheap: lists process names only (no CPU/memory stats),
    /// then reads the command line of the League process itself.
    pub fn discover() -> Option<LcuClient> {
        let creds = find_credentials()?;
        LcuClient::from_base_url(format!("https://127.0.0.1:{}", creds.port), &creds.token).ok()
    }

    fn from_base_url(base: String, token: &str) -> anyhow::Result<LcuClient> {
        let auth = base64::engine::general_purpose::STANDARD.encode(format!("riot:{token}"));
        let mut auth = header::HeaderValue::from_str(&format!("Basic {auth}"))?;
        auth.set_sensitive(true);
        let mut headers = header::HeaderMap::new();
        headers.insert(header::AUTHORIZATION, auth);
        headers.insert(
            header::ACCEPT,
            header::HeaderValue::from_static("application/json"),
        );
        let http = reqwest::Client::builder()
            .default_headers(headers)
            // See the module docs: Riot's self-signed v1 certificate can't be
            // validated by rustls. Loopback-only client.
            .danger_accept_invalid_certs(true)
            .no_proxy()
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(3))
            .build()?;
        Ok(LcuClient {
            http,
            base,
            summoner_name: Arc::new(Mutex::new(None)),
        })
    }

    /// Tests only: a client for a fake League client at `base`, e.g.
    /// `http://127.0.0.1:1234` (production only ever uses [`Self::discover`]).
    #[cfg(test)]
    pub(crate) fn for_test(base: &str, token: &str) -> LcuClient {
        LcuClient::from_base_url(base.to_string(), token).expect("LCU test client")
    }

    /// Send a request. Err only on transport errors (client gone, timeout);
    /// any HTTP status is returned with the parsed body (Null when empty,
    /// a JSON string when the body isn't JSON).
    async fn request(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
    ) -> anyhow::Result<(StatusCode, Value)> {
        let mut req = self
            .http
            .request(method.clone(), format!("{}{}", self.base, path));
        if let Some(body) = body {
            req = req.json(body);
        }
        let resp = req
            .send()
            .await
            .with_context(|| format!("{method} {path}: League client not reachable"))?;
        let status = resp.status();
        let bytes = resp
            .bytes()
            .await
            .with_context(|| format!("{method} {path}: reading response"))?;
        let value = if bytes.iter().all(|b| b.is_ascii_whitespace()) {
            Value::Null
        } else {
            serde_json::from_slice(&bytes)
                .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()))
        };
        Ok((status, value))
    }

    /// Like `request`, but non-2xx statuses are errors.
    async fn send(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
    ) -> anyhow::Result<Value> {
        let (status, value) = self.request(method.clone(), path, body).await?;
        if !status.is_success() {
            bail!("{method} {path}: {}", http_error(status, &value));
        }
        Ok(value)
    }

    /// GET a JSON endpoint, e.g. "/lol-champ-select/v1/session".
    pub async fn get(&self, path: &str) -> anyhow::Result<Value> {
        self.send(Method::GET, path, None).await
    }

    /// Connection status + gameflow phase. Err means the client went away.
    pub async fn status(&self) -> anyhow::Result<LcuStatus> {
        let (st, phase) = self
            .request(Method::GET, "/lol-gameflow/v1/gameflow-phase", None)
            .await?;
        if st == StatusCode::UNAUTHORIZED || st == StatusCode::FORBIDDEN {
            // Stale credentials (client restarted on the same port).
            bail!("League client rejected our credentials ({st})");
        }
        let phase = match (st.is_success(), phase.as_str()) {
            (true, Some(p)) if !p.is_empty() => p.to_string(),
            // Plugins not ready yet (client still starting) → treat as idle.
            _ => "None".to_string(),
        };

        let cached = self.summoner_name.lock().ok().and_then(|g| g.clone());
        let summoner_name = match cached {
            Some(name) => Some(name),
            None => {
                let (st, summoner) = self
                    .request(Method::GET, "/lol-summoner/v1/current-summoner", None)
                    .await?;
                let name = if st.is_success() {
                    summoner_display_name(&summoner)
                } else {
                    None
                };
                if let (Some(name), Ok(mut guard)) = (&name, self.summoner_name.lock()) {
                    *guard = Some(name.clone());
                }
                name
            }
        };

        Ok(LcuStatus {
            connected: true,
            summoner_name,
            phase,
        })
    }

    /// Current champ select state (default/not-in-champ-select when none).
    pub async fn champ_select(
        &self,
        roles: &HashMap<u32, Vec<Role>>,
    ) -> anyhow::Result<ChampSelectState> {
        Ok(self.champ_select_with_identity(roles).await?.0)
    }

    /// The raw champ select session; `None` when there is none (404).
    async fn champ_select_session(&self) -> anyhow::Result<Option<Value>> {
        let (st, session) = self
            .request(Method::GET, "/lol-champ-select/v1/session", None)
            .await?;
        if st == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !st.is_success() {
            bail!(
                "GET /lol-champ-select/v1/session: {}",
                http_error(st, &session)
            );
        }
        Ok(Some(session))
    }

    /// [`Self::champ_select`] plus which champ select it is
    /// ([`session_identity`]), so the watcher can tell a new champ select
    /// (dodge → requeue) from the same one seen again.
    pub async fn champ_select_with_identity(
        &self,
        roles: &HashMap<u32, Vec<Role>>,
    ) -> anyhow::Result<(ChampSelectState, Option<String>)> {
        let Some(session) = self.champ_select_session().await? else {
            return Ok((ChampSelectState::default(), None));
        };
        let from_gameflow = match self
            .request(Method::GET, "/lol-gameflow/v1/session", None)
            .await
        {
            Ok((st, flow)) if st.is_success() => {
                flow.pointer("/gameData/queue/id").and_then(Value::as_i64)
            }
            _ => None,
        };
        // Newer clients also put the queue id in the champ select session.
        let queue_id = from_gameflow.or_else(|| {
            session
                .get("queueId")
                .and_then(Value::as_i64)
                .filter(|&q| q > 0)
        });
        Ok((
            parse_champ_select(&session, queue_id, roles),
            session_identity(&session),
        ))
    }

    /// Cheap re-check before writing: the current champ select's identity
    /// and my champion (one request). `None` when not in champ select.
    pub async fn current_pick(&self) -> anyhow::Result<Option<(Option<String>, Option<u32>)>> {
        Ok(self.champ_select_session().await?.map(|session| {
            let me = parse_champ_select(&session, None, &HashMap::new()).my_champion_id;
            (session_identity(&session), me)
        }))
    }

    /// Push the rune page and item set (per settings toggles) into the
    /// client. Summoner spells are never changed. Never panics; failures are
    /// reported in `ImportResult.messages`.
    ///
    /// Rune pages: our own `CSH:` pages are always replaced. A user page is
    /// only ever replaced when the user agreed to it: `overwrite_page_id`
    /// (from a previous result's `needs_confirmation`), and only if all
    /// slots are still used.
    pub async fn import_build(
        &self,
        build: &Build,
        settings: &Settings,
        static_data: Option<&StaticData>,
        overwrite_page_id: Option<u64>,
    ) -> ImportResult {
        let mut result = ImportResult::default();
        let title = build_title(build, static_data);

        if settings.import_runes {
            let name = truncate_chars(&title, RUNE_PAGE_NAME_MAX);
            match self
                .import_runes(build, &name, static_data, overwrite_page_id)
                .await
            {
                Ok(RunesOutcome::Imported(notes)) => {
                    result.runes = true;
                    result.messages.push(format!("Runes: set page \"{name}\"."));
                    result.messages.extend(notes);
                }
                Ok(RunesOutcome::NoFreeSlot(page)) => {
                    result.messages.push(match &page {
                        Some(page) => format!(
                            "Runes not imported: all rune page slots are used. \
                             Replace your current page \"{}\"?",
                            page.name
                        ),
                        None => "Runes not imported: all rune page slots are used and the \
                                 current page can't be replaced. Select one of your own rune \
                                 pages, or delete one, and import again."
                            .to_string(),
                    });
                    result.needs_confirmation = page;
                }
                Err(e) => result.messages.push(format!("Runes not imported: {e:#}")),
            }
        }

        // Summoner spells are deliberately never changed (owner decision):
        // they're shown as a recommendation only.

        if settings.import_item_set {
            match self.import_item_set(build, &title).await {
                Ok(()) => {
                    result.item_set = true;
                    result
                        .messages
                        .push(format!("Item set: saved \"{title}\"."));
                }
                Err(e) => result
                    .messages
                    .push(format!("Item set not imported: {e:#}")),
            }
        }

        result
    }

    /// Create the rune page, replacing our own `CSH:` pages. When every slot
    /// is used, replace the user's page `overwrite` if given (they agreed),
    /// else report the current page as the one to ask about.
    async fn import_runes(
        &self,
        build: &Build,
        name: &str,
        static_data: Option<&StaticData>,
        overwrite: Option<u64>,
    ) -> anyhow::Result<RunesOutcome> {
        let runes = &build.runes;
        if runes.primary_style == 0
            || runes.sub_style == 0
            || runes.perks.len() != 6
            || runes.shards.len() != 3
        {
            bail!("the build has an incomplete rune page");
        }
        let mut notes = Vec::new();
        let selected: Vec<u32> = ordered_perks(runes, static_data)
            .into_iter()
            .chain(runes.shards.iter().copied())
            .collect();
        let body = json!({
            "name": name,
            "primaryStyleId": runes.primary_style,
            "subStyleId": runes.sub_style,
            "selectedPerkIds": selected,
            "current": true,
        });

        // 1. Remove pages we created earlier.
        let pages = self.get("/lol-perks/v1/pages").await?;
        for page in pages.as_array().into_iter().flatten() {
            let ours = page_name(page).starts_with(PREFIX) && flag(page, "isDeletable");
            if let (true, Some(id)) = (ours, page_id(page)) {
                if let Err(e) = self
                    .send(Method::DELETE, &format!("/lol-perks/v1/pages/{id}"), None)
                    .await
                {
                    notes.push(format!(
                        "Couldn't delete old page \"{}\": {e:#}",
                        page_name(page)
                    ));
                }
            }
        }

        // 2. Create the new page.
        let first_err = match self
            .send(Method::POST, "/lol-perks/v1/pages", Some(&body))
            .await
        {
            Ok(_) => return Ok(RunesOutcome::Imported(notes)),
            Err(e) => e,
        };

        // 3. Failed. Unless it's because all slots are used, report the error.
        let pages = self.get("/lol-perks/v1/pages").await?;
        let pages = pages.as_array().cloned().unwrap_or_default();
        let inventory = self.get("/lol-perks/v1/inventory").await.ok();
        if !no_free_page_slot(&pages, inventory.as_ref()) {
            return Err(first_err);
        }

        // 4. No free slot: never touch a user page without their consent.
        let Some(overwrite) = overwrite else {
            let current = pages
                .iter()
                .find(|p| flag(p, "current") && is_user_page(p))
                .and_then(|p| {
                    Some(RunePageRef {
                        id: page_id(p)?,
                        name: page_name(p).to_string(),
                    })
                });
            return Ok(RunesOutcome::NoFreeSlot(current));
        };
        let Some(page) = pages.iter().find(|p| page_id(p) == Some(overwrite)) else {
            bail!("the rune page to replace no longer exists");
        };
        if !is_user_page(page) {
            bail!("rune page \"{}\" can't be replaced", page_name(page));
        }
        let old_name = page_name(page).to_string();
        self.send(
            Method::DELETE,
            &format!("/lol-perks/v1/pages/{overwrite}"),
            None,
        )
        .await
        .with_context(|| format!("replacing your page \"{old_name}\""))?;
        match self
            .send(Method::POST, "/lol-perks/v1/pages", Some(&body))
            .await
        {
            Ok(_) => {
                notes.push(format!("Replaced your rune page \"{old_name}\"."));
                Ok(RunesOutcome::Imported(notes))
            }
            Err(e) => {
                // Put the user's page back.
                let restore = json!({
                    "name": old_name,
                    "primaryStyleId": page.get("primaryStyleId").cloned().unwrap_or(Value::Null),
                    "subStyleId": page.get("subStyleId").cloned().unwrap_or(Value::Null),
                    "selectedPerkIds": page.get("selectedPerkIds").cloned().unwrap_or(json!([])),
                    "current": flag(page, "current"),
                });
                match self
                    .send(Method::POST, "/lol-perks/v1/pages", Some(&restore))
                    .await
                {
                    Ok(_) => Err(e),
                    Err(e2) => Err(e.context(format!(
                        "also failed to restore your page \"{old_name}\": {e2:#}"
                    ))),
                }
            }
        }
    }

    async fn import_item_set(&self, build: &Build, title: &str) -> anyhow::Result<()> {
        let summoner = self.get("/lol-summoner/v1/current-summoner").await?;
        let summoner_id = summoner
            .get("summonerId")
            .and_then(Value::as_u64)
            .filter(|&id| id != 0)
            .ok_or_else(|| anyhow!("no summoner id (not logged in?)"))?;
        let path = format!("/lol-item-sets/v1/item-sets/{summoner_id}/sets");
        let existing = self.get(&path).await?;
        // The PUT replaces *all* item sets: never write back a list we
        // couldn't read, or the user's own sets would be wiped.
        if !existing.get("itemSets").is_some_and(Value::is_array) {
            bail!("couldn't read your item sets from the client");
        }
        let new_set = make_item_set(build, title, &new_uid(build.champion_id));
        let mut updated = merge_item_sets(existing, new_set, build.champion_id, now_ms());
        if updated.get("accountId").and_then(Value::as_u64).is_none() {
            if let Some(account) = summoner.get("accountId") {
                updated["accountId"] = account.clone();
            }
        }
        self.send(Method::PUT, &path, Some(&updated)).await?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Discovery
// ---------------------------------------------------------------------------

/// Find the running League client's port + password from its `lockfile`.
///
/// Riot/Vanguard- and antivirus-friendly by design: this NEVER inspects other
/// processes: no process list, no process handles, no command lines, no
/// memory reads. It only reads two small files that League writes for exactly
/// this purpose: Riot's install metadata (where League is installed) and the
/// `lockfile` (present only while the client runs). A stale lockfile left by
/// a crash just fails to connect and is retried later.
fn find_credentials() -> Option<Credentials> {
    install_dirs().iter().find_map(|dir| {
        let text = std::fs::read_to_string(dir.join("lockfile")).ok()?;
        parse_lockfile(&text)
    })
}

/// Riot Client's record of where League is installed (relative to ProgramData).
const PRODUCT_SETTINGS: &str =
    r"Riot Games\Metadata\league_of_legends.live\league_of_legends.live.product_settings.yaml";

/// Candidate League install folders, best first.
fn install_dirs() -> Vec<PathBuf> {
    let program_data = std::env::var_os("ProgramData")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"));
    let mut dirs: Vec<PathBuf> = std::fs::read_to_string(program_data.join(PRODUCT_SETTINGS))
        .ok()
        .and_then(|yaml| parse_product_install_path(&yaml))
        .into_iter()
        .collect();
    let default = PathBuf::from(DEFAULT_INSTALL_DIR);
    if !dirs.contains(&default) {
        dirs.push(default);
    }
    dirs
}

/// `product_install_full_path: "C:/Riot Games/League of Legends"` from Riot's
/// product_settings.yaml (a simple line scan; no YAML dependency needed).
fn parse_product_install_path(yaml: &str) -> Option<PathBuf> {
    yaml.lines().find_map(|line| {
        let value = line.trim().strip_prefix("product_install_full_path:")?;
        let value = value.trim().trim_matches(|c| c == '"' || c == '\'');
        (!value.is_empty()).then(|| PathBuf::from(value))
    })
}

/// `LeagueClient:pid:port:password:protocol`
fn parse_lockfile(text: &str) -> Option<Credentials> {
    let parts: Vec<&str> = text.trim().split(':').collect();
    if parts.len() < 5 {
        return None;
    }
    let port = parts[2].trim().parse().ok()?;
    let token = parts[3].trim().to_string();
    (!token.is_empty()).then_some(Credentials { port, token })
}

// ---------------------------------------------------------------------------
// Champ select parsing
// ---------------------------------------------------------------------------

/// Non-negative integer field as u32 (missing / negative → 0).
fn num(v: &Value, key: &str) -> u32 {
    v.get(key)
        .and_then(Value::as_i64)
        .and_then(|n| u32::try_from(n).ok())
        .unwrap_or(0)
}

fn flag(v: &Value, key: &str) -> bool {
    v.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn nonzero(id: u32) -> Option<u32> {
    (id != 0).then_some(id)
}

/// Pure: which champ select a session is, stable for its whole life and
/// different for the next one (a dodge and requeue is a new game): the
/// `gameId`, else the session `id` newer clients send. `None` if neither.
pub fn session_identity(session: &Value) -> Option<String> {
    if let Some(game_id) = session.get("gameId").and_then(Value::as_u64) {
        if game_id != 0 {
            return Some(format!("game:{game_id}"));
        }
    }
    session
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .map(|id| format!("session:{id}"))
}

/// Pure: turn a `/lol-champ-select/v1/session` JSON into our state.
pub fn parse_champ_select(
    session: &Value,
    queue_id: Option<i64>,
    roles: &HashMap<u32, Vec<Role>>,
) -> ChampSelectState {
    let empty = Vec::new();
    let local_cell = session.get("localPlayerCellId").and_then(Value::as_i64);
    let my_team = session
        .get("myTeam")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    let their_team = session
        .get("theirTeam")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    // `actions` is a list of turns, each a list of simultaneous actions.
    let actions: Vec<&Value> = session
        .get("actions")
        .and_then(Value::as_array)
        .unwrap_or(&empty)
        .iter()
        .flat_map(|turn| match turn {
            Value::Array(turn) => turn.iter().collect::<Vec<_>>(),
            other => vec![other],
        })
        .collect();
    let action_type = |a: &Value| {
        a.get("type")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    };
    let cell_of = |v: &Value, key: &str| v.get(key).and_then(Value::as_i64);

    let queue = queue_id.and_then(Queue::from_lcu_queue_id);
    let is_aram = queue.is_some_and(Queue::is_aram);
    let position = |p: &Value| {
        if is_aram {
            None
        } else {
            p.get("assignedPosition")
                .and_then(Value::as_str)
                .and_then(Role::from_lcu_position)
        }
    };

    // Me.
    let me = my_team
        .iter()
        .find(|p| local_cell.is_some() && cell_of(p, "cellId") == local_cell);
    let my_picks: Vec<&Value> = actions
        .iter()
        .copied()
        .filter(|a| {
            action_type(a) == "pick"
                && local_cell.is_some()
                && cell_of(a, "actorCellId") == local_cell
        })
        .collect();
    let hovered = my_picks
        .iter()
        .find(|a| flag(a, "isInProgress") && !flag(a, "completed"))
        .and_then(|a| nonzero(num(a, "championId")));
    // Selected champion, then the live hover during my pick turn (fresher
    // than a planning-phase intent), then the declared intent.
    let my_champion_id = me.and_then(|p| {
        nonzero(num(p, "championId"))
            .or(hovered)
            .or_else(|| nonzero(num(p, "championPickIntent")))
    });
    let my_champion_locked = my_champion_id.is_some()
        && if my_picks.is_empty() {
            // ARAM / all-random: no pick actions — the assigned champion is final
            // (bench swaps/rerolls just change the champion id).
            me.is_some_and(|p| num(p, "championId") != 0)
        } else {
            // Any completed own pick (trades can later change the champion id).
            my_picks.iter().any(|a| flag(a, "completed"))
        };
    let my_role = me.and_then(position);

    let allies = my_team
        .iter()
        .map(|p| AllyPick {
            champion_id: nonzero(num(p, "championId"))
                .or_else(|| nonzero(num(p, "championPickIntent")))
                .unwrap_or(0),
            role: position(p),
            is_me: local_cell.is_some() && cell_of(p, "cellId") == local_cell,
        })
        .collect();

    // Bans: completed ban actions + the bans lists, deduplicated.
    let mut bans: Vec<u32> = Vec::new();
    let ban_actions = actions
        .iter()
        .filter(|a| action_type(a) == "ban" && flag(a, "completed"))
        .map(|a| num(a, "championId"));
    let ban_lists = ["myTeamBans", "theirTeamBans"].into_iter().flat_map(|k| {
        session
            .pointer(&format!("/bans/{k}"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_i64)
            .filter_map(|n| u32::try_from(n).ok())
    });
    for id in ban_actions.chain(ban_lists) {
        if id != 0 && !bans.contains(&id) {
            bans.push(id);
        }
    }

    // Enemies: their locked champion (from theirTeam, else their completed pick action).
    let enemy_ids: Vec<u32> = their_team
        .iter()
        .map(|p| {
            nonzero(num(p, "championId"))
                .or_else(|| {
                    let cell = cell_of(p, "cellId")?;
                    actions
                        .iter()
                        .find(|a| {
                            action_type(a) == "pick"
                                && flag(a, "completed")
                                && cell_of(a, "actorCellId") == Some(cell)
                        })
                        .and_then(|a| nonzero(num(a, "championId")))
                })
                .unwrap_or(0)
        })
        .collect();
    let known: Vec<Option<Role>> = their_team.iter().map(position).collect();
    let enemies: Vec<EnemyPick> = if is_aram {
        enemy_ids
            .iter()
            .map(|&champion_id| EnemyPick {
                champion_id,
                role: None,
                role_inferred: false,
            })
            .collect()
    } else {
        // Roles the client tells us are fixed; guess the rest among the remaining roles.
        let taken: Vec<Role> = known.iter().flatten().copied().collect();
        let unknown_ids: Vec<u32> = enemy_ids
            .iter()
            .zip(&known)
            .map(|(&id, k)| if k.is_some() { 0 } else { id })
            .collect();
        let guessed = infer_roles_excluding(&unknown_ids, roles, &taken);
        enemy_ids
            .iter()
            .zip(known.iter().zip(guessed))
            .map(|(&champion_id, (known, guessed))| match known {
                Some(role) => EnemyPick {
                    champion_id,
                    role: Some(*role),
                    role_inferred: false,
                },
                None => EnemyPick {
                    champion_id,
                    role: guessed,
                    role_inferred: guessed.is_some(),
                },
            })
            .collect()
    };

    let lane_opponent_id = my_role.and_then(|role| {
        enemies
            .iter()
            .find(|e| e.champion_id != 0 && e.role == Some(role))
            .map(|e| e.champion_id)
    });

    ChampSelectState {
        in_champ_select: true,
        queue_id,
        queue,
        my_role,
        my_champion_id,
        my_champion_locked,
        allies,
        enemies,
        lane_opponent_id,
        bans,
    }
}

/// Score for playing a champion in its k-th most played role.
const ROLE_WEIGHTS: [u32; 5] = [100, 40, 15, 5, 2];

fn role_score(champion_id: u32, role: Role, roles: &HashMap<u32, Vec<Role>>) -> u32 {
    roles
        .get(&champion_id)
        .and_then(|rs| rs.iter().position(|r| *r == role))
        .map_or(0, |k| ROLE_WEIGHTS.get(k).copied().unwrap_or(1))
}

/// Pure: best-guess role for each enemy champion id (0 = not picked yet).
/// (`parse_champ_select` uses the variant that also respects roles the
/// client already revealed.)
#[allow(dead_code)]
pub fn infer_enemy_roles(enemy_ids: &[u32], roles: &HashMap<u32, Vec<Role>>) -> Vec<Option<Role>> {
    infer_roles_excluding(enemy_ids, roles, &[])
}

/// Assign picked champions to distinct roles (not in `taken`) maximizing the
/// total role score, by exhaustive search (≤ 5! assignments). A champion
/// whose assigned role scores 0 (no role data for it) only keeps that role
/// when it's forced by elimination: every free role is filled and it's the
/// only such champion. Otherwise its role is unknown (None).
fn infer_roles_excluding(
    ids: &[u32],
    roles: &HashMap<u32, Vec<Role>>,
    taken: &[Role],
) -> Vec<Option<Role>> {
    let picked: Vec<usize> = (0..ids.len()).filter(|&i| ids[i] != 0).collect();
    let free: Vec<Role> = Role::ALL
        .into_iter()
        .filter(|r| !taken.contains(r))
        .collect();

    struct Search<'a> {
        ids: &'a [u32],
        picked: &'a [usize],
        free: &'a [Role],
        roles: &'a HashMap<u32, Vec<Role>>,
        current: Vec<Option<usize>>,
        best: Vec<Option<usize>>,
        best_score: Option<u32>,
    }
    impl Search<'_> {
        fn run(&mut self, k: usize, used: u32, score: u32) {
            if k == self.picked.len() {
                if self.best_score.is_none_or(|b| score > b) {
                    self.best_score = Some(score);
                    self.best = self.current.clone();
                }
                return;
            }
            let id = self.ids[self.picked[k]];
            let mut any_free = false;
            for (ri, role) in self.free.iter().enumerate() {
                if used & (1 << ri) != 0 {
                    continue;
                }
                any_free = true;
                self.current[k] = Some(ri);
                let s = role_score(id, *role, self.roles);
                self.run(k + 1, used | (1 << ri), score + s);
            }
            if !any_free {
                // More picked champions than roles: leave the rest unassigned.
                self.current[k] = None;
                self.run(k + 1, used, score);
            }
        }
    }

    let mut search = Search {
        ids,
        picked: &picked,
        free: &free,
        roles,
        current: vec![None; picked.len()],
        best: vec![None; picked.len()],
        best_score: None,
    };
    search.run(0, 0, 0);

    let mut out = vec![None; ids.len()];
    let assigned: Vec<(usize, Role)> = picked
        .iter()
        .zip(&search.best)
        .filter_map(|(&i, ri)| ri.map(|ri| (i, free[ri])))
        .collect();
    let zero_score = assigned
        .iter()
        .filter(|(i, role)| role_score(ids[*i], *role, roles) == 0)
        .count();
    let forced = assigned.len() == free.len() && zero_score == 1;
    for (i, role) in assigned {
        if role_score(ids[i], role, roles) > 0 || forced {
            out[i] = Some(role);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Import helpers (pure)
// ---------------------------------------------------------------------------

fn summoner_display_name(summoner: &Value) -> Option<String> {
    ["gameName", "displayName"]
        .iter()
        .filter_map(|k| summoner.get(*k).and_then(Value::as_str))
        .map(str::trim)
        .find(|s| !s.is_empty())
        .map(str::to_string)
}

/// LCU error body → short text.
fn http_error(status: StatusCode, body: &Value) -> String {
    let message = body
        .get("message")
        .and_then(Value::as_str)
        .or_else(|| body.as_str())
        .unwrap_or("")
        .trim();
    if message.is_empty() {
        format!("HTTP {status}")
    } else {
        format!("HTTP {status}: {message}")
    }
}

fn page_name(page: &Value) -> &str {
    page.get("name").and_then(Value::as_str).unwrap_or("")
}

fn page_id(page: &Value) -> Option<u64> {
    page.get("id").and_then(Value::as_u64)
}

/// A page the user created (not a default/preset/temporary page).
fn is_user_page(page: &Value) -> bool {
    flag(page, "isEditable") && flag(page, "isDeletable") && !flag(page, "isTemporary")
}

/// True when the inventory says no more custom pages can be created.
fn no_free_page_slot(pages: &[Value], inventory: Option<&Value>) -> bool {
    let Some(inv) = inventory else { return false };
    if inv.get("canAddCustomPage").and_then(Value::as_bool) == Some(false) {
        return true;
    }
    let Some(owned) = inv.get("ownedPageCount").and_then(Value::as_u64) else {
        return false;
    };
    // The client's own count of custom pages, else ours.
    let custom = inv
        .get("customPageCount")
        .and_then(Value::as_u64)
        .unwrap_or_else(|| pages.iter().filter(|p| is_user_page(p)).count() as u64);
    custom >= owned
}

fn champion_name(static_data: Option<&StaticData>, id: u32) -> String {
    static_data
        .and_then(|sd| sd.champions.iter().find(|c| c.id == id))
        .map(|c| c.name.clone())
        .unwrap_or_else(|| id.to_string())
}

fn role_label(role: Option<Role>, queue: Queue) -> &'static str {
    match role {
        Some(Role::Top) => "Top",
        Some(Role::Jungle) => "Jungle",
        Some(Role::Mid) => "Mid",
        Some(Role::Adc) => "ADC",
        Some(Role::Support) => "Support",
        None if queue.is_aram() => "ARAM",
        None => "",
    }
}

/// "CSH: Yorick vs Gwen" (matchup build) or "CSH: Yorick Top".
fn build_title(build: &Build, static_data: Option<&StaticData>) -> String {
    let champ = champion_name(static_data, build.champion_id);
    match build.opponent_id {
        Some(opp) if !build.fell_back_to_general => {
            format!("{PREFIX} {champ} vs {}", champion_name(static_data, opp))
        }
        _ => format!("{PREFIX} {champ} {}", role_label(build.role, build.queue))
            .trim_end()
            .to_string(),
    }
}

/// The 6 perks in the order the client itself uses: keystone, the 3 primary
/// rows, then the 2 secondary perks by row. u.gg lists the keystone and then
/// the other five sorted by id, mixing both trees (Grasp, Presence of Mind,
/// Demolish, Overgrowth, Bone Plating, Legend: Bloodline). Unchanged when the
/// rune trees are unknown or the perks don't fit them.
fn ordered_perks(runes: &RunePage, static_data: Option<&StaticData>) -> Vec<u32> {
    let original = runes.perks.clone();
    let style = |id: u32| static_data?.rune_styles.iter().find(|s| s.id == id);
    let (Some(primary), Some(sub)) = (style(runes.primary_style), style(runes.sub_style)) else {
        return original;
    };
    let in_row = |row: &[RuneInfo]| -> Vec<u32> {
        runes
            .perks
            .iter()
            .copied()
            .filter(|p| row.iter().any(|r| r.id == *p))
            .collect()
    };
    let mut out = Vec::with_capacity(6);
    for row in &primary.slots {
        match in_row(row)[..] {
            [perk] => out.push(perk),
            _ => return original,
        }
    }
    // Secondary tree: keystones can't be taken.
    for row in sub.slots.iter().skip(1) {
        out.extend(in_row(row));
    }
    let mut sorted_out = out.clone();
    let mut sorted_original = original.clone();
    sorted_out.sort_unstable();
    sorted_original.sort_unstable();
    if sorted_out == sorted_original {
        out
    } else {
        original
    }
}

fn truncate_chars(s: &str, max: usize) -> String {
    s.chars()
        .take(max)
        .collect::<String>()
        .trim_end()
        .to_string()
}

fn item_list(ids: impl IntoIterator<Item = u32>) -> Vec<Value> {
    // Collapse repeats (e.g. 2 health potions) into a count, keeping order.
    let mut out: Vec<(u32, u32)> = Vec::new();
    for id in ids.into_iter().filter(|&id| id != 0) {
        match out.iter_mut().find(|(i, _)| *i == id) {
            Some((_, count)) => *count += 1,
            None => out.push((id, 1)),
        }
    }
    out.into_iter()
        .map(|(id, count)| json!({ "id": id.to_string(), "count": count }))
        .collect()
}

fn percent(win_rate: f64) -> f64 {
    // u.gg win rates are 0..1; tolerate a percentage too.
    if win_rate > 1.0 {
        win_rate
    } else {
        win_rate * 100.0
    }
}

/// Item set JSON for the client (`LolItemSetsItemSet`).
fn make_item_set(build: &Build, title: &str, uid: &str) -> Value {
    let core_title = if build.core_items.games > 0 && build.core_items.win_rate > 0.0 {
        format!("Core build ({:.0}% WR)", percent(build.core_items.win_rate))
    } else {
        "Core build".to_string()
    };
    let options = |opts: &[ItemOption]| item_list(opts.iter().map(|o| o.item_id));
    let blocks: Vec<Value> = [
        (
            "Starting items".to_string(),
            item_list(build.starting_items.items.iter().copied()),
        ),
        (
            core_title,
            item_list(build.core_items.items.iter().copied()),
        ),
        ("4th item options".to_string(), options(&build.fourth_items)),
        ("5th item options".to_string(), options(&build.fifth_items)),
        ("6th item options".to_string(), options(&build.sixth_items)),
    ]
    .into_iter()
    .filter(|(_, items)| !items.is_empty())
    .map(|(kind, items)| {
        json!({
            "type": kind,
            "hideIfSummonerSpell": "",
            "showIfSummonerSpell": "",
            "items": items,
        })
    })
    .collect();
    let map = if build.queue.is_aram() {
        MAP_HOWLING_ABYSS
    } else {
        MAP_SUMMONERS_RIFT
    };
    json!({
        "uid": uid,
        "title": title,
        "type": "custom",
        "map": "any",
        "mode": "any",
        "sortrank": 0,
        "startedFrom": "blank",
        "associatedChampions": [build.champion_id],
        "associatedMaps": [map],
        "preferredItemSlots": [],
        "blocks": blocks,
    })
}

/// Replace our previous item set(s) for this champion with `new_set`.
fn merge_item_sets(mut existing: Value, new_set: Value, champion_id: u32, timestamp: u64) -> Value {
    if !existing.is_object() {
        existing = json!({});
    }
    let mut sets: Vec<Value> = existing
        .get("itemSets")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    sets.retain(|set| {
        let ours = set
            .get("title")
            .and_then(Value::as_str)
            .is_some_and(|t| t.starts_with(PREFIX));
        let for_champ = set
            .get("associatedChampions")
            .and_then(Value::as_array)
            .is_some_and(|c| {
                c.iter()
                    .any(|id| id.as_u64() == Some(u64::from(champion_id)))
            });
        !(ours && for_champ)
    });
    sets.push(new_set);
    existing["itemSets"] = Value::Array(sets);
    existing["timestamp"] = json!(timestamp);
    existing
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Random-enough UUID-shaped id for a new item set.
fn new_uid(seed: u32) -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut words = [0u64; 2];
    for (i, w) in words.iter_mut().enumerate() {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u64(now_ms());
        h.write_u32(seed);
        h.write_usize(i);
        *w = h.finish();
    }
    let hex = format!("{:016x}{:016x}", words[0], words[1]);
    format!(
        "{}-{}-4{}-a{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[13..16],
        &hex[17..20],
        &hex[20..32]
    )
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpListener;
    use Role::*;

    pub(crate) fn fixture(name: &str) -> Value {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/lcu")
            .join(name);
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap()
    }

    /// Small hand-made u.gg primary roles map.
    fn roles() -> HashMap<u32, Vec<Role>> {
        HashMap::from([
            (83, vec![Top, Jungle]),       // Yorick
            (887, vec![Top, Jungle, Mid]), // Gwen
            (122, vec![Top, Jungle]),      // Darius
            (64, vec![Jungle, Top, Mid]),  // Lee Sin
            (222, vec![Adc]),              // Jinx
            (412, vec![Support]),          // Thresh
            (103, vec![Mid]),              // Ahri
            (86, vec![Top, Mid]),          // Garen
        ])
    }

    fn enemy(champion_id: u32, role: Option<Role>, role_inferred: bool) -> EnemyPick {
        EnemyPick {
            champion_id,
            role,
            role_inferred,
        }
    }

    fn ally(champion_id: u32, role: Option<Role>, is_me: bool) -> AllyPick {
        AllyPick {
            champion_id,
            role,
            is_me,
        }
    }

    // --- parse_champ_select -------------------------------------------------

    #[test]
    fn ranked_draft_session() {
        let cs = parse_champ_select(&fixture("champ_select_ranked.json"), Some(420), &roles());
        assert!(cs.in_champ_select);
        assert_eq!(cs.queue_id, Some(420));
        assert_eq!(cs.queue, Some(Queue::RankedSolo));
        assert_eq!(cs.my_role, Some(Top));
        assert_eq!(cs.my_champion_id, Some(83));
        assert!(cs.my_champion_locked);
        assert_eq!(
            cs.allies,
            vec![
                ally(254, Some(Jungle), false),
                ally(103, Some(Mid), false),
                ally(83, Some(Top), true),
                ally(145, Some(Adc), false),
                ally(111, Some(Support), false), // pick intent
            ]
        );
        // Skipped ban (0) dropped, action + list bans deduplicated.
        assert_eq!(cs.bans, vec![157, 238, 266, 555, 875, 360, 10, 24, 893]);
        // Gwen and Darius both prefer top: Darius takes top (Gwen goes mid,
        // her 3rd role) because Lee Sin owns jungle.
        assert_eq!(
            cs.enemies,
            vec![
                enemy(887, Some(Mid), true),
                enemy(64, Some(Jungle), true),
                enemy(122, Some(Top), true),
                enemy(222, Some(Adc), true),
                enemy(0, None, false),
            ]
        );
        assert_eq!(cs.lane_opponent_id, Some(122));
    }

    #[test]
    fn ranked_all_enemies_picked() {
        let mut session = fixture("champ_select_ranked.json");
        session["theirTeam"][4]["championId"] = json!(412);
        let cs = parse_champ_select(&session, Some(420), &roles());
        let got: Vec<_> = cs.enemies.iter().map(|e| (e.champion_id, e.role)).collect();
        assert_eq!(
            got,
            vec![
                (887, Some(Mid)),
                (64, Some(Jungle)),
                (122, Some(Top)),
                (222, Some(Adc)),
                (412, Some(Support)),
            ]
        );
        assert_eq!(cs.lane_opponent_id, Some(122));
    }

    #[test]
    fn revealed_enemy_positions_are_used_and_excluded_from_guessing() {
        let mut session = fixture("champ_select_ranked.json");
        session["theirTeam"][0]["assignedPosition"] = json!("top"); // Gwen
        let cs = parse_champ_select(&session, Some(420), &roles());
        assert_eq!(cs.enemies[0], enemy(887, Some(Top), false));
        assert_eq!(cs.enemies[1], enemy(64, Some(Jungle), true));
        // Darius has no data for the remaining roles and isn't forced → unknown.
        assert_eq!(cs.enemies[2], enemy(122, None, false));
        assert_eq!(cs.enemies[3], enemy(222, Some(Adc), true));
        assert_eq!(cs.lane_opponent_id, Some(887));
    }

    #[test]
    fn enemy_champion_from_completed_pick_action() {
        let mut session = fixture("champ_select_ranked.json");
        session["theirTeam"][0]["championId"] = json!(0);
        let cs = parse_champ_select(&session, Some(420), &roles());
        assert_eq!(cs.enemies[0].champion_id, 887);
    }

    #[test]
    fn hover_then_intent_then_nothing() {
        let mut session = fixture("champ_select_ranked.json");
        // My pick turn: hovering Yorick, not locked; I had declared Garen.
        session["myTeam"][2]["championId"] = json!(0);
        session["myTeam"][2]["championPickIntent"] = json!(86);
        let action = &mut session["actions"][3][1];
        assert_eq!(action["actorCellId"], json!(2));
        action["completed"] = json!(false);
        action["isInProgress"] = json!(true);
        let cs = parse_champ_select(&session, Some(420), &roles());
        assert_eq!(cs.my_champion_id, Some(83));
        assert!(!cs.my_champion_locked);

        // Planning phase: only the declared intent.
        session["actions"][3][1]["championId"] = json!(0);
        session["actions"][3][1]["isInProgress"] = json!(false);
        let cs = parse_champ_select(&session, Some(420), &roles());
        assert_eq!(cs.my_champion_id, Some(86));
        assert!(!cs.my_champion_locked);

        session["myTeam"][2]["championPickIntent"] = json!(0);
        let cs = parse_champ_select(&session, Some(420), &roles());
        assert_eq!(cs.my_champion_id, None);
        assert!(!cs.my_champion_locked);
    }

    #[test]
    fn blind_pick_session() {
        let cs = parse_champ_select(&fixture("champ_select_blind.json"), Some(430), &roles());
        assert_eq!(cs.queue, Some(Queue::NormalBlind));
        assert_eq!(cs.my_role, None);
        assert_eq!(cs.my_champion_id, Some(86));
        assert!(cs.my_champion_locked);
        let allies: Vec<_> = cs
            .allies
            .iter()
            .map(|a| (a.champion_id, a.role, a.is_me))
            .collect();
        assert_eq!(
            allies,
            vec![
                (412, None, false),
                (0, None, false),
                (103, None, false),
                (86, None, true),
                (0, None, false),
            ]
        );
        assert_eq!(cs.enemies, vec![enemy(0, None, false); 5]);
        assert_eq!(cs.lane_opponent_id, None);
        assert!(cs.bans.is_empty());
    }

    #[test]
    fn aram_session() {
        let cs = parse_champ_select(&fixture("champ_select_aram.json"), Some(450), &roles());
        assert_eq!(cs.queue, Some(Queue::Aram));
        assert_eq!(cs.my_role, None);
        assert_eq!(cs.my_champion_id, Some(83));
        // No pick actions in ARAM: the assigned champion counts as locked.
        assert!(cs.my_champion_locked);
        assert_eq!(cs.allies.len(), 5);
        assert!(cs.allies[2].is_me && cs.allies.iter().all(|a| a.role.is_none()));
        assert!(cs.enemies.is_empty());
        assert_eq!(cs.lane_opponent_id, None);

        let mayhem = parse_champ_select(&fixture("champ_select_aram.json"), Some(2400), &roles());
        assert_eq!(mayhem.queue, Some(Queue::AramMayhem));
        assert_eq!(mayhem.my_role, None);
    }

    #[test]
    fn session_identity_is_the_game_id_else_the_session_id() {
        let mut session = fixture("champ_select_ranked.json");
        assert_eq!(
            session_identity(&session).as_deref(),
            Some("game:7212345678")
        );
        session["gameId"] = json!(0);
        assert_eq!(
            session_identity(&session).as_deref(),
            Some("session:a3f1c2d4-5e6f-4a7b-8c9d-0e1f2a3b4c5d")
        );
        session["id"] = json!("");
        assert_eq!(session_identity(&session), None);
        assert_eq!(session_identity(&json!({})), None);
        assert_eq!(session_identity(&json!("garbage")), None);
    }

    #[test]
    fn unknown_queue_and_garbage_input() {
        let cs = parse_champ_select(&fixture("champ_select_ranked.json"), None, &roles());
        assert_eq!(cs.queue, None);
        assert_eq!(cs.my_role, Some(Top));
        let cs = parse_champ_select(&fixture("champ_select_ranked.json"), Some(-1), &roles());
        assert_eq!((cs.queue_id, cs.queue), (Some(-1), None));

        for junk in [
            json!(null),
            json!({}),
            json!([]),
            json!({"myTeam": 5, "actions": [1, [null]]}),
        ] {
            let cs = parse_champ_select(&junk, Some(420), &roles());
            assert!(cs.in_champ_select);
            assert_eq!(cs.my_champion_id, None);
            assert!(!cs.my_champion_locked);
        }
    }

    // --- infer_enemy_roles ---------------------------------------------------

    #[test]
    fn infer_full_team() {
        let got = infer_enemy_roles(&[887, 64, 122, 222, 412], &roles());
        assert_eq!(
            got,
            vec![Some(Mid), Some(Jungle), Some(Top), Some(Adc), Some(Support)]
        );
        let got = infer_enemy_roles(&[412, 222, 64, 103, 86], &roles());
        assert_eq!(
            got,
            vec![Some(Support), Some(Adc), Some(Jungle), Some(Mid), Some(Top)]
        );
    }

    #[test]
    fn infer_conflicting_top_laners() {
        // Gwen vs Darius for top, with Lee Sin in the jungle.
        let got = infer_enemy_roles(&[887, 122, 64], &roles());
        assert_eq!(got, vec![Some(Mid), Some(Top), Some(Jungle)]);
        // Garen (top, mid) vs Darius (top, jungle): Garen moves mid.
        let got = infer_enemy_roles(&[86, 122, 64], &roles());
        assert_eq!(got, vec![Some(Mid), Some(Top), Some(Jungle)]);
    }

    #[test]
    fn infer_partial_and_unpicked() {
        assert_eq!(
            infer_enemy_roles(&[0, 887, 0, 0, 0], &roles()),
            vec![None, Some(Top), None, None, None]
        );
        assert_eq!(infer_enemy_roles(&[0, 0, 0, 0, 0], &roles()), vec![None; 5]);
        assert_eq!(infer_enemy_roles(&[], &roles()), Vec::<Option<Role>>::new());
        assert_eq!(
            infer_enemy_roles(&[83, 103], &roles()),
            vec![Some(Top), Some(Mid)]
        );
    }

    #[test]
    fn infer_champion_without_role_data() {
        // Not forced: unknown.
        assert_eq!(
            infer_enemy_roles(&[999, 887], &roles()),
            vec![None, Some(Top)]
        );
        // Last one standing gets the leftover role.
        assert_eq!(
            infer_enemy_roles(&[887, 64, 122, 222, 999], &roles()),
            vec![Some(Mid), Some(Jungle), Some(Top), Some(Adc), Some(Support)]
        );
        // No role data at all (u.gg unavailable): no guesses.
        assert_eq!(
            infer_enemy_roles(&[887, 64, 122, 222, 412], &HashMap::new()),
            vec![None; 5]
        );
    }

    #[test]
    fn infer_more_than_five_does_not_panic() {
        let got = infer_enemy_roles(&[887, 64, 122, 222, 412, 103], &roles());
        assert_eq!(got.len(), 6);
        assert_eq!(got.iter().filter(|r| r.is_some()).count(), 5);
    }

    // --- discovery -----------------------------------------------------------

    #[test]
    fn lockfile_credentials() {
        assert_eq!(
            parse_lockfile("LeagueClient:15872:54321:p4ss_W0rd-x:https\n"),
            Some(Credentials {
                port: 54321,
                token: "p4ss_W0rd-x".into()
            })
        );
        assert_eq!(parse_lockfile(""), None);
        assert_eq!(parse_lockfile("LeagueClient:1:notaport:pw:https"), None);
        assert_eq!(parse_lockfile("LeagueClient:1:2:"), None);
    }

    #[test]
    fn product_settings_install_path() {
        let yaml = "product_install_full_path: \"D:/Games/Riot Games/League of Legends\"\nproduct_install_root: \"D:/Games/Riot Games\"\n";
        assert_eq!(
            parse_product_install_path(yaml),
            Some(PathBuf::from("D:/Games/Riot Games/League of Legends"))
        );
        assert_eq!(
            parse_product_install_path("product_install_root: \"C:/x\""),
            None
        );
        assert_eq!(
            parse_product_install_path("product_install_full_path: \"\""),
            None
        );
        // The default install folder is always a candidate.
        assert!(install_dirs().contains(&PathBuf::from(DEFAULT_INSTALL_DIR)));
    }

    #[test]
    fn discover_does_not_panic() {
        // None on CI; may find a real client on a dev PC.
        let _ = LcuClient::discover();
    }

    // --- import helpers ------------------------------------------------------

    fn static_data() -> StaticData {
        let champ = |id: u32, key: &str, name: &str| ChampionInfo {
            id,
            key: key.into(),
            name: name.into(),
            icon: String::new(),
            roles: Vec::new(),
        };
        StaticData {
            version: "16.19.1".into(),
            ugg_patch: "16_19".into(),
            champions: vec![
                champ(83, "Yorick", "Yorick"),
                champ(887, "Gwen", "Gwen"),
                champ(136, "AurelionSol", "Aurelion Sol"),
                champ(20, "Nunu", "Nunu & Willump"),
            ],
            items: Vec::new(),
            rune_styles: Vec::new(),
            shards: Vec::new(),
            spells: Vec::new(),
        }
    }

    fn sample_build() -> Build {
        let opt = |item_id: u32| ItemOption {
            item_id,
            games: 120,
            win_rate: 0.55,
        };
        Build {
            source: Source::Ugg,
            champion_id: 83,
            role: Some(Top),
            opponent_id: Some(887),
            queue: Queue::RankedSolo,
            patch: "16_19".into(),
            rank: "emerald_plus".into(),
            region: "world".into(),
            games: 602,
            win_rate: 0.543,
            fell_back_to_general: false,
            runes: RunePage {
                primary_style: 8400,
                sub_style: 8000,
                perks: vec![8437, 8446, 8429, 8451, 9111, 9105],
                shards: vec![5008, 5001, 5001],
                games: 500,
                win_rate: 0.55,
            },
            spells: Spells {
                ids: [4, 12],
                games: 500,
                win_rate: 0.54,
            },
            starting_items: ItemGroup {
                items: vec![1054, 2003, 2003],
                games: 400,
                win_rate: 0.53,
            },
            core_items: ItemGroup {
                items: vec![6631, 3047, 3053],
                games: 300,
                win_rate: 0.561,
            },
            fourth_items: vec![opt(3071), opt(3065)],
            fifth_items: vec![opt(3075)],
            sixth_items: Vec::new(),
            skill_order: vec!["Q".into(), "E".into(), "W".into()],
            skill_priority: "QEW".into(),
            available_roles: vec![Top, Jungle],
            augments: Vec::new(),
        }
    }

    #[test]
    fn titles() {
        let sd = static_data();
        let mut build = sample_build();
        assert_eq!(build_title(&build, Some(&sd)), "CSH: Yorick vs Gwen");
        assert_eq!(build_title(&build, None), "CSH: 83 vs 887");
        build.fell_back_to_general = true;
        assert_eq!(build_title(&build, Some(&sd)), "CSH: Yorick Top");
        build.opponent_id = None;
        build.fell_back_to_general = false;
        build.role = None;
        build.queue = Queue::Aram;
        assert_eq!(build_title(&build, Some(&sd)), "CSH: Yorick ARAM");

        build.champion_id = 136;
        build.opponent_id = Some(20);
        build.queue = Queue::RankedSolo;
        let title = build_title(&build, Some(&sd));
        assert_eq!(title, "CSH: Aurelion Sol vs Nunu & Willump");
        assert_eq!(
            truncate_chars(&title, RUNE_PAGE_NAME_MAX),
            "CSH: Aurelion Sol vs Nunu"
        );
        assert_eq!(truncate_chars("CSH: Ünïcödé ✓", 13), "CSH: Ünïcödé");
    }

    #[test]
    fn item_set_json() {
        let set = make_item_set(&sample_build(), "CSH: Yorick vs Gwen", "uid-1");
        assert_eq!(set["title"], "CSH: Yorick vs Gwen");
        assert_eq!(set["type"], "custom");
        assert_eq!(set["map"], "any");
        assert_eq!(set["mode"], "any");
        assert_eq!(set["associatedChampions"], json!([83]));
        assert_eq!(set["associatedMaps"], json!([11]));
        let blocks = set["blocks"].as_array().unwrap();
        let kinds: Vec<_> = blocks.iter().map(|b| b["type"].as_str().unwrap()).collect();
        assert_eq!(
            kinds,
            vec![
                "Starting items",
                "Core build (56% WR)",
                "4th item options",
                "5th item options"
            ]
        );
        assert_eq!(
            blocks[0]["items"],
            json!([{"id": "1054", "count": 1}, {"id": "2003", "count": 2}])
        );
        assert_eq!(
            blocks[1]["items"],
            json!([{"id": "6631", "count": 1}, {"id": "3047", "count": 1}, {"id": "3053", "count": 1}])
        );

        let mut aram = sample_build();
        aram.queue = Queue::AramMayhem;
        let set = make_item_set(&aram, "CSH: Yorick ARAM", "uid-2");
        assert_eq!(set["associatedMaps"], json!([12]));
    }

    #[test]
    fn item_set_merge_replaces_only_our_set_for_this_champion() {
        let new_set = make_item_set(&sample_build(), "CSH: Yorick vs Gwen", "uid-new");
        let merged = merge_item_sets(fixture("item_sets.json"), new_set, 83, 1791000000000);
        assert_eq!(merged["accountId"], json!(2345678901234567u64));
        assert_eq!(merged["timestamp"], json!(1791000000000u64));
        let titles: Vec<_> = merged["itemSets"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["title"].as_str().unwrap())
            .collect();
        assert_eq!(
            titles,
            vec!["My own Yorick set", "CSH: Gwen Top", "CSH: Yorick vs Gwen"]
        );

        let merged = merge_item_sets(json!(null), json!({"title": "CSH: x"}), 83, 1);
        assert_eq!(merged["itemSets"].as_array().unwrap().len(), 1);
    }

    /// Precision + Resolve trees as in `ddragon_runesReforged.json`.
    fn with_rune_trees(mut sd: StaticData) -> StaticData {
        let style = |id: u32, rows: &[&[u32]]| RuneStyle {
            id,
            name: String::new(),
            icon: String::new(),
            slots: rows
                .iter()
                .map(|row| {
                    row.iter()
                        .map(|&id| RuneInfo {
                            id,
                            name: String::new(),
                            icon: String::new(),
                            short_desc: String::new(),
                        })
                        .collect()
                })
                .collect(),
        };
        sd.rune_styles = vec![
            style(
                8000,
                &[
                    &[8005, 8008, 8021, 8010],
                    &[9101, 9111, 8009],
                    &[9104, 9105, 9103],
                    &[8014, 8017, 8299],
                ],
            ),
            style(
                8400,
                &[
                    &[8437, 8439, 8465],
                    &[8446, 8463, 8401],
                    &[8429, 8444, 8473],
                    &[8451, 8453, 8242],
                ],
            ),
        ];
        sd
    }

    /// u.gg's order for Yorick top (keystone, then the rest sorted by id).
    const UGG_YORICK_PERKS: [u32; 6] = [8437, 8009, 8446, 8451, 8473, 9103];

    #[test]
    fn perks_are_put_in_client_order() {
        let sd = with_rune_trees(static_data());
        let mut runes = sample_build().runes;
        runes.perks = UGG_YORICK_PERKS.to_vec();
        assert_eq!(
            ordered_perks(&runes, Some(&sd)),
            // Grasp, Demolish, Bone Plating, Overgrowth | Presence of Mind, Bloodline
            vec![8437, 8446, 8473, 8451, 8009, 9103]
        );
        // Precision primary (another u.gg row): Conqueror, Triumph, Legend, Last Stand.
        let precision = RunePage {
            primary_style: 8000,
            sub_style: 8400,
            perks: vec![8010, 8299, 8446, 8473, 9103, 9111],
            ..runes.clone()
        };
        assert_eq!(
            ordered_perks(&precision, Some(&sd)),
            vec![8010, 9111, 9103, 8299, 8446, 8473]
        );
        // Unknown trees or perks that don't fit them: left alone.
        assert_eq!(ordered_perks(&runes, None), UGG_YORICK_PERKS.to_vec());
        assert_eq!(
            ordered_perks(&runes, Some(&static_data())),
            UGG_YORICK_PERKS.to_vec()
        );
        let mut odd = runes.clone();
        odd.perks[1] = 8010; // a second keystone
        assert_eq!(ordered_perks(&odd, Some(&sd)), odd.perks);
    }

    #[tokio::test]
    async fn imported_page_lists_perks_in_client_order() {
        let (mock, client) = mock_client(|_| {}).await;
        let mut build = sample_build();
        build.runes.perks = UGG_YORICK_PERKS.to_vec();
        let sd = with_rune_trees(static_data());
        let result = client
            .import_build(&build, &runes_only(), Some(&sd), None)
            .await;
        assert!(result.runes, "{result:?}");
        let m = mock.lock().unwrap();
        assert_eq!(
            m.pages.last().unwrap()["selectedPerkIds"],
            json!([8437, 8446, 8473, 8451, 8009, 9103, 5008, 5001, 5001])
        );
    }

    #[test]
    fn uid_shape() {
        let a = new_uid(83);
        assert_eq!(a.len(), 36);
        assert_eq!(a.matches('-').count(), 4);
        assert_ne!(a, new_uid(83));
    }

    #[test]
    fn free_page_slot_detection() {
        let pages = fixture("perks_pages.json");
        let pages = pages.as_array().unwrap();
        let inventory = fixture("perks_inventory.json");
        assert!(no_free_page_slot(pages, Some(&inventory)));
        assert!(!no_free_page_slot(
            pages,
            Some(&json!({"ownedPageCount": 4}))
        ));
        assert!(no_free_page_slot(
            pages,
            Some(&json!({"ownedPageCount": 3}))
        ));
        assert!(!no_free_page_slot(pages, None));
    }

    // --- against a mock League client ---------------------------------------

    #[derive(Default)]
    pub(crate) struct Mock {
        pages: Vec<Value>,
        owned_pages: u64,
        next_page_id: u64,
        /// POST of a page with this name fails (simulates a client error).
        fail_post_named: Option<String>,
        pub(crate) session: Option<Value>,
        gameflow: Value,
        summoner: Value,
        item_sets: Value,
        requests: Vec<(String, String, Value)>,
        auth: Vec<String>,
    }

    impl Mock {
        fn new() -> Mock {
            Mock {
                pages: fixture("perks_pages.json").as_array().unwrap().clone(),
                owned_pages: fixture("perks_inventory.json")["ownedPageCount"]
                    .as_u64()
                    .unwrap(),
                next_page_id: 2_000_000_000,
                gameflow: fixture("gameflow_session.json"),
                summoner: fixture("current_summoner.json"),
                item_sets: fixture("item_sets.json"),
                ..Default::default()
            }
        }

        fn user_pages(&self) -> u64 {
            self.pages.iter().filter(|p| is_user_page(p)).count() as u64
        }

        fn page_names(&self) -> Vec<String> {
            self.pages
                .iter()
                .map(|p| page_name(p).to_string())
                .collect()
        }

        pub(crate) fn requests_to(&self, method: &str, prefix: &str) -> usize {
            self.requests
                .iter()
                .filter(|(m, p, _)| m == method && p.starts_with(prefix))
                .count()
        }

        fn handle(&mut self, method: &str, path: &str, body: Value) -> (u16, Value) {
            self.requests
                .push((method.into(), path.into(), body.clone()));
            let not_found = (
                404,
                json!({"errorCode": "RPC_ERROR", "httpStatus": 404, "message": "Resource not found"}),
            );
            match (method, path) {
                ("GET", "/lol-gameflow/v1/gameflow-phase") => (200, json!("ChampSelect")),
                ("GET", "/lol-gameflow/v1/session") => (200, self.gameflow.clone()),
                ("GET", "/lol-summoner/v1/current-summoner") => (200, self.summoner.clone()),
                ("GET", "/lol-champ-select/v1/session") => match &self.session {
                    Some(s) => (200, s.clone()),
                    None => (
                        404,
                        json!({"errorCode": "RPC_ERROR", "httpStatus": 404, "message": "No active delegate"}),
                    ),
                },
                ("GET", "/lol-perks/v1/pages") => (200, Value::Array(self.pages.clone())),
                ("GET", "/lol-perks/v1/inventory") => {
                    let n = self.user_pages();
                    (
                        200,
                        json!({
                            "canAddCustomPage": n < self.owned_pages,
                            "customPageCount": n,
                            "isCustomPageCreationUnlocked": true,
                            "ownedPageCount": self.owned_pages,
                        }),
                    )
                }
                ("POST", "/lol-perks/v1/pages") => {
                    if self.fail_post_named.as_deref() == body["name"].as_str() {
                        return (500, json!({"httpStatus": 500, "message": "Invalid page"}));
                    }
                    if self.user_pages() >= self.owned_pages {
                        return (
                            400,
                            json!({"errorCode": "RPC_ERROR", "httpStatus": 400, "message": "Max pages reached"}),
                        );
                    }
                    if flag(&body, "current") {
                        for p in &mut self.pages {
                            p["current"] = json!(false);
                        }
                    }
                    let mut page = body;
                    page["id"] = json!(self.next_page_id);
                    page["isEditable"] = json!(true);
                    page["isDeletable"] = json!(true);
                    page["isTemporary"] = json!(false);
                    self.next_page_id += 1;
                    self.pages.push(page.clone());
                    (200, page)
                }
                ("DELETE", p) if p.starts_with("/lol-perks/v1/pages/") => {
                    let id: u64 = p.rsplit('/').next().unwrap().parse().unwrap();
                    match self.pages.iter().position(|pg| page_id(pg) == Some(id)) {
                        Some(i) if flag(&self.pages[i], "isDeletable") => {
                            self.pages.remove(i);
                            (204, Value::Null)
                        }
                        Some(_) => (400, json!({"message": "Page is not deletable"})),
                        None => not_found,
                    }
                }
                ("GET", "/lol-item-sets/v1/item-sets/123456789/sets") => {
                    (200, self.item_sets.clone())
                }
                ("PUT", "/lol-item-sets/v1/item-sets/123456789/sets") => {
                    self.item_sets = body;
                    (201, Value::Null)
                }
                _ => not_found,
            }
        }
    }

    /// Minimal HTTP/1.1 server around `Mock` (one request per connection).
    async fn serve(mock: Arc<StdMutex<Mock>>) -> LcuClient {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let mock = mock.clone();
                tokio::spawn(async move {
                    let (read, mut write) = stream.into_split();
                    let mut reader = BufReader::new(read);
                    let mut request_line = String::new();
                    if reader.read_line(&mut request_line).await.unwrap_or(0) == 0 {
                        return;
                    }
                    let mut parts = request_line.split_whitespace();
                    let method = parts.next().unwrap_or("").to_string();
                    let path = parts.next().unwrap_or("").to_string();
                    let (mut length, mut auth) = (0usize, String::new());
                    loop {
                        let mut line = String::new();
                        reader.read_line(&mut line).await.unwrap();
                        let line = line.trim_end();
                        if line.is_empty() {
                            break;
                        }
                        if let Some((k, v)) = line.split_once(':') {
                            if k.eq_ignore_ascii_case("content-length") {
                                length = v.trim().parse().unwrap();
                            } else if k.eq_ignore_ascii_case("authorization") {
                                auth = v.trim().to_string();
                            }
                        }
                    }
                    let mut body = vec![0; length];
                    reader.read_exact(&mut body).await.unwrap();
                    let body = if body.is_empty() {
                        Value::Null
                    } else {
                        serde_json::from_slice(&body).unwrap()
                    };
                    let (status, resp) = {
                        let mut m = mock.lock().unwrap();
                        m.auth.push(auth);
                        m.handle(&method, &path, body)
                    };
                    let text = if resp.is_null() {
                        String::new()
                    } else {
                        resp.to_string()
                    };
                    let head = format!(
                        "HTTP/1.1 {status} Mock\r\nContent-Type: application/json\r\n\
                         Content-Length: {}\r\nConnection: close\r\n\r\n",
                        text.len()
                    );
                    write.write_all(head.as_bytes()).await.unwrap();
                    write.write_all(text.as_bytes()).await.unwrap();
                    let _ = write.shutdown().await;
                });
            }
        });
        LcuClient::from_base_url(format!("http://{addr}"), "test").unwrap()
    }

    /// A client whose port has nothing listening (League closed/restarting).
    pub(crate) async fn unreachable_client() -> LcuClient {
        let port = {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            listener.local_addr().unwrap().port()
        };
        LcuClient::from_base_url(format!("http://127.0.0.1:{port}"), "x").unwrap()
    }

    pub(crate) async fn mock_client(
        setup: impl FnOnce(&mut Mock),
    ) -> (Arc<StdMutex<Mock>>, LcuClient) {
        let mut mock = Mock::new();
        setup(&mut mock);
        let mock = Arc::new(StdMutex::new(mock));
        let client = serve(mock.clone()).await;
        (mock, client)
    }

    const MY_CONQUEROR: u64 = 1981829302;
    const DEFAULT_PAGE: u64 = 50;

    /// No `CSH:` page and every slot used.
    fn full_without_our_page(m: &mut Mock) {
        m.pages.retain(|p| !page_name(p).starts_with(PREFIX));
        m.owned_pages = 2;
    }

    fn runes_only() -> Settings {
        Settings {
            import_item_set: false,
            ..Settings::default()
        }
    }

    #[tokio::test]
    async fn import_replaces_our_page_and_item_set_and_never_touches_spells() {
        let (mock, client) =
            mock_client(|m| m.session = Some(fixture("champ_select_ranked.json"))).await;
        let sd = static_data();
        let result = client
            .import_build(&sample_build(), &Settings::default(), Some(&sd), None)
            .await;
        assert!(result.runes && result.item_set, "{result:?}");
        assert_eq!(result.needs_confirmation, None);

        let m = mock.lock().unwrap();
        assert!(m.auth.iter().all(|a| a == "Basic cmlvdDp0ZXN0"));
        // Old CSH page replaced, the user's and the default page untouched.
        assert_eq!(
            m.page_names(),
            vec![
                "My Conqueror",
                "Mid Electrocute",
                "Domination",
                "CSH: Yorick vs Gwen"
            ]
        );
        let new_page = m.pages.last().unwrap();
        assert_eq!(new_page["primaryStyleId"], json!(8400));
        assert_eq!(new_page["subStyleId"], json!(8000));
        assert_eq!(
            new_page["selectedPerkIds"],
            json!([8437, 8446, 8429, 8451, 9111, 9105, 5008, 5001, 5001])
        );
        assert_eq!(new_page["current"], json!(true));
        assert_eq!(m.requests_to("DELETE", "/lol-perks/v1/pages/"), 1);
        // Never the "delete ALL pages" endpoint, never summoner spells.
        assert!(!m
            .requests
            .iter()
            .any(|(meth, p, _)| meth == "DELETE" && p == "/lol-perks/v1/pages"));
        assert!(!m
            .requests
            .iter()
            .any(|(_, p, _)| p.contains("my-selection")));

        let titles: Vec<_> = m.item_sets["itemSets"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["title"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(
            titles,
            vec!["My own Yorick set", "CSH: Gwen Top", "CSH: Yorick vs Gwen"]
        );
        assert_eq!(m.item_sets["accountId"], json!(2345678901234567u64));
    }

    #[tokio::test]
    async fn unreadable_item_sets_are_never_overwritten() {
        // A 2xx answer without the list (empty body, error object): a PUT
        // would replace every item set the user has with ours.
        for answer in [
            Value::Null,
            json!({}),
            json!({"itemSets": null}),
            json!("oops"),
        ] {
            let (mock, client) = mock_client(|m| m.item_sets = answer.clone()).await;
            let settings = Settings {
                import_runes: false,
                ..Settings::default()
            };
            let result = client
                .import_build(&sample_build(), &settings, None, None)
                .await;
            assert!(!result.item_set, "{answer}: {result:?}");
            assert_eq!(
                mock.lock().unwrap().requests_to("PUT", "/lol-item-sets"),
                0,
                "{answer}"
            );
        }
    }

    #[tokio::test]
    async fn import_respects_toggles() {
        let (mock, client) = mock_client(|_| {}).await;
        let settings = Settings {
            import_runes: false,
            import_item_set: false,
            ..Settings::default()
        };
        let result = client
            .import_build(&sample_build(), &settings, None, None)
            .await;
        assert_eq!(result, ImportResult::default());
        assert!(mock.lock().unwrap().requests.is_empty());
    }

    #[tokio::test]
    async fn full_slots_ask_before_overwriting_a_user_page() {
        let (mock, client) = mock_client(full_without_our_page).await;
        let result = client
            .import_build(&sample_build(), &runes_only(), None, None)
            .await;
        assert!(!result.runes);
        assert_eq!(
            result.needs_confirmation,
            Some(RunePageRef {
                id: MY_CONQUEROR,
                name: "My Conqueror".into()
            })
        );
        assert!(
            result.messages[0].contains("My Conqueror"),
            "{:?}",
            result.messages
        );
        let m = mock.lock().unwrap();
        assert_eq!(m.requests_to("DELETE", "/lol-perks"), 0);
        assert_eq!(
            m.page_names(),
            vec!["My Conqueror", "Mid Electrocute", "Domination"]
        );
    }

    #[tokio::test]
    async fn full_slots_overwrite_after_confirmation() {
        let (mock, client) = mock_client(full_without_our_page).await;
        let result = client
            .import_build(
                &sample_build(),
                &runes_only(),
                Some(&static_data()),
                Some(MY_CONQUEROR),
            )
            .await;
        assert!(result.runes, "{result:?}");
        assert_eq!(result.needs_confirmation, None);
        assert!(result
            .messages
            .iter()
            .any(|m| m.contains("Replaced your rune page \"My Conqueror\"")));
        let m = mock.lock().unwrap();
        assert_eq!(
            m.page_names(),
            vec!["Mid Electrocute", "Domination", "CSH: Yorick vs Gwen"]
        );
    }

    #[tokio::test]
    async fn confirmation_not_needed_when_a_slot_is_free() {
        // Our old page is reused: the user's page isn't deleted even if an
        // overwrite id is passed.
        let (mock, client) = mock_client(|_| {}).await;
        let result = client
            .import_build(&sample_build(), &runes_only(), None, Some(MY_CONQUEROR))
            .await;
        assert!(result.runes);
        assert!(mock
            .lock()
            .unwrap()
            .page_names()
            .contains(&"My Conqueror".to_string()));
    }

    #[tokio::test]
    async fn never_overwrites_a_default_page() {
        let (mock, client) = mock_client(|m| {
            full_without_our_page(m);
            for p in &mut m.pages {
                p["current"] = json!(page_id(p) == Some(DEFAULT_PAGE));
            }
        })
        .await;
        let asked = client
            .import_build(&sample_build(), &runes_only(), None, None)
            .await;
        assert!(!asked.runes);
        assert_eq!(asked.needs_confirmation, None);
        assert!(
            asked.messages[0].contains("can't be replaced"),
            "{:?}",
            asked.messages
        );

        let forced = client
            .import_build(&sample_build(), &runes_only(), None, Some(DEFAULT_PAGE))
            .await;
        assert!(!forced.runes);
        assert_eq!(mock.lock().unwrap().requests_to("DELETE", "/lol-perks"), 0);
    }

    #[tokio::test]
    async fn failed_overwrite_restores_the_user_page() {
        let (mock, client) = mock_client(|m| {
            full_without_our_page(m);
            m.fail_post_named = Some("CSH: Yorick vs Gwen".into());
        })
        .await;
        let result = client
            .import_build(
                &sample_build(),
                &runes_only(),
                Some(&static_data()),
                Some(MY_CONQUEROR),
            )
            .await;
        assert!(!result.runes);
        let m = mock.lock().unwrap();
        assert_eq!(
            m.page_names(),
            vec!["Mid Electrocute", "Domination", "My Conqueror"]
        );
        let restored = m.pages.last().unwrap();
        assert_eq!(
            restored["selectedPerkIds"],
            json!([8010, 9111, 9105, 8299, 8444, 8451, 5005, 5008, 5001])
        );
    }

    #[tokio::test]
    async fn incomplete_rune_page_is_rejected_without_requests() {
        let (mock, client) = mock_client(|_| {}).await;
        let mut build = sample_build();
        build.runes.shards.clear();
        let result = client.import_build(&build, &runes_only(), None, None).await;
        assert!(!result.runes);
        assert!(mock.lock().unwrap().requests.is_empty());
    }

    #[tokio::test]
    async fn status_and_champ_select_over_http() {
        let (mock, client) = mock_client(|_| {}).await;
        let status = client.status().await.unwrap();
        assert_eq!(
            status,
            LcuStatus {
                connected: true,
                summoner_name: Some("Yorick Main".into()),
                phase: "ChampSelect".into(),
            }
        );
        // Summoner name is cached.
        client.status().await.unwrap();
        assert_eq!(mock.lock().unwrap().requests_to("GET", "/lol-summoner"), 1);

        // Not in champ select → default state.
        assert_eq!(
            client.champ_select(&roles()).await.unwrap(),
            ChampSelectState::default()
        );

        mock.lock().unwrap().session = Some(fixture("champ_select_ranked.json"));
        let cs = client.champ_select(&roles()).await.unwrap();
        assert_eq!(cs.queue, Some(Queue::RankedSolo));
        assert_eq!(cs.lane_opponent_id, Some(122));

        // Gameflow without queue data → the session's own queueId.
        {
            let mut m = mock.lock().unwrap();
            m.gameflow = json!({"phase": "ChampSelect"});
            m.session = Some(fixture("champ_select_aram.json"));
        }
        let cs = client.champ_select(&roles()).await.unwrap();
        assert_eq!(cs.queue, Some(Queue::Aram));
    }

    #[tokio::test]
    async fn errors_when_client_is_gone() {
        let port = {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            listener.local_addr().unwrap().port()
        };
        let client = LcuClient::from_base_url(format!("http://127.0.0.1:{port}"), "x").unwrap();
        assert!(client.status().await.is_err());
        assert!(client.champ_select(&roles()).await.is_err());
        let result = client
            .import_build(&sample_build(), &Settings::default(), None, None)
            .await;
        assert!(!result.runes && !result.item_set);
        assert_eq!(result.messages.len(), 2);
    }
}
