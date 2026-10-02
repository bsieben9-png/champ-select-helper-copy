//! End-to-end scenarios: the real watcher loop (`Watcher::tick`) and the real
//! Tauri commands, against the fake League client, with a mock Tauri app and
//! offline u.gg / Data Dragon fixtures (no network, no League).
//!
//! Each tick is one iteration of the background loop (1 s apart in the app).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};
use tauri::test::MockRuntime;
use tauri::{Listener, Manager};
use tokio::sync::RwLock;

use super::fake_lcu::*;
use crate::http_cache::cache_file_name;
use crate::http_cache::test_util::{fixture, temp_dir};
use crate::model::*;
use crate::watcher::Watcher;
use crate::{ddragon, ugg, AppState};

const YORICK: u32 = 83;
const GWEN: u32 = 887;
const LEE_SIN: u32 = 64;
const JINX: u32 = 222;
const ASHE: u32 = 22;

/// Polls outside champ select after which the watcher counts it as left.
const LEAVE_POLLS: usize = 3;

const UGG: &str = "https://stats2.u.gg/lol/1.5";
const UGG_VERSIONS: &str =
    "https://static.bigbrain.gg/assets/lol/riot_patch_update/prod/ugg/ugg-api-versions.json";
const DDRAGON: &str = "https://ddragon.leagueoflegends.com";

fn seed(dir: &Path, url: &str, fixture_name: &str) {
    std::fs::write(dir.join(cache_file_name(url)), fixture(fixture_name)).unwrap();
}

/// Offline u.gg + Data Dragon with the trimmed fixtures (patch 16_19).
/// Yorick has general, ARAM and vs-Gwen data; Ashe gets a copy of Yorick's
/// ARAM file (for ARAM swaps). Anything else counts as "no data" (403).
fn offline_sources(name: &str) -> (ugg::Ugg, ddragon::DDragon, PathBuf) {
    let dir = temp_dir(name);
    let (u, d) = (dir.join("ugg"), dir.join("ddragon"));
    std::fs::create_dir_all(&u).unwrap();
    std::fs::create_dir_all(&d).unwrap();
    let p = "16_19";
    seed(&u, UGG_VERSIONS, "ugg-api-versions.json");
    seed(
        &u,
        &format!("{UGG}/overview/{p}/ranked_solo_5x5/83/1.5.0.json"),
        "overview_83_ranked_solo_5x5.json",
    );
    for champ in [YORICK, ASHE] {
        seed(
            &u,
            &format!("{UGG}/overview/{p}/normal_aram/{champ}/1.5.0.json"),
            "overview_83_normal_aram.json",
        );
    }
    seed(
        &u,
        &format!("{UGG}/overview/{p}/ranked_solo_5x5/matchups/83_887/1.5.0.json"),
        "overview_matchup_83_887_ranked_solo_5x5.json",
    );
    seed(
        &u,
        &format!("{UGG}/matchups/{p}/ranked_solo_5x5/83/1.5.0.json"),
        "matchups_83_ranked_solo_5x5.json",
    );
    seed(
        &u,
        &format!("{UGG}/primary_roles/{p}/1.5.0.json"),
        "primary_roles.json",
    );
    seed(
        &u,
        &format!("{UGG}/champion_ranking/world/{p}/ranked_solo_5x5/emerald_plus/1.5.0.json"),
        "champion_ranking_world_emerald_plus.json",
    );
    seed(
        &d,
        &format!("{DDRAGON}/api/versions.json"),
        "ddragon_versions.json",
    );
    for file in ["champion", "item", "runesReforged", "summoner"] {
        seed(
            &d,
            &format!("{DDRAGON}/cdn/16.19.1/data/en_US/{file}.json"),
            &format!("ddragon_{file}.json"),
        );
    }
    (
        ugg::Ugg::new_offline(u),
        ddragon::DDragon::new_offline(d),
        dir,
    )
}

/// The app (mock Tauri runtime + real `AppState`), the real watcher (with
/// its "already imported" marker file), and a fake League client the watcher
/// is already connected to.
struct Harness {
    app: tauri::App<MockRuntime>,
    watcher: Watcher,
    fake: FakeLcu,
    events: Arc<Mutex<Vec<(&'static str, Value)>>>,
    dir: PathBuf,
}

impl Harness {
    async fn new(name: &str, settings: Settings, setup: impl FnOnce(&mut FakeState)) -> Harness {
        let fake = FakeLcu::start(setup).await;
        let (ugg, ddragon, dir) = offline_sources(name);
        let state = AppState {
            ugg,
            ddragon,
            settings: RwLock::new(settings),
            settings_path: dir.join("settings.json"),
            lcu: RwLock::new(Some(fake.client())),
            lcu_status: RwLock::new(LcuStatus::default()),
            champ_select: RwLock::new(ChampSelectState::default()),
            static_data: RwLock::new(None),
        };
        // The app's real commands, callable through the mock IPC.
        let app = tauri::test::mock_builder()
            .invoke_handler(tauri::generate_handler![
                crate::get_static_data,
                crate::get_build,
                crate::get_counters,
                crate::get_tier_list,
                crate::get_matchups,
                crate::get_settings,
                crate::save_settings,
                crate::get_champ_select,
                crate::get_lcu_status,
                crate::import_build,
            ])
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        app.manage(state);
        let events = Arc::new(Mutex::new(Vec::new()));
        for event in ["lcu-status", "champ-select", "auto-imported"] {
            let events = events.clone();
            app.listen_any(event, move |e| {
                let payload = serde_json::from_str(e.payload()).unwrap();
                events.lock().unwrap().push((event, payload));
            });
        }
        Harness {
            app,
            watcher: Watcher::new(dir.join("last_import.json")),
            fake,
            events,
            dir,
        }
    }

    fn state(&self) -> tauri::State<'_, AppState> {
        self.app.state::<AppState>()
    }

    fn ugg(&self) -> &ugg::Ugg {
        &self.state().inner().ugg
    }

    fn marker(&self) -> PathBuf {
        self.dir.join("last_import.json")
    }

    /// The app is closed and started again: a new watcher (same marker file).
    fn restart_app(&mut self) {
        self.watcher = Watcher::new(self.marker());
    }

    /// One poll of the background loop, plus the background work it started
    /// (u.gg answering, the import) — in the app that's well under a second.
    async fn tick(&mut self) -> Duration {
        let delay = self.poll().await;
        self.settle().await;
        delay
    }

    /// Only the poll itself: it must never wait for u.gg.
    async fn poll(&mut self) -> Duration {
        tokio::time::timeout(Duration::from_secs(5), self.watcher.tick(self.app.handle()))
            .await
            .expect("the poll waited for something slow (u.gg?)")
    }

    /// Wait for the background work (roles, the import) to finish.
    async fn settle(&mut self) {
        tokio::time::timeout(Duration::from_secs(20), self.watcher.settle())
            .await
            .expect("background work didn't finish");
    }

    async fn ticks(&mut self, n: usize) {
        for _ in 0..n {
            self.tick().await;
        }
    }

    /// Set the client's gameflow phase and champ select session.
    fn client_shows(&self, phase: &str, draft: Option<&Draft>) {
        let mut st = self.fake.state();
        st.phase = phase.into();
        st.queue_id = draft.map(|d| d.queue_id);
        st.session = draft.map(Draft::session);
    }

    fn events(&self, name: &str) -> Vec<Value> {
        self.events
            .lock()
            .unwrap()
            .iter()
            .filter(|(n, _)| *n == name)
            .map(|(_, v)| v.clone())
            .collect()
    }

    fn auto_imports(&self) -> Vec<AutoImportEvent> {
        self.events("auto-imported")
            .into_iter()
            .map(|v| serde_json::from_value(v).unwrap())
            .collect()
    }

    async fn champ_select(&self) -> ChampSelectState {
        self.state().champ_select.read().await.clone()
    }

    async fn lcu_status(&self) -> LcuStatus {
        self.state().lcu_status.read().await.clone()
    }

    /// Writes (non-GET requests) as "METHOD path".
    fn writes(&self) -> Vec<String> {
        self.fake
            .state()
            .writes()
            .iter()
            .map(|r| format!("{} {}", r.method, r.path))
            .collect()
    }

    /// What the UI does for the Import button / the overwrite dialog: fetch
    /// the build (`get_build`), then `import_build` — the real commands.
    async fn manual_import(
        &self,
        role: Option<Role>,
        opponent: Option<u32>,
        queue: Queue,
        overwrite_page_id: Option<u64>,
    ) -> ImportResult {
        let build = crate::get_build(self.state(), YORICK, role, opponent, queue)
            .await
            .unwrap();
        crate::import_build(self.state(), build, overwrite_page_id)
            .await
            .unwrap()
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Invariants that must hold after any scenario.
fn assert_never_touched_spells_or_defaults(fake: &FakeState) {
    for r in &fake.log {
        assert!(
            !r.path.contains("my-selection")
                && !(r.is_write() && r.path.starts_with("/lol-champ-select")),
            "the app must never change champ select / summoner spells: {} {}",
            r.method,
            r.path
        );
        assert!(
            !(r.method == "DELETE" && r.path == "/lol-perks/v1/pages"),
            "the app must never delete all rune pages"
        );
        assert!(
            !(r.is_write() && r.path == format!("/lol-perks/v1/pages/{PAGE_DEFAULT}")),
            "the app touched the default (non-editable) page: {} {}",
            r.method,
            r.path
        );
        assert!(
            !(r.method == "PUT" && r.path.starts_with("/lol-perks/v1/pages/")),
            "the app edited a rune page in place: {}",
            r.path
        );
    }
    // The default page is unchanged (apart from the client moving the
    // "current" flag to a newly created page).
    let mut default = fake
        .page(PAGE_DEFAULT)
        .expect("default page still exists")
        .clone();
    let original = lcu_fixture("perks_pages.json")[3].clone();
    assert_eq!(original["id"], json!(PAGE_DEFAULT));
    default["current"] = original["current"].clone();
    default["isActive"] = original["isActive"].clone();
    assert_eq!(default, original, "default page changed");
    assert!(
        fake.invalid_pages.is_empty(),
        "invalid rune pages created: {:?}",
        fake.invalid_pages
    );
}

// ---------------------------------------------------------------------------
// Ranked: exactly one auto-import, at lock-in
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ranked_auto_import_runs_once_at_lock_in_and_never_again() {
    let mut h = Harness::new("e2e-ranked", Settings::default(), |_| {}).await;

    h.client_shows("Lobby", None);
    h.tick().await;
    let status = h.lcu_status().await;
    assert!(status.connected);
    assert_eq!(status.phase, "Lobby");
    assert_eq!(status.summoner_name.as_deref(), Some("Yorick Main"));

    // Planning phase: I declare Yorick.
    let draft = Draft::ranked().intent(YORICK);
    h.client_shows("ChampSelect", Some(&draft));
    h.ticks(2).await;
    let cs = h.champ_select().await;
    assert!(cs.in_champ_select);
    assert_eq!(cs.queue, Some(Queue::RankedSolo));
    assert_eq!(cs.my_role, Some(Role::Top));
    assert_eq!(cs.my_champion_id, Some(YORICK));
    assert!(!cs.my_champion_locked);

    // Enemy jungle + ADC lock; my turn: hovering Yorick.
    let draft = draft
        .enemy_locks(1, LEE_SIN)
        .enemy_locks(3, JINX)
        .hover(YORICK);
    h.client_shows("ChampSelect", Some(&draft));
    h.ticks(3).await;
    assert!(
        h.writes().is_empty(),
        "imported before lock-in: {:?}",
        h.writes()
    );
    assert!(h.auto_imports().is_empty());

    // Lock in. Enemy top not known yet → general build.
    let draft = draft.lock();
    h.client_shows("ChampSelect", Some(&draft));
    h.tick().await;
    assert_eq!(
        h.writes(),
        vec![
            format!("DELETE /lol-perks/v1/pages/{PAGE_OLD_CSH}"),
            "POST /lol-perks/v1/pages".to_string(),
            format!("PUT /lol-item-sets/v1/item-sets/{SUMMONER_ID}/sets"),
        ]
    );
    let imports = h.auto_imports();
    assert_eq!(imports.len(), 1);
    assert_eq!(imports[0].champion_id, YORICK);
    assert_eq!(imports[0].opponent_id, None);
    assert!(
        imports[0].result.runes && imports[0].result.item_set,
        "{:?}",
        imports[0].result
    );
    assert_eq!(imports[0].result.needs_confirmation, None);
    let our_page_id = {
        let st = h.fake.state();
        // Perks in the client's slot order: Resolve keystone, rows 1-3, then
        // the two Precision perks by row, then shards (u.gg lists them as
        // keystone + the rest sorted by id).
        let post = st.log.iter().find(|r| r.method == "POST").unwrap();
        assert_eq!(
            post.body["selectedPerkIds"],
            json!([8437, 8446, 8473, 8451, 8009, 9103, 5005, 5008, 5001])
        );
        assert_eq!(post.body["primaryStyleId"], json!(8400));
        assert_eq!(post.body["subStyleId"], json!(8000));
        let ours = st.our_pages();
        assert_eq!(ours.len(), 1);
        assert_eq!(ours[0]["name"], json!("CSH: Yorick Top"));
        assert_eq!(ours[0]["current"], json!(true));
        ours[0]["id"].as_u64().unwrap()
    };

    h.ticks(3).await;

    // The enemy top laner locks Gwen afterwards: shown, but no re-import.
    let draft = draft.enemy_locks(0, GWEN);
    h.client_shows("ChampSelect", Some(&draft));
    h.ticks(2).await;
    let cs = h.champ_select().await;
    assert_eq!(cs.lane_opponent_id, Some(GWEN));
    assert!(cs.my_champion_locked);

    // The user edits the imported page and one of their own pages.
    let edited = json!([8437, 8463, 8444, 8451, 9111, 9105, 5008, 5001, 5011]);
    {
        let mut st = h.fake.state();
        st.user_edits_page(our_page_id, edited.clone());
        st.user_edits_page(
            PAGE_MY_CONQUEROR,
            json!([8010, 9111, 9104, 8299, 8444, 8451, 5005, 5008, 5001]),
        );
    }
    h.ticks(3).await;

    // Finalization, game start, in game.
    h.client_shows("ChampSelect", Some(&draft.clone().finalization()));
    h.ticks(3).await;
    h.client_shows("GameStart", None);
    h.tick().await;
    h.client_shows("InProgress", None);
    h.ticks(2).await;

    assert_eq!(
        h.writes().len(),
        3,
        "writes after lock-in: {:?}",
        h.writes()
    );
    assert_eq!(h.auto_imports().len(), 1);
    assert!(!h.champ_select().await.in_champ_select);
    assert_eq!(h.lcu_status().await.phase, "InProgress");
    let st = h.fake.state();
    assert_eq!(st.count("POST", "/lol-perks/v1/pages"), 1);
    assert_eq!(st.count("PUT", "/lol-item-sets/"), 1);
    assert_eq!(st.page(our_page_id).unwrap()["selectedPerkIds"], edited);
    assert_never_touched_spells_or_defaults(&st);
}

#[tokio::test]
async fn trade_after_lock_in_does_not_reimport() {
    let mut h = Harness::new("e2e-trade", Settings::default(), |_| {}).await;
    let draft = Draft::ranked().hover(YORICK).lock();
    h.client_shows("ChampSelect", Some(&draft));
    h.ticks(2).await;
    assert_eq!(h.auto_imports().len(), 1);
    h.client_shows("ChampSelect", Some(&draft.traded_to(GWEN)));
    h.ticks(3).await;
    assert_eq!(h.champ_select().await.my_champion_id, Some(GWEN));
    assert_eq!(h.auto_imports().len(), 1, "a trade must not re-import");
    assert_eq!(h.writes().len(), 3);
}

#[tokio::test]
async fn blind_pick_imports_the_most_played_role_build() {
    let mut h = Harness::new("e2e-blind", Settings::default(), |_| {}).await;
    let draft = Draft::blind().hover(YORICK);
    h.client_shows("ChampSelect", Some(&draft));
    h.ticks(2).await;
    let cs = h.champ_select().await;
    assert_eq!((cs.queue, cs.my_role), (Some(Queue::NormalBlind), None));
    assert!(h.auto_imports().is_empty());
    h.client_shows("ChampSelect", Some(&draft.lock()));
    h.ticks(2).await;
    let imports = h.auto_imports();
    assert_eq!(imports.len(), 1);
    assert!(imports[0].result.runes && imports[0].result.item_set);
    let st = h.fake.state();
    assert_eq!(st.our_pages()[0]["name"], json!("CSH: Yorick Top"));
    assert_never_touched_spells_or_defaults(&st);
}

#[tokio::test]
async fn auto_import_off_writes_nothing() {
    let settings = Settings {
        auto_import: false,
        ..Settings::default()
    };
    let mut h = Harness::new("e2e-off", settings, |_| {}).await;
    let draft = Draft::ranked().intent(YORICK);
    h.client_shows("ChampSelect", Some(&draft));
    h.ticks(2).await;
    let draft = draft.hover(YORICK).lock();
    h.client_shows("ChampSelect", Some(&draft));
    h.ticks(3).await;
    let draft = draft.enemy_locks(0, GWEN);
    h.client_shows("ChampSelect", Some(&draft));
    h.ticks(3).await;

    assert!(h.champ_select().await.my_champion_locked);
    assert!(h.writes().is_empty(), "{:?}", h.writes());
    assert!(h.auto_imports().is_empty());
}

#[tokio::test]
async fn auto_import_with_runes_and_item_set_off_does_nothing() {
    let settings = Settings {
        import_runes: false,
        import_item_set: false,
        ..Settings::default()
    };
    let mut h = Harness::new("e2e-toggles", settings, |_| {}).await;
    h.client_shows("ChampSelect", Some(&Draft::ranked().hover(YORICK).lock()));
    h.ticks(3).await;
    assert!(h.writes().is_empty(), "{:?}", h.writes());
    // No "Couldn't import — nothing was imported" event for the UI either.
    assert!(h.auto_imports().is_empty(), "{:?}", h.auto_imports());
}

// ---------------------------------------------------------------------------
// Rune pages
// ---------------------------------------------------------------------------

#[tokio::test]
async fn full_rune_pages_ask_first_then_replace_only_the_confirmed_page() {
    let mut h = Harness::new("e2e-full", Settings::default(), |st| {
        st.fill_page_slots_without_our_page();
    })
    .await;
    let pages_before = h.fake.state().pages.clone();

    h.client_shows("ChampSelect", Some(&Draft::ranked().hover(YORICK).lock()));
    h.ticks(3).await;

    // Auto-import: no page deleted, the user's current page is offered.
    let imports = h.auto_imports();
    assert_eq!(imports.len(), 1);
    let result = &imports[0].result;
    assert!(!result.runes);
    assert!(result.item_set, "item set is imported anyway: {result:?}");
    assert_eq!(
        result.needs_confirmation,
        Some(RunePageRef {
            id: PAGE_MY_CONQUEROR,
            name: "My Conqueror".into()
        })
    );
    {
        let st = h.fake.state();
        assert_eq!(st.count("DELETE", "/lol-perks"), 0);
        assert_eq!(st.pages, pages_before, "pages changed without consent");
    }

    // The user confirms in the dialog → import_build(build, overwrite id).
    let result = h
        .manual_import(
            Some(Role::Top),
            None,
            Queue::RankedSolo,
            Some(PAGE_MY_CONQUEROR),
        )
        .await;
    assert!(result.runes, "{result:?}");
    assert_eq!(result.needs_confirmation, None);
    h.ticks(2).await;

    let st = h.fake.state();
    let deletes: Vec<_> = st
        .writes()
        .into_iter()
        .filter(|r| r.method == "DELETE")
        .map(|r| r.path)
        .collect();
    assert_eq!(
        deletes,
        vec![format!("/lol-perks/v1/pages/{PAGE_MY_CONQUEROR}")]
    );
    assert_eq!(
        st.page_names(),
        vec!["Mid Electrocute", "Domination", "CSH: Yorick Top"]
    );
    // The other pages are untouched.
    for id in [PAGE_MID_ELECTROCUTE, PAGE_DEFAULT] {
        let before = pages_before.iter().find(|p| p["id"] == json!(id)).unwrap();
        assert_eq!(st.page(id).unwrap(), before);
    }
    assert_never_touched_spells_or_defaults(&st);
}

#[tokio::test]
async fn full_rune_pages_with_our_page_replace_it_and_keep_user_pages() {
    let mut h = Harness::new("e2e-full-ours", Settings::default(), |st| {
        // CSH page + 2 user pages, 3 slots: full.
        st.owned_pages = 3;
    })
    .await;
    let pages_before = h.fake.state().pages.clone();
    h.client_shows("ChampSelect", Some(&Draft::ranked().hover(YORICK).lock()));
    h.ticks(2).await;

    let imports = h.auto_imports();
    assert_eq!(imports.len(), 1);
    assert!(imports[0].result.runes, "{:?}", imports[0].result);
    assert_eq!(imports[0].result.needs_confirmation, None);
    let st = h.fake.state();
    assert_eq!(
        st.page_names(),
        vec![
            "My Conqueror",
            "Mid Electrocute",
            "Domination",
            "CSH: Yorick Top"
        ]
    );
    for id in [PAGE_MY_CONQUEROR, PAGE_MID_ELECTROCUTE, PAGE_DEFAULT] {
        let before = pages_before.iter().find(|p| p["id"] == json!(id)).unwrap();
        let mut after = st.page(id).unwrap().clone();
        // Making our page current un-sets "current" on the others — that's
        // the client's doing, not an edit by the app.
        after["current"] = before["current"].clone();
        after["isActive"] = before["isActive"].clone();
        assert_eq!(&after, before);
    }
    assert_never_touched_spells_or_defaults(&st);
}

#[tokio::test]
async fn full_rune_pages_never_offer_or_replace_a_default_page() {
    let mut h = Harness::new("e2e-full-default", Settings::default(), |st| {
        st.fill_page_slots_without_our_page();
        for p in &mut st.pages {
            p["current"] = json!(p["id"] == json!(PAGE_DEFAULT));
        }
    })
    .await;
    h.client_shows("ChampSelect", Some(&Draft::ranked().hover(YORICK).lock()));
    h.ticks(2).await;
    let imports = h.auto_imports();
    assert_eq!(imports.len(), 1);
    assert!(!imports[0].result.runes);
    assert_eq!(imports[0].result.needs_confirmation, None);

    // Even if asked to, the default page is never replaced.
    let result = h
        .manual_import(Some(Role::Top), None, Queue::RankedSolo, Some(PAGE_DEFAULT))
        .await;
    assert!(!result.runes);
    let st = h.fake.state();
    assert_eq!(st.count("DELETE", "/lol-perks"), 0);
    assert_never_touched_spells_or_defaults(&st);
}

// ---------------------------------------------------------------------------
// Item sets
// ---------------------------------------------------------------------------

/// A user item set document with things the app doesn't know about.
fn exotic_item_sets() -> Value {
    json!({
        "accountId": ACCOUNT_ID,
        "itemSets": [
            {
                "associatedChampions": [83], "associatedMaps": [11, 12],
                "blocks": [{"hideIfSummonerSpell": "", "items": [{"count": 1, "id": "3078"}],
                            "showIfSummonerSpell": "", "type": "Ünïcödé ✓ \"quoted\" \\ block"}],
                "map": "any", "mode": "any",
                "preferredItemSlots": [{"id": "3078", "preferredItemSlot": 1}],
                "sortrank": 2, "startedFrom": "blank", "title": "My own Yorick set",
                "type": "custom", "uid": "1c6f1a2b-3d4e-4f5a-b7c8-d9e0f1a2b3c4",
                "someFutureField": {"nested": [1, 2.5, null, true, "x"], "big": 9007199254740993u64}
            },
            {
                "associatedChampions": [83], "associatedMaps": [11], "blocks": [],
                "map": "any", "mode": "any", "preferredItemSlots": [], "sortrank": 0,
                "startedFrom": "blank", "title": "CSH: Yorick vs Darius", "type": "custom",
                "uid": "0b5e0f1a-2c3d-4e5f-a6b7-c8d9e0f1a2b3"
            },
            {
                "associatedChampions": [887], "associatedMaps": [11], "blocks": [],
                "map": "any", "mode": "any", "preferredItemSlots": [], "sortrank": 0,
                "startedFrom": "blank", "title": "CSH: Gwen Top", "type": "custom",
                "uid": "2d7a2b3c-4e5f-4a6b-c8d9-e0f1a2b3c4d5"
            },
            {
                "associatedChampions": [83], "associatedMaps": [], "blocks": [],
                "map": "SR", "mode": "CLASSIC", "preferredItemSlots": [], "sortrank": 5,
                "startedFrom": "blank", "title": "csh: lowercase is the user's", "type": "custom",
                "uid": "3e8b3c4d-5f60-4b7c-d9e0-f1a2b3c4d5e6"
            },
            {
                "associatedChampions": [], "associatedMaps": [], "blocks": [],
                "map": "any", "mode": "any", "preferredItemSlots": [], "sortrank": 0.5,
                "startedFrom": "recommended", "title": "Global set", "type": "global",
                "uid": "4f9c4d5e-6071-4c8d-e0f1-a2b3c4d5e6f7"
            }
        ],
        "timestamp": 1790900000000u64,
        "someTopLevelField": {"keep": ["me", 1]}
    })
}

fn is_ours_for_yorick(set: &Value) -> bool {
    set["title"].as_str().unwrap_or("").starts_with("CSH:")
        && set["associatedChampions"]
            .as_array()
            .is_some_and(|c| c.contains(&json!(YORICK)))
}

#[tokio::test]
async fn item_sets_keep_user_sets_and_replace_ours() {
    let mut h = Harness::new("e2e-itemsets", Settings::default(), |st| {
        st.item_sets.insert(SUMMONER_ID, exotic_item_sets());
    })
    .await;
    let before = exotic_item_sets();
    let others_before: Vec<Value> = before["itemSets"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| !is_ours_for_yorick(s))
        .cloned()
        .collect();

    let check = |st: &FakeState, title: &str| {
        let doc = st.item_set_doc();
        assert_eq!(doc["accountId"], json!(ACCOUNT_ID));
        assert_eq!(doc["someTopLevelField"], before["someTopLevelField"]);
        let sets = st.item_sets();
        let others: Vec<Value> = sets
            .iter()
            .filter(|s| !is_ours_for_yorick(s))
            .cloned()
            .collect();
        // Same sets, same order, byte-for-byte once serialized.
        assert_eq!(others.len(), others_before.len());
        for (a, b) in others.iter().zip(&others_before) {
            assert_eq!(
                serde_json::to_string(a).unwrap(),
                serde_json::to_string(b).unwrap()
            );
        }
        let ours: Vec<&Value> = sets.iter().filter(|s| is_ours_for_yorick(s)).collect();
        assert_eq!(ours.len(), 1, "exactly one CSH set for Yorick: {sets:#?}");
        assert_eq!(ours[0]["title"], json!(title));
        assert_eq!(ours[0]["associatedMaps"], json!([11]));
        assert!(!ours[0]["blocks"].as_array().unwrap().is_empty());
        // The raw request body is valid JSON with the same content.
        let raw = st.last_item_set_put.as_deref().unwrap();
        assert_eq!(&serde_json::from_str::<Value>(raw).unwrap(), doc);
    };

    // Auto-import at lock-in (no lane opponent yet).
    h.client_shows("ChampSelect", Some(&Draft::ranked().hover(YORICK).lock()));
    h.ticks(2).await;
    check(&h.fake.state(), "CSH: Yorick Top");

    // Gwen shows up; the user presses "Import matchup build".
    let result = h
        .manual_import(Some(Role::Top), Some(GWEN), Queue::RankedSolo, None)
        .await;
    assert!(result.item_set, "{result:?}");
    check(&h.fake.state(), "CSH: Yorick vs Gwen");
    let st = h.fake.state();
    assert_eq!(st.count("PUT", "/lol-item-sets/"), 2);
    // The rune page was replaced too, not duplicated.
    let ours: Vec<_> = st.our_pages().iter().map(|p| p["name"].clone()).collect();
    assert_eq!(ours, vec![json!("CSH: Yorick vs Gwen")]);
    assert_never_touched_spells_or_defaults(&st);
}

#[tokio::test]
async fn item_sets_for_a_new_account_without_sets() {
    let mut h = Harness::new("e2e-itemsets-empty", Settings::default(), |st| {
        st.item_sets.insert(
            SUMMONER_ID,
            json!({"accountId": 0, "itemSets": [], "timestamp": 0}),
        );
    })
    .await;
    h.client_shows("ChampSelect", Some(&Draft::ranked().hover(YORICK).lock()));
    h.ticks(2).await;
    let st = h.fake.state();
    assert_eq!(st.item_sets().len(), 1);
    assert!(is_ours_for_yorick(&st.item_sets()[0]));
}

// ---------------------------------------------------------------------------
// Dodges, ARAM, disconnects
// ---------------------------------------------------------------------------

#[tokio::test]
async fn dodge_and_new_champ_select_imports_again_once() {
    let mut h = Harness::new("e2e-dodge", Settings::default(), |_| {}).await;
    h.client_shows("ChampSelect", Some(&Draft::ranked().hover(YORICK).lock()));
    h.ticks(3).await;
    assert_eq!(h.auto_imports().len(), 1);

    // Someone dodges: back to the lobby / queue.
    h.client_shows("Lobby", None);
    h.tick().await;
    assert!(!h.champ_select().await.in_champ_select);
    h.client_shows("Matchmaking", None);
    h.tick().await;
    h.client_shows("ReadyCheck", None);
    h.tick().await;

    // New champ select: this time Gwen is locked top before I lock.
    let draft = Draft::ranked().enemy_locks(0, GWEN).hover(YORICK);
    h.client_shows("ChampSelect", Some(&draft));
    h.ticks(2).await;
    assert_eq!(h.auto_imports().len(), 1);
    h.client_shows("ChampSelect", Some(&draft.lock()));
    h.ticks(4).await;

    let imports = h.auto_imports();
    assert_eq!(imports.len(), 2);
    assert_eq!(imports[1].opponent_id, Some(GWEN));
    assert!(imports[1].result.runes && imports[1].result.item_set);
    let st = h.fake.state();
    assert_eq!(st.count("POST", "/lol-perks/v1/pages"), 2);
    assert_eq!(st.count("PUT", "/lol-item-sets/"), 2);
    let ours: Vec<_> = st.our_pages().iter().map(|p| p["name"].clone()).collect();
    assert_eq!(ours, vec![json!("CSH: Yorick vs Gwen")]);
    assert_eq!(
        st.item_sets()
            .iter()
            .filter(|s| is_ours_for_yorick(s))
            .count(),
        1
    );
    assert_never_touched_spells_or_defaults(&st);
}

#[tokio::test]
async fn aram_imports_once_per_champion() {
    for queue_id in [450, 2400, 2450] {
        let mut h = Harness::new("e2e-aram", Settings::default(), |_| {}).await;
        let draft = Draft::aram(queue_id, YORICK, vec![ASHE, 99]);
        h.client_shows("ChampSelect", Some(&draft));
        h.ticks(3).await;
        let imports = h.auto_imports();
        assert_eq!(imports.len(), 1, "queue {queue_id}");
        assert!(imports[0].result.runes && imports[0].result.item_set);
        {
            let st = h.fake.state();
            assert_eq!(st.our_pages()[0]["name"], json!("CSH: Yorick ARAM"));
            let ours: Vec<_> = st
                .item_sets()
                .into_iter()
                .filter(is_ours_for_yorick)
                .collect();
            assert_eq!(ours[0]["associatedMaps"], json!([12]), "Howling Abyss");
        }

        // Bench swap → a new champion → imported once.
        h.client_shows("ChampSelect", Some(&draft.swap_to(ASHE)));
        h.ticks(3).await;
        let imports = h.auto_imports();
        assert_eq!(imports.len(), 2, "queue {queue_id}");
        assert_eq!(imports[1].champion_id, ASHE);
        let st = h.fake.state();
        assert_eq!(st.count("POST", "/lol-perks/v1/pages"), 2);
        assert_eq!(st.our_pages().len(), 1);
        assert_never_touched_spells_or_defaults(&st);
    }
}

#[tokio::test]
async fn client_disappearing_mid_select_disconnects_cleanly() {
    let mut h = Harness::new("e2e-gone", Settings::default(), |_| {}).await;
    h.client_shows(
        "ChampSelect",
        Some(&Draft::ranked().intent(YORICK).hover(YORICK)),
    );
    h.ticks(2).await;
    assert!(h.champ_select().await.in_champ_select);

    h.fake.shutdown();
    let delay = h.tick().await;
    assert!(delay > Duration::from_secs(1), "back to discovery polling");
    let status = h.lcu_status().await;
    assert!(!status.connected);
    assert_eq!(status.phase, "None");
    assert_eq!(h.champ_select().await, ChampSelectState::default());
    assert!(h.state().lcu.read().await.is_none());
    // The UI was told.
    let last_status = h.events("lcu-status").pop().unwrap();
    assert_eq!(last_status["connected"], json!(false));
    let last_cs = h.events("champ-select").pop().unwrap();
    assert_eq!(last_cs["in_champ_select"], json!(false));
    // Keeps going without a client.
    h.ticks(3).await;
    assert!(!h.lcu_status().await.connected);
    // Manual import without a client: an error, not a panic.
    let build = crate::get_build(h.state(), YORICK, Some(Role::Top), None, Queue::RankedSolo)
        .await
        .unwrap();
    assert!(crate::import_build(h.state(), build, None).await.is_err());
}

#[tokio::test]
async fn client_crashing_during_the_import_does_not_panic() {
    let mut h = Harness::new("e2e-crash", Settings::default(), |_| {}).await;
    let draft = Draft::ranked().hover(YORICK);
    h.client_shows("ChampSelect", Some(&draft));
    h.ticks(2).await;
    {
        let mut st = h.fake.state();
        st.session = Some(draft.lock().session());
        // gameflow-phase, session, gameflow session, (build ready) session
        // check, GET pages, DELETE old page — then the client dies (the POST
        // gets no answer).
        st.crash_after = Some(6);
    }
    h.tick().await;
    let imports = h.auto_imports();
    assert_eq!(imports.len(), 1);
    let result = &imports[0].result;
    assert!(!result.runes && !result.item_set, "{result:?}");
    assert_eq!(result.messages.len(), 2, "{:?}", result.messages);
    h.ticks(2).await;
    assert!(!h.lcu_status().await.connected);
}

#[tokio::test]
async fn client_restarted_with_new_credentials_disconnects() {
    let mut h = Harness::new("e2e-401", Settings::default(), |_| {}).await;
    h.client_shows("Lobby", None);
    h.tick().await;
    assert!(h.lcu_status().await.connected);
    h.fake.state().token = "a-new-password".into();
    h.tick().await;
    assert!(!h.lcu_status().await.connected);
    assert!(h.state().lcu.read().await.is_none());
}

// ---------------------------------------------------------------------------
// Robustness: a single failed poll after lock-in
// ---------------------------------------------------------------------------

/// Lock in, get the auto-import, then the user edits the imported page.
/// Returns the page id and the edited perks.
async fn locked_in_and_edited(h: &mut Harness) -> (u64, Value) {
    let draft = Draft::ranked().hover(YORICK).lock();
    locked_in_and_edited_in(h, &draft).await
}

async fn locked_in_and_edited_in(h: &mut Harness, draft: &Draft) -> (u64, Value) {
    h.client_shows("ChampSelect", Some(draft));
    h.ticks(2).await;
    assert_eq!(h.auto_imports().len(), 1);
    let our_page = h.fake.state().our_pages()[0]["id"].as_u64().unwrap();
    let edited = json!([8437, 8463, 8444, 8451, 9111, 9105, 5008, 5001, 5011]);
    h.fake.state().user_edits_page(our_page, edited.clone());
    (our_page, edited)
}

/// The perks of rune page `id` in the fake client.
fn perks(h: &Harness, id: u64) -> Option<Value> {
    h.fake
        .state()
        .page(id)
        .map(|p| p["selectedPerkIds"].clone())
}

/// After lock-in one poll dies at the transport level (the busy client times
/// out / resets the connection) and the watcher reconnects into the same
/// champ select: no second import over the user's edits.
#[tokio::test]
async fn transport_error_after_lock_in_does_not_reimport() {
    let mut h = Harness::new("e2e-blip", Settings::default(), |_| {}).await;
    let (our_page, edited) = locked_in_and_edited(&mut h).await;
    h.fake.state().crash_after = Some(0);
    h.tick().await;
    assert!(!h.lcu_status().await.connected);
    h.fake.state().crash_after = None;
    // What `LcuClient::discover` does on a PC once the client answers again.
    *h.state().lcu.write().await = Some(h.fake.client());
    h.ticks(3).await;
    assert!(h.champ_select().await.in_champ_select);
    assert_eq!(h.auto_imports().len(), 1, "re-imported after a reconnect");
    assert_eq!(
        h.fake
            .state()
            .page(our_page)
            .map(|p| p["selectedPerkIds"].clone()),
        Some(edited)
    );
}

/// Same, but the busy client *answers* one poll with an error: a 503 from
/// the phase endpoint (read as phase "None") or a 404 for the champ select
/// session. Neither means "left champ select": no second import over the
/// user's edits (owner: "auto-import runs exactly once ... your own edits
/// after lock-in are safe").
#[tokio::test]
async fn error_answer_after_lock_in_does_not_reimport() {
    for (path, status) in [
        ("/lol-gameflow/v1/gameflow-phase", 503),
        ("/lol-champ-select/v1/session", 404),
    ] {
        let mut h = Harness::new("e2e-blip-answer", Settings::default(), |_| {}).await;
        let (our_page, edited) = locked_in_and_edited(&mut h).await;
        let events_before = h.events("champ-select").len();
        h.fake.state().fail_once.insert(path.into(), status);
        h.ticks(4).await;
        assert_eq!(
            h.auto_imports().len(),
            1,
            "{path} {status} once: re-imported"
        );
        assert_eq!(
            perks(&h, our_page),
            Some(edited),
            "{path} {status} once: the user's edits were overwritten"
        );
        // The UI isn't told "champ select over" for one bad answer: it would
        // forget the auto-import, and with it the "Import matchup build?"
        // hint when the lane opponent shows up later.
        let cleared: Vec<_> = h.events("champ-select")[events_before..]
            .iter()
            .filter(|cs| cs["in_champ_select"] == json!(false))
            .cloned()
            .collect();
        assert!(cleared.is_empty(), "{path} {status} once: UI cleared");
        assert!(h.champ_select().await.in_champ_select);
    }
}

/// The League client restarts mid champ select (it answers "None" while it
/// starts up), then rejoins the same champ select: no second import over the
/// user's edits. The next real champ select imports again.
#[tokio::test]
async fn league_restart_mid_champ_select_does_not_reimport() {
    let mut h = Harness::new("e2e-league-restart", Settings::default(), |_| {}).await;
    let draft = Draft::ranked().hover(YORICK).lock();
    let (our_page, edited) = locked_in_and_edited_in(&mut h, &draft).await;
    h.client_shows("None", None);
    h.ticks(LEAVE_POLLS + 3).await;
    // Shown as "not in champ select" once it lasts.
    assert!(!h.champ_select().await.in_champ_select);
    h.client_shows("ChampSelect", Some(&draft));
    h.ticks(3).await;
    assert!(h.champ_select().await.in_champ_select);
    assert_eq!(
        h.auto_imports().len(),
        1,
        "re-imported after a client restart"
    );
    assert_eq!(perks(&h, our_page), Some(edited));

    // A dodge: lobby, queue, a new champ select → imported once.
    h.client_shows("Lobby", None);
    h.ticks(LEAVE_POLLS).await;
    h.client_shows("Matchmaking", None);
    h.tick().await;
    h.client_shows("ChampSelect", Some(&Draft::ranked().hover(YORICK).lock()));
    h.ticks(3).await;
    assert_eq!(h.auto_imports().len(), 2);
}

// ---------------------------------------------------------------------------
// u.gg slow or hanging: the poll loop never waits for it
// ---------------------------------------------------------------------------

/// Requests to u.gg for builds (general, matchup and ARAM files).
const BUILD_FILES: &str = "/overview/";

/// Connected in the lobby, champion roles loaded.
async fn in_lobby(name: &str) -> Harness {
    let mut h = Harness::new(name, Settings::default(), |_| {}).await;
    h.client_shows("Lobby", None);
    h.tick().await;
    h
}

#[tokio::test]
async fn hanging_ugg_never_holds_up_champ_select_updates() {
    let mut h = in_lobby("e2e-hang").await;
    h.ugg().set_stalled(BUILD_FILES, true);

    // Lock in: the import starts in the background and waits for u.gg.
    let draft = Draft::ranked().enemy_locks(1, LEE_SIN).hover(YORICK).lock();
    h.client_shows("ChampSelect", Some(&draft));
    h.poll().await;
    assert!(h.champ_select().await.my_champion_locked);

    // u.gg doesn't answer; champ select keeps updating every poll.
    let draft = draft.enemy_locks(0, GWEN);
    h.client_shows("ChampSelect", Some(&draft));
    h.poll().await;
    let cs = h.events("champ-select").pop().unwrap();
    assert_eq!(cs["lane_opponent_id"], json!(GWEN), "UI not updated");
    for _ in 0..5 {
        h.poll().await;
    }
    h.client_shows("ChampSelect", Some(&draft.clone().finalization()));
    h.poll().await;
    assert!(h.writes().is_empty(), "{:?}", h.writes());
    assert!(h.auto_imports().is_empty());

    // u.gg answers: imported once, with what was known at lock-in.
    h.ugg().set_stalled(BUILD_FILES, false);
    h.settle().await;
    let imports = h.auto_imports();
    assert_eq!(imports.len(), 1);
    assert_eq!(imports[0].opponent_id, None);
    assert!(imports[0].result.runes && imports[0].result.item_set);
    assert_eq!(h.writes().len(), 3, "{:?}", h.writes());
    h.ticks(5).await;
    assert_eq!(h.auto_imports().len(), 1);
    assert_eq!(h.writes().len(), 3, "{:?}", h.writes());
    assert_never_touched_spells_or_defaults(&h.fake.state());
}

/// The build arrives after the user left champ select (a dodge): nothing is
/// written — whether the watcher already counted the champ select as left or
/// not. The next champ select imports once.
#[tokio::test]
async fn build_arriving_after_a_dodge_writes_nothing() {
    for lobby_polls in [1, LEAVE_POLLS + 1] {
        let mut h = in_lobby("e2e-late-build").await;
        h.ugg().set_stalled(BUILD_FILES, true);
        h.client_shows("ChampSelect", Some(&Draft::ranked().hover(YORICK).lock()));
        h.poll().await;
        // Someone dodges while u.gg is slow: shown right away.
        h.client_shows("Lobby", None);
        for _ in 0..lobby_polls {
            h.poll().await;
        }
        assert_eq!(h.lcu_status().await.phase, "Lobby");
        assert!(!h.champ_select().await.in_champ_select);
        let last_cs = h.events("champ-select").pop().unwrap();
        assert_eq!(last_cs["in_champ_select"], json!(false));
        h.ugg().set_stalled(BUILD_FILES, false);
        h.settle().await;
        assert!(h.writes().is_empty(), "{lobby_polls}: {:?}", h.writes());
        assert!(h.auto_imports().is_empty(), "{lobby_polls}");

        h.ticks(LEAVE_POLLS).await;
        h.client_shows("ChampSelect", Some(&Draft::ranked().hover(YORICK).lock()));
        h.ticks(3).await;
        assert_eq!(h.auto_imports().len(), 1, "{lobby_polls}");
        assert_eq!(h.writes().len(), 3, "{lobby_polls}: {:?}", h.writes());
    }
}

/// The next champ select's lock-in comes while the old build is still
/// loading: the old import never writes, the new one imports once.
#[tokio::test]
async fn old_build_never_lands_in_the_next_champ_select() {
    let mut h = in_lobby("e2e-old-build").await;
    h.ugg().set_stalled(BUILD_FILES, true);
    h.client_shows("ChampSelect", Some(&Draft::ranked().hover(YORICK).lock()));
    h.poll().await;
    h.client_shows("Lobby", None);
    for _ in 0..LEAVE_POLLS {
        h.poll().await;
    }
    // Requeue; this time Gwen is known at lock-in.
    let draft = Draft::ranked().enemy_locks(0, GWEN).hover(YORICK).lock();
    h.client_shows("ChampSelect", Some(&draft));
    h.poll().await;
    h.ugg().set_stalled(BUILD_FILES, false);
    h.settle().await;
    h.ticks(3).await;
    let imports = h.auto_imports();
    assert_eq!(imports.len(), 1, "{imports:?}");
    assert_eq!(imports[0].opponent_id, Some(GWEN));
    let st = h.fake.state();
    assert_eq!(st.count("POST", "/lol-perks/v1/pages"), 1);
    assert_eq!(st.count("PUT", "/lol-item-sets/"), 1);
    let ours: Vec<_> = st.our_pages().iter().map(|p| p["name"].clone()).collect();
    assert_eq!(ours, vec![json!("CSH: Yorick vs Gwen")]);
}

/// The same, without passing through the lobby long enough for the watcher
/// to notice (the new champ select is told apart by its game id).
#[tokio::test]
async fn old_build_never_lands_in_a_new_champ_select_seen_directly() {
    let mut h = in_lobby("e2e-old-build-direct").await;
    h.ugg().set_stalled(BUILD_FILES, true);
    h.client_shows("ChampSelect", Some(&Draft::ranked().hover(YORICK).lock()));
    h.poll().await;
    h.client_shows("ChampSelect", Some(&Draft::ranked().hover(YORICK).lock()));
    h.poll().await;
    h.ugg().set_stalled(BUILD_FILES, false);
    h.settle().await;
    h.ticks(3).await;
    assert_eq!(h.auto_imports().len(), 1);
    assert_eq!(h.fake.state().count("POST", "/lol-perks/v1/pages"), 1);
}

/// ARAM: a bench swap while the build is loading → only the new champion is
/// imported (the old one never writes).
#[tokio::test]
async fn aram_swap_while_the_build_loads_imports_only_the_new_champion() {
    let mut h = in_lobby("e2e-aram-swap").await;
    h.ugg().set_stalled(BUILD_FILES, true);
    let draft = Draft::aram(450, YORICK, vec![ASHE]);
    h.client_shows("ChampSelect", Some(&draft));
    h.poll().await;
    h.client_shows("ChampSelect", Some(&draft.swap_to(ASHE)));
    h.poll().await;
    h.ugg().set_stalled(BUILD_FILES, false);
    h.settle().await;
    assert!(h.writes().is_empty(), "{:?}", h.writes());
    h.ticks(3).await;
    let imports = h.auto_imports();
    assert_eq!(imports.len(), 1, "{imports:?}");
    assert_eq!(imports[0].champion_id, ASHE);
    assert_eq!(h.fake.state().count("POST", "/lol-perks/v1/pages"), 1);
}

/// Draft: a trade before the (slow) build arrived. Nothing is written for
/// the champion that was traded away, and a trade never starts an import.
#[tokio::test]
async fn trade_while_the_build_loads_imports_nothing() {
    let mut h = in_lobby("e2e-trade-slow").await;
    h.ugg().set_stalled(BUILD_FILES, true);
    let draft = Draft::ranked().hover(YORICK).lock();
    h.client_shows("ChampSelect", Some(&draft));
    h.poll().await;
    h.client_shows("ChampSelect", Some(&draft.traded_to(GWEN)));
    h.poll().await;
    h.ugg().set_stalled(BUILD_FILES, false);
    h.settle().await;
    h.ticks(5).await;
    assert!(h.writes().is_empty(), "{:?}", h.writes());
    assert!(h.auto_imports().is_empty());
}

/// u.gg has no build at lock-in: nothing written, the user is told, and it
/// is NEVER retried later (owner rule: one auto-import chance, at lock-in),
/// even once u.gg is back.
#[tokio::test]
async fn failed_build_fetch_is_never_retried() {
    let mut h = in_lobby("e2e-no-retry").await;
    let overview = h.dir.join("ugg").join(cache_file_name(&format!(
        "{UGG}/overview/16_19/ranked_solo_5x5/83/1.5.0.json"
    )));
    let saved = std::fs::read(&overview).unwrap();
    std::fs::remove_file(&overview).unwrap();
    h.client_shows("ChampSelect", Some(&Draft::ranked().hover(YORICK).lock()));
    h.ticks(3).await;
    assert!(h.writes().is_empty());
    let imports = h.auto_imports();
    assert_eq!(imports.len(), 1, "the user is told: {imports:?}");
    assert!(!imports[0].result.runes && !imports[0].result.item_set);
    assert!(imports[0].result.messages[0].contains("Press Import"));

    // u.gg is back: still nothing, however long we wait.
    std::fs::write(&overview, saved).unwrap();
    h.ugg().clear_memory();
    h.ticks(10).await;
    assert!(h.writes().is_empty(), "retried: {:?}", h.writes());
    assert_eq!(h.auto_imports().len(), 1);
}

/// u.gg's champion roles hang: champ select still updates (no enemy role
/// guesses meanwhile). Lock-in waits a moment for them so the import knows
/// the lane opponent; once they arrive the lane opponent is shown.
#[tokio::test]
async fn hanging_roles_never_hold_up_champ_select_updates() {
    let mut h = Harness::new("e2e-roles-hang", Settings::default(), |_| {}).await;
    h.ugg().set_stalled("/primary_roles/", true);
    let draft = Draft::ranked().enemy_locks(0, GWEN).hover(YORICK);
    h.client_shows("ChampSelect", Some(&draft));
    h.poll().await;
    h.poll().await;
    let cs = h.champ_select().await;
    assert!(cs.in_champ_select);
    assert_eq!(cs.my_champion_id, Some(YORICK));
    assert_eq!(cs.enemies[0].champion_id, GWEN);
    assert_eq!(cs.enemies[0].role, None, "no role data yet");
    assert_eq!(cs.lane_opponent_id, None);

    h.client_shows("ChampSelect", Some(&draft.lock()));
    h.poll().await;
    assert!(h.champ_select().await.my_champion_locked);
    assert!(h.writes().is_empty(), "imported without the lane opponent");

    h.ugg().set_stalled("/primary_roles/", false);
    h.settle().await;
    h.ticks(2).await;
    assert_eq!(h.champ_select().await.lane_opponent_id, Some(GWEN));
    let imports = h.auto_imports();
    assert_eq!(imports.len(), 1);
    assert_eq!(imports[0].opponent_id, Some(GWEN));
}

/// u.gg moves to a new patch while the app runs: the champion roles are
/// reloaded (checked at most hourly), so enemy lanes are guessed with them.
#[tokio::test]
async fn champion_roles_reload_after_a_patch_change() {
    let mut h = in_lobby("e2e-roles-patch").await;
    // Patch 16_20 is out, and Gwen is now mostly played mid.
    let ugg_dir = h.dir.join("ugg");
    let mut versions: Value = serde_json::from_slice(&fixture("ugg-api-versions.json")).unwrap();
    versions["16_20"] = versions["16_19"].clone();
    std::fs::write(
        ugg_dir.join(cache_file_name(UGG_VERSIONS)),
        versions.to_string(),
    )
    .unwrap();
    let mut roles: Value = serde_json::from_slice(&fixture("primary_roles.json")).unwrap();
    roles[GWEN.to_string()] = json!([5, 4]);
    std::fs::write(
        ugg_dir.join(cache_file_name(&format!(
            "{UGG}/primary_roles/16_20/1.5.0.json"
        ))),
        roles.to_string(),
    )
    .unwrap();
    h.ugg().clear_memory();

    let draft = Draft::ranked().enemy_locks(0, GWEN).hover(YORICK);
    h.client_shows("ChampSelect", Some(&draft));
    h.ticks(3).await;
    // Not re-checked within the hour: still 16_19's roles.
    assert_eq!(h.champ_select().await.lane_opponent_id, Some(GWEN));

    h.watcher.expire_roles_check();
    h.ticks(2).await;
    let cs = h.champ_select().await;
    assert_eq!(cs.enemies[0].role, Some(Role::Mid));
    assert_eq!(cs.lane_opponent_id, None);
}

// ---------------------------------------------------------------------------
// App restart mid champ select
// ---------------------------------------------------------------------------

/// The app is closed and started again during champ select, after the
/// auto-import: it remembers (marker file) and doesn't import over the
/// user's edits. A different champ select still imports once.
#[tokio::test]
async fn app_restart_mid_champ_select_does_not_reimport() {
    let mut h = Harness::new("e2e-app-restart", Settings::default(), |_| {}).await;
    let draft = Draft::ranked().hover(YORICK).lock();
    let (our_page, edited) = locked_in_and_edited_in(&mut h, &draft).await;
    assert!(h.marker().exists());

    h.restart_app();
    h.ticks(4).await;
    assert!(h.champ_select().await.my_champion_locked);
    assert_eq!(
        h.auto_imports().len(),
        1,
        "re-imported after an app restart"
    );
    assert_eq!(perks(&h, our_page), Some(edited));

    // Restarted again, now in a different champ select: imports once.
    h.restart_app();
    h.client_shows("ChampSelect", Some(&Draft::ranked().hover(YORICK).lock()));
    h.ticks(4).await;
    assert_eq!(h.auto_imports().len(), 2);

    // Out of champ select the marker is gone.
    h.client_shows("InProgress", None);
    h.ticks(LEAVE_POLLS).await;
    assert!(!h.marker().exists());
    h.restart_app();
    h.client_shows("ChampSelect", Some(&Draft::ranked().hover(YORICK).lock()));
    h.ticks(3).await;
    assert_eq!(h.auto_imports().len(), 3);
}

/// Auto-import was off at lock-in, then switched on and the app restarted:
/// still nothing imported in that champ select.
#[tokio::test]
async fn app_restart_after_lock_in_with_auto_import_off_does_not_import() {
    let off = Settings {
        auto_import: false,
        ..Settings::default()
    };
    let mut h = Harness::new("e2e-app-restart-off", off, |_| {}).await;
    h.client_shows("ChampSelect", Some(&Draft::ranked().hover(YORICK).lock()));
    h.ticks(3).await;
    h.state().settings.write().await.auto_import = true;
    h.restart_app();
    h.ticks(3).await;
    assert!(h.writes().is_empty(), "{:?}", h.writes());
    assert!(h.auto_imports().is_empty());
}

// ---------------------------------------------------------------------------
// The UI's invoke() calls, through Tauri's IPC layer
// ---------------------------------------------------------------------------

/// Calls every command exactly like `src/lib/api.ts` does (camelCase argument
/// names, JSON bodies) through Tauri's real IPC dispatch, so a renamed
/// argument (e.g. `overwritePageId` ↔ `overwrite_page_id`) fails here — the
/// import/overwrite path can't be exercised in the real app without League.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ui_invoke_calls_reach_the_commands() {
    let h = Harness::new("e2e-ipc", Settings::default(), |st| {
        st.fill_page_slots_without_our_page();
    })
    .await;
    let webview = tauri::WebviewWindowBuilder::new(&h.app, "main", Default::default())
        .build()
        .unwrap();
    let invoke = |cmd: &str, args: Value| -> Result<Value, Value> {
        let request = tauri::webview::InvokeRequest {
            cmd: cmd.into(),
            callback: tauri::ipc::CallbackFn(0),
            error: tauri::ipc::CallbackFn(1),
            // The app's own origin (the custom protocol), as for the real UI.
            url: if cfg!(windows) {
                "http://tauri.localhost"
            } else {
                "tauri://localhost"
            }
            .parse()
            .unwrap(),
            body: tauri::ipc::InvokeBody::Json(args),
            headers: Default::default(),
            invoke_key: tauri::test::INVOKE_KEY.to_string(),
        };
        tokio::task::block_in_place(|| {
            tauri::test::get_ipc_response(&webview, request)
                .map(|body| body.deserialize::<Value>().unwrap())
        })
    };

    let sd = invoke("get_static_data", json!({})).unwrap();
    assert_eq!(sd["ugg_patch"], json!("16_19"));
    assert!(!sd["champions"].as_array().unwrap().is_empty());

    // getBuild(championId, role, opponentId, queue)
    let build = invoke(
        "get_build",
        json!({"championId": YORICK, "role": "top", "opponentId": GWEN, "queue": "ranked_solo"}),
    )
    .unwrap();
    assert_eq!(build["opponent_id"], json!(GWEN));
    assert_eq!(build["fell_back_to_general"], json!(false));
    let general = invoke(
        "get_build",
        json!({"championId": YORICK, "role": null, "opponentId": null, "queue": "aram_mayhem"}),
    )
    .unwrap();
    assert_eq!(general["queue"], json!("aram_mayhem"));

    // getCounters(enemyId, role, queue) / getMatchups / getTierList
    let counters = invoke(
        "get_counters",
        json!({"enemyId": YORICK, "role": "top", "queue": "ranked_solo"}),
    )
    .unwrap();
    assert!(!counters.as_array().unwrap().is_empty());
    let matchups = invoke(
        "get_matchups",
        json!({"championId": YORICK, "role": "top", "queue": "ranked_solo"}),
    )
    .unwrap();
    assert!(!matchups.as_array().unwrap().is_empty());
    let tiers = invoke(
        "get_tier_list",
        json!({"role": "top", "queue": "ranked_solo"}),
    )
    .unwrap();
    assert!(!tiers.as_array().unwrap().is_empty());

    // importBuild(build, overwritePageId): pages full → asks first…
    let asked = invoke(
        "import_build",
        json!({"build": build, "overwritePageId": null}),
    )
    .unwrap();
    assert_eq!(asked["runes"], json!(false));
    assert_eq!(asked["needs_confirmation"]["id"], json!(PAGE_MY_CONQUEROR));
    assert_eq!(h.fake.state().count("DELETE", "/lol-perks"), 0);
    // …then replaces exactly the confirmed page.
    let replaced = invoke(
        "import_build",
        json!({"build": build, "overwritePageId": PAGE_MY_CONQUEROR}),
    )
    .unwrap();
    assert_eq!(replaced["runes"], json!(true), "{replaced}");
    let deletes: Vec<String> = h
        .fake
        .state()
        .writes()
        .into_iter()
        .filter(|r| r.method == "DELETE")
        .map(|r| r.path)
        .collect();
    assert_eq!(
        deletes,
        vec![format!("/lol-perks/v1/pages/{PAGE_MY_CONQUEROR}")]
    );

    // saveSettings(newSettings) / getSettings
    let mut settings = invoke("get_settings", json!({})).unwrap();
    settings["min_games"] = json!(250);
    settings["champion_pool"] = json!([YORICK]);
    invoke("save_settings", json!({"newSettings": settings})).unwrap();
    assert_eq!(invoke("get_settings", json!({})).unwrap(), settings);
    let on_disk: Value =
        serde_json::from_str(&std::fs::read_to_string(h.dir.join("settings.json")).unwrap())
            .unwrap();
    assert_eq!(on_disk, settings);

    // getChampSelect / getLcuStatus
    let cs = invoke("get_champ_select", json!({})).unwrap();
    assert_eq!(cs["in_champ_select"], json!(false));
    let status = invoke("get_lcu_status", json!({})).unwrap();
    assert_eq!(status["connected"], json!(false));

    // Bad arguments are an error, not a panic.
    assert!(invoke(
        "get_build",
        json!({"championId": "x", "queue": "ranked_solo"})
    )
    .is_err());
}

// ---------------------------------------------------------------------------
// Request rate
// ---------------------------------------------------------------------------

/// Far from champ select (home screen, in game) the client is only asked for
/// its phase, every few seconds; in and right before champ select every
/// second. Right after champ select it stays fast for a few polls, so a dodge
/// is still noticed (and the next champ select imports again).
#[tokio::test]
async fn polls_slowly_away_from_champ_select() {
    let fast = Duration::from_secs(1);
    let mut h = Harness::new("e2e-poll-rate", Settings::default(), |_| {}).await;

    h.client_shows("None", None);
    h.ticks(3).await;
    assert!(h.tick().await > fast, "home screen polled every second");
    for phase in ["Lobby", "Matchmaking", "ReadyCheck"] {
        h.client_shows(phase, None);
        assert_eq!(h.tick().await, fast, "{phase}");
    }
    h.client_shows("ChampSelect", Some(&Draft::ranked().hover(YORICK).lock()));
    assert_eq!(h.tick().await, fast);
    assert_eq!(h.auto_imports().len(), 1);

    // In game: fast for the first polls, then only the phase, slowly.
    h.client_shows("InProgress", None);
    assert_eq!(h.tick().await, fast);
    h.ticks(2).await;
    let before = h.fake.state().log.len();
    assert!(h.tick().await > fast, "in game polled every second");
    let paths: Vec<String> = h.fake.state().log[before..]
        .iter()
        .map(|r| format!("{} {}", r.method, r.path))
        .collect();
    assert_eq!(paths, ["GET /lol-gameflow/v1/gameflow-phase"]);

    // A dodge right after lock-in: back in the queue (fast polls), then the
    // new champ select imports again.
    h.client_shows("ChampSelect", Some(&Draft::ranked().hover(YORICK).lock()));
    h.tick().await;
    assert_eq!(h.auto_imports().len(), 2);
    h.client_shows("Matchmaking", None);
    for _ in 0..3 {
        assert_eq!(h.tick().await, fast);
    }
    h.client_shows("ChampSelect", Some(&Draft::ranked().hover(YORICK).lock()));
    h.tick().await;
    assert_eq!(h.auto_imports().len(), 3);
}
