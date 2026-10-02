//! A fake League client (LCU) for end-to-end tests.
//!
//! A small HTTP/1.1 server on 127.0.0.1 (one request per connection) backed by
//! an in-memory [`FakeState`]: gameflow phase/session, current summoner, a
//! scripted champ select session, rune pages (with the client's
//! editable/deletable/current flags and a page-slot limit), the perks
//! inventory and per-summoner item sets. Every request is recorded so tests
//! can assert exactly what the app wrote.
//!
//! The production client talks HTTPS to the real League client; tests point
//! it at this server over plain HTTP via `LcuClient::for_test`.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use base64::Engine as _;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

use crate::lcu::LcuClient;

pub const SUMMONER_ID: u64 = 123456789;
pub const ACCOUNT_ID: u64 = 2345678901234567;
/// Ids of the pages in `tests/fixtures/lcu/perks_pages.json`.
pub const PAGE_OLD_CSH: u64 = 1981829301;
pub const PAGE_MY_CONQUEROR: u64 = 1981829302;
pub const PAGE_MID_ELECTROCUTE: u64 = 1981829303;
pub const PAGE_DEFAULT: u64 = 50;

pub fn lcu_fixture(name: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/lcu")
        .join(name);
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap()
}

/// One recorded HTTP request.
#[derive(Debug, Clone)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub body: Value,
}

impl Request {
    pub fn is_write(&self) -> bool {
        self.method != "GET"
    }
}

pub struct FakeState {
    /// Password the client must send (HTTP Basic `riot:<token>`), else 401.
    pub token: String,
    /// `/lol-gameflow/v1/gameflow-phase`, e.g. "Lobby", "ChampSelect".
    pub phase: String,
    /// Queue id in `/lol-gameflow/v1/session` (None → no `gameData.queue`).
    pub queue_id: Option<i64>,
    /// `/lol-champ-select/v1/session`; None → 404 "No active delegate".
    pub session: Option<Value>,
    pub summoner: Value,
    pub pages: Vec<Value>,
    /// Custom rune page slots the account owns.
    pub owned_pages: u64,
    next_page_id: u64,
    /// Item set documents by summoner id.
    pub item_sets: HashMap<u64, Value>,
    /// Raw body of the last item set PUT.
    pub last_item_set_put: Option<String>,
    /// Style id → rows of perk ids (row 0 = keystones), for validating
    /// created rune pages like the client does (`isValid`).
    rune_styles: HashMap<u32, Vec<Vec<u32>>>,
    /// Why created pages were invalid (empty = all valid).
    pub invalid_pages: Vec<String>,
    /// Every request, in order.
    pub log: Vec<Request>,
    /// After this many more requests the "client" stops answering: the
    /// connection is closed without a response (process crashed mid-call).
    pub crash_after: Option<usize>,
    /// path → HTTP status to answer once (e.g. a 503 while the client is busy).
    pub fail_once: HashMap<String, u16>,
}

impl FakeState {
    fn new() -> FakeState {
        let mut item_sets = HashMap::new();
        item_sets.insert(SUMMONER_ID, lcu_fixture("item_sets.json"));
        let runes: Value = serde_json::from_slice(&crate::http_cache::test_util::fixture(
            "ddragon_runesReforged.json",
        ))
        .unwrap();
        let rune_styles = runes
            .as_array()
            .unwrap()
            .iter()
            .map(|style| {
                let rows = style["slots"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|slot| {
                        slot["runes"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|r| r["id"].as_u64().unwrap() as u32)
                            .collect()
                    })
                    .collect();
                (style["id"].as_u64().unwrap() as u32, rows)
            })
            .collect();
        FakeState {
            token: "fake-token".into(),
            phase: "None".into(),
            queue_id: None,
            session: None,
            summoner: lcu_fixture("current_summoner.json"),
            pages: lcu_fixture("perks_pages.json").as_array().unwrap().clone(),
            owned_pages: 5,
            next_page_id: 2_000_000_000,
            item_sets,
            last_item_set_put: None,
            rune_styles,
            invalid_pages: Vec::new(),
            log: Vec::new(),
            crash_after: None,
            fail_once: HashMap::new(),
        }
    }

    // --- assertions helpers ---------------------------------------------

    pub fn writes(&self) -> Vec<Request> {
        self.log.iter().filter(|r| r.is_write()).cloned().collect()
    }

    pub fn count(&self, method: &str, path_prefix: &str) -> usize {
        self.log
            .iter()
            .filter(|r| r.method == method && r.path.starts_with(path_prefix))
            .count()
    }

    pub fn page(&self, id: u64) -> Option<&Value> {
        self.pages.iter().find(|p| p["id"].as_u64() == Some(id))
    }

    pub fn page_names(&self) -> Vec<String> {
        self.pages
            .iter()
            .map(|p| p["name"].as_str().unwrap_or("").to_string())
            .collect()
    }

    pub fn our_pages(&self) -> Vec<&Value> {
        self.pages
            .iter()
            .filter(|p| p["name"].as_str().unwrap_or("").starts_with("CSH:"))
            .collect()
    }

    pub fn item_set_doc(&self) -> &Value {
        &self.item_sets[&SUMMONER_ID]
    }

    pub fn item_sets(&self) -> Vec<Value> {
        self.item_set_doc()["itemSets"]
            .as_array()
            .cloned()
            .unwrap_or_default()
    }

    // --- setup helpers ----------------------------------------------------

    /// Remove our `CSH:` pages and make every custom slot used.
    pub fn fill_page_slots_without_our_page(&mut self) {
        self.pages
            .retain(|p| !p["name"].as_str().unwrap_or("").starts_with("CSH:"));
        self.owned_pages = self.custom_pages();
    }

    /// The user edits a rune page in the client (not an app request).
    pub fn user_edits_page(&mut self, id: u64, perks: Value) {
        let page = self
            .pages
            .iter_mut()
            .find(|p| p["id"].as_u64() == Some(id))
            .expect("page to edit");
        page["selectedPerkIds"] = perks;
        page["lastModified"] = json!(1791999999999u64);
    }

    fn custom_pages(&self) -> u64 {
        self.pages
            .iter()
            .filter(|p| flag(p, "isDeletable") && !flag(p, "isTemporary"))
            .count() as u64
    }

    // --- request handling -------------------------------------------------

    fn handle(&mut self, method: &str, path: &str, auth: &str, body: Value) -> (u16, Value) {
        self.log.push(Request {
            method: method.into(),
            path: path.into(),
            body: body.clone(),
        });
        let expected = format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode(format!("riot:{}", self.token))
        );
        if let Some(status) = self.fail_once.remove(path) {
            return (
                status,
                json!({"errorCode": "RPC_ERROR", "httpStatus": status, "message": "Service unavailable"}),
            );
        }
        if auth != expected {
            return (
                401,
                json!({"errorCode": "UNAUTHORIZED", "httpStatus": 401, "message": "Unauthorized"}),
            );
        }
        let not_found = |what: &str| {
            (
                404,
                json!({"errorCode": "RPC_ERROR", "httpStatus": 404, "message": what}),
            )
        };
        let sets_path = format!("/lol-item-sets/v1/item-sets/{SUMMONER_ID}/sets");
        match (method, path) {
            ("GET", "/lol-gameflow/v1/gameflow-phase") => (200, json!(self.phase)),
            ("GET", "/lol-gameflow/v1/session") => {
                if self.phase == "None" {
                    return not_found("No gameflow session");
                }
                let mut flow = lcu_fixture("gameflow_session.json");
                flow["phase"] = json!(self.phase);
                match self.queue_id {
                    Some(q) => flow["gameData"]["queue"]["id"] = json!(q),
                    None => flow["gameData"] = json!({}),
                }
                (200, flow)
            }
            ("GET", "/lol-summoner/v1/current-summoner") => (200, self.summoner.clone()),
            ("GET", "/lol-champ-select/v1/session") => match &self.session {
                Some(s) => (200, s.clone()),
                None => not_found("No active delegate"),
            },
            (_, p) if p.starts_with("/lol-champ-select/") => {
                // e.g. PATCH .../session/my-selection (summoner spells):
                // recorded so tests can assert it never happens.
                (204, Value::Null)
            }
            ("GET", "/lol-perks/v1/pages") => (200, Value::Array(self.pages.clone())),
            ("GET", "/lol-perks/v1/currentpage") => {
                match self.pages.iter().find(|p| flag(p, "current")) {
                    Some(p) => (200, p.clone()),
                    None => not_found("No current page"),
                }
            }
            ("GET", "/lol-perks/v1/inventory") => {
                let custom = self.custom_pages();
                (
                    200,
                    json!({
                        "canAddCustomPage": custom < self.owned_pages,
                        "customPageCount": custom,
                        "isCustomPageCreationUnlocked": true,
                        "ownedPageCount": self.owned_pages,
                    }),
                )
            }
            ("POST", "/lol-perks/v1/pages") => self.create_page(body),
            ("DELETE", "/lol-perks/v1/pages") => {
                // "Delete all pages": must never be used by the app.
                self.pages
                    .retain(|p| !(flag(p, "isDeletable") && !flag(p, "isTemporary")));
                (204, Value::Null)
            }
            ("DELETE", p) if p.starts_with("/lol-perks/v1/pages/") => {
                let Some(id) = p.rsplit('/').next().and_then(|s| s.parse::<u64>().ok()) else {
                    return (400, json!({"message": "bad id"}));
                };
                match self
                    .pages
                    .iter()
                    .position(|pg| pg["id"].as_u64() == Some(id))
                {
                    Some(i) if flag(&self.pages[i], "isDeletable") => {
                        self.pages.remove(i);
                        (204, Value::Null)
                    }
                    Some(_) => (
                        400,
                        json!({"httpStatus": 400, "message": "Page is not deletable"}),
                    ),
                    None => not_found("Page not found"),
                }
            }
            ("PUT", p) if p.starts_with("/lol-perks/v1/pages/") => {
                // Editing a page in place (the app never needs this).
                (400, json!({"message": "not supported by the fake"}))
            }
            ("GET", p) if p.starts_with("/lol-item-sets/v1/item-sets/") => {
                if p != sets_path {
                    return not_found("Unknown summoner");
                }
                (200, self.item_set_doc().clone())
            }
            ("PUT", p) if p.starts_with("/lol-item-sets/v1/item-sets/") => {
                if p != sets_path {
                    return not_found("Unknown summoner");
                }
                if !body["itemSets"].is_array() {
                    return (400, json!({"message": "itemSets missing"}));
                }
                self.item_sets.insert(SUMMONER_ID, body);
                (201, Value::Null)
            }
            _ => not_found("Unknown endpoint"),
        }
    }

    fn create_page(&mut self, body: Value) -> (u16, Value) {
        if self.custom_pages() >= self.owned_pages {
            return (
                400,
                json!({"errorCode": "RPC_ERROR", "httpStatus": 400, "message": "Max pages reached"}),
            );
        }
        let problem = self.validate_page(&body);
        if let Some(problem) = &problem {
            self.invalid_pages.push(format!(
                "{}: {problem}",
                body["name"].as_str().unwrap_or("?")
            ));
        }
        if flag(&body, "current") {
            for p in &mut self.pages {
                p["current"] = json!(false);
                p["isActive"] = json!(false);
            }
        }
        let mut page = body;
        page["id"] = json!(self.next_page_id);
        page["isEditable"] = json!(true);
        page["isDeletable"] = json!(true);
        page["isTemporary"] = json!(false);
        page["isActive"] = page["current"].clone();
        page["isValid"] = json!(problem.is_none());
        self.next_page_id += 1;
        self.pages.push(page.clone());
        (200, page)
    }

    /// Like the client: 9 perk ids (4 primary rows incl. keystone, 2 from
    /// different secondary rows, 3 shards), distinct primary/sub styles.
    fn validate_page(&self, page: &Value) -> Option<String> {
        let name = page["name"].as_str().unwrap_or("");
        if name.is_empty() {
            return Some("empty name".into());
        }
        if name.chars().count() > 25 {
            return Some(format!("name longer than 25 characters ({name:?})"));
        }
        let style = |k: &str| page[k].as_u64().map(|v| v as u32);
        let (Some(primary), Some(sub)) = (style("primaryStyleId"), style("subStyleId")) else {
            return Some("missing style ids".into());
        };
        if primary == sub {
            return Some("primary style == sub style".into());
        }
        let perks: Vec<u32> = page["selectedPerkIds"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_u64())
                    .map(|v| v as u32)
                    .collect()
            })
            .unwrap_or_default();
        if perks.len() != 9 {
            return Some(format!("{} perk ids instead of 9", perks.len()));
        }
        let (Some(p_rows), Some(s_rows)) =
            (self.rune_styles.get(&primary), self.rune_styles.get(&sub))
        else {
            return None; // style not in the trimmed fixture: can't check
        };
        for (row, perk) in perks[..4].iter().enumerate() {
            if !p_rows.get(row).is_some_and(|r| r.contains(perk)) {
                return Some(format!("perk {perk} not in primary row {row}"));
            }
        }
        let sub_row = |perk: u32| (1..s_rows.len()).find(|&r| s_rows[r].contains(&perk));
        match (sub_row(perks[4]), sub_row(perks[5])) {
            (Some(a), Some(b)) if a != b => {}
            _ => return Some(format!("bad secondary perks {:?}", &perks[4..6])),
        }
        const SHARD_ROWS: [&[u32]; 3] = [
            &[5008, 5005, 5007],
            &[5008, 5010, 5001],
            &[5011, 5013, 5001],
        ];
        for (row, shard) in perks[6..].iter().enumerate() {
            if !SHARD_ROWS[row].contains(shard) {
                return Some(format!("shard {shard} not valid in row {row}"));
            }
        }
        None
    }
}

fn flag(v: &Value, key: &str) -> bool {
    v.get(key).and_then(Value::as_bool).unwrap_or(false)
}

/// A running fake client. Dropping it stops the server.
pub struct FakeLcu {
    pub base: String,
    state: Arc<Mutex<FakeState>>,
    server: JoinHandle<()>,
}

impl FakeLcu {
    pub async fn start(setup: impl FnOnce(&mut FakeState)) -> FakeLcu {
        let mut state = FakeState::new();
        setup(&mut state);
        let state = Arc::new(Mutex::new(state));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let shared = state.clone();
        let server = tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let state = shared.clone();
                tokio::spawn(async move {
                    let _ = serve_one(stream, state).await;
                });
            }
        });
        FakeLcu {
            base,
            state,
            server,
        }
    }

    pub fn state(&self) -> MutexGuard<'_, FakeState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// A production `LcuClient` pointed at this fake.
    pub fn client(&self) -> LcuClient {
        let token = self.state().token.clone();
        LcuClient::for_test(&self.base, &token)
    }

    /// The League client process exits: the port stops accepting connections.
    pub fn shutdown(&self) {
        self.server.abort();
    }
}

impl Drop for FakeLcu {
    fn drop(&mut self) {
        self.server.abort();
    }
}

async fn serve_one(
    stream: tokio::net::TcpStream,
    state: Arc<Mutex<FakeState>>,
) -> std::io::Result<()> {
    let (read, mut write) = stream.into_split();
    let mut reader = BufReader::new(read);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).await? == 0 {
        return Ok(());
    }
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("").to_string();
    let (mut length, mut auth) = (0usize, String::new());
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).await? == 0 {
            break;
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            if k.eq_ignore_ascii_case("content-length") {
                length = v.trim().parse().unwrap_or(0);
            } else if k.eq_ignore_ascii_case("authorization") {
                auth = v.trim().to_string();
            }
        }
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).await?;
    let raw = String::from_utf8_lossy(&body).into_owned();
    let body = if body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&body).unwrap_or(Value::String(raw.clone()))
    };

    let response = {
        let mut st = state.lock().unwrap_or_else(|e| e.into_inner());
        match st.crash_after {
            Some(0) => None,
            Some(n) => {
                st.crash_after = Some(n - 1);
                Some(st.handle(&method, &path, &auth, body))
            }
            None => Some(st.handle(&method, &path, &auth, body)),
        }
        .inspect(|_| {
            if method == "PUT" && path.starts_with("/lol-item-sets/") {
                st.last_item_set_put = Some(raw.clone());
            }
        })
    };
    let Some((status, resp)) = response else {
        // Crashed: close without answering.
        return Ok(());
    };
    let text = if resp.is_null() {
        String::new()
    } else {
        resp.to_string()
    };
    let head = format!(
        "HTTP/1.1 {status} Fake\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n",
        text.len()
    );
    write.write_all(head.as_bytes()).await?;
    write.write_all(text.as_bytes()).await?;
    write.shutdown().await
}

// ---------------------------------------------------------------------------
// Scripted champ select sessions
// ---------------------------------------------------------------------------

fn next_game_id() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(7212345678);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// Builds `/lol-champ-select/v1/session` documents shaped like the real
/// client's, step by step: planning intent → hover → lock → enemy picks.
#[derive(Clone)]
pub struct Draft {
    pub queue_id: i64,
    /// `gameId`: every new champ select (`Draft::ranked()`, …) gets its own,
    /// steps derived from it (`.hover()`, `.lock()`, …) keep it.
    pub game_id: u64,
    pub local_cell: i64,
    /// (cellId, assignedPosition, championId, championPickIntent)
    my_team: Vec<(i64, &'static str, u32, u32)>,
    /// (cellId, championId) — positions are hidden for the enemy team.
    their_team: Vec<(i64, u32)>,
    /// Pick actions: (actorCellId, championId, completed, isInProgress).
    picks: Vec<(i64, u32, bool, bool)>,
    timer_phase: &'static str,
    /// ARAM: no pick actions, a bench.
    bench: Vec<u32>,
}

impl Draft {
    /// Ranked solo/duo, me = cell 2 (top), nobody picked yet.
    pub fn ranked() -> Draft {
        Draft {
            queue_id: 420,
            game_id: next_game_id(),
            local_cell: 2,
            my_team: vec![
                (0, "jungle", 0, 0),
                (1, "middle", 0, 0),
                (2, "top", 0, 0),
                (3, "bottom", 0, 0),
                (4, "utility", 0, 0),
            ],
            their_team: (5..10).map(|c| (c, 0)).collect(),
            picks: (0..10).map(|c| (c, 0, false, false)).collect(),
            timer_phase: "PLANNING",
            bench: Vec::new(),
        }
    }

    /// Normal blind pick (430): no assigned positions, simultaneous picks.
    pub fn blind() -> Draft {
        let mut d = Draft::ranked();
        d.queue_id = 430;
        for p in &mut d.my_team {
            p.1 = "";
        }
        d
    }

    /// ARAM (queue 450) / Mayhem (2400): champion assigned, no pick actions.
    pub fn aram(queue_id: i64, my_champion: u32, bench: Vec<u32>) -> Draft {
        Draft {
            queue_id,
            game_id: next_game_id(),
            local_cell: 7,
            my_team: vec![
                (5, "", 222, 0),
                (6, "", 412, 0),
                (7, "", my_champion, 0),
                (8, "", 103, 0),
                (9, "", 86, 0),
            ],
            their_team: Vec::new(),
            picks: Vec::new(),
            timer_phase: "BAN_PICK",
            bench,
        }
    }

    fn me(&mut self) -> &mut (i64, &'static str, u32, u32) {
        let cell = self.local_cell;
        self.my_team.iter_mut().find(|p| p.0 == cell).unwrap()
    }

    fn my_pick(&mut self) -> &mut (i64, u32, bool, bool) {
        let cell = self.local_cell;
        self.picks.iter_mut().find(|p| p.0 == cell).unwrap()
    }

    /// Planning phase: declare an intent.
    pub fn intent(mut self, champion: u32) -> Draft {
        self.me().3 = champion;
        self
    }

    /// My pick turn: hovering a champion (not locked).
    pub fn hover(mut self, champion: u32) -> Draft {
        self.timer_phase = "BAN_PICK";
        self.me().2 = champion;
        *self.my_pick() = (self.local_cell, champion, false, true);
        self
    }

    /// Lock in the hovered champion.
    pub fn lock(mut self) -> Draft {
        let champion = self.me().2;
        assert_ne!(champion, 0, "hover before locking");
        *self.my_pick() = (self.local_cell, champion, true, false);
        self
    }

    /// Enemy `slot` (0..5) locks `champion`.
    pub fn enemy_locks(mut self, slot: usize, champion: u32) -> Draft {
        self.timer_phase = "BAN_PICK";
        let cell = self.their_team[slot].0;
        self.their_team[slot].1 = champion;
        let pick = self.picks.iter_mut().find(|p| p.0 == cell).unwrap();
        *pick = (cell, champion, true, false);
        self
    }

    /// ARAM bench swap / reroll: my champion changes.
    pub fn swap_to(mut self, champion: u32) -> Draft {
        let old = self.me().2;
        self.me().2 = champion;
        self.bench.retain(|&c| c != champion);
        self.bench.push(old);
        self
    }

    /// Champion trade with a teammate after locking in.
    pub fn traded_to(mut self, champion: u32) -> Draft {
        self.timer_phase = "FINALIZATION";
        self.me().2 = champion;
        self
    }

    pub fn finalization(mut self) -> Draft {
        self.timer_phase = "FINALIZATION";
        self
    }

    pub fn session(&self) -> Value {
        let my_team: Vec<Value> = self
            .my_team
            .iter()
            .map(|&(cell, pos, champ, intent)| {
                json!({
                    "assignedPosition": pos,
                    "cellId": cell,
                    "championId": champ,
                    "championPickIntent": intent,
                    "gameName": "",
                    "isHumanoid": false,
                    "nameVisibilityType": "VISIBLE",
                    "playerType": "PLAYER",
                    "selectedSkinId": champ * 1000,
                    "spell1Id": 4,
                    "spell2Id": 12,
                    "summonerId": if cell == self.local_cell { super::fake_lcu::SUMMONER_ID } else { 0 },
                    "team": 1,
                    "wardSkinId": -1,
                })
            })
            .collect();
        let their_team: Vec<Value> = self
            .their_team
            .iter()
            .map(|&(cell, champ)| {
                json!({
                    "assignedPosition": "",
                    "cellId": cell,
                    "championId": champ,
                    "championPickIntent": 0,
                    "nameVisibilityType": "HIDDEN",
                    "spell1Id": 0,
                    "spell2Id": 0,
                    "summonerId": 0,
                    "team": 2,
                })
            })
            .collect();
        let mut id = 0;
        let actions: Vec<Value> = self
            .picks
            .iter()
            .map(|&(actor, champ, completed, in_progress)| {
                id += 1;
                json!([{
                    "actorCellId": actor,
                    "championId": champ,
                    "completed": completed,
                    "id": id,
                    "isAllyAction": actor < 5,
                    "isInProgress": in_progress,
                    "pickTurn": 1,
                    "type": "pick",
                }])
            })
            .collect();
        json!({
            "actions": actions,
            "allowRerolling": !self.bench.is_empty(),
            "bans": {"myTeamBans": [], "numBans": 0, "theirTeamBans": []},
            "benchChampions": self.bench.iter().map(|c| json!({"championId": c, "isPriority": false})).collect::<Vec<_>>(),
            "benchEnabled": !self.bench.is_empty(),
            "gameId": self.game_id,
            "hasSimultaneousBans": true,
            "hasSimultaneousPicks": self.picks.is_empty(),
            "isCustomGame": false,
            "isSpectating": false,
            "localPlayerCellId": self.local_cell,
            "myTeam": my_team,
            "queueId": self.queue_id,
            "theirTeam": their_team,
            "timer": {"adjustedTimeLeftInPhase": 20000, "isInfinite": false, "phase": self.timer_phase, "totalTimeInPhase": 30000},
            "trades": [],
        })
    }
}
