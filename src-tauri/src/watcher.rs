//! Background loop: connects to the League client, polls champ select,
//! emits `lcu-status` / `champ-select` / `auto-imported` events, and runs
//! auto-import. OWNER: client agent.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::lcu::LcuClient;
use crate::model::*;
use crate::AppState;

/// How often to look for the client while it isn't running.
const DISCOVER_INTERVAL: Duration = Duration::from_secs(3);
/// How often to poll status / champ select while connected, in or close to
/// champ select (lobby, queue, ready check).
const POLL_INTERVAL: Duration = Duration::from_secs(1);
/// How often to poll in phases far from champ select (home screen, in game,
/// end of game): one cheap gameflow-phase request, nothing else.
const IDLE_POLL_INTERVAL: Duration = Duration::from_secs(5);
/// Forget the once-per-champ-select import only after this many polls in a
/// row outside champ select. A single error reply (phase 503 → "None", or a
/// session 404) must not count as "left champ select", or the next poll would
/// re-import over the user's edits. A real dodge/requeue takes far longer.
const LEAVE_POLLS: u32 = 3;
/// Retry a failed auto-import (e.g. u.gg unreachable) after this long.
const IMPORT_RETRY: Duration = Duration::from_secs(15);
/// Retry loading u.gg primary roles after this long when it failed.
const ROLES_RETRY: Duration = Duration::from_secs(30);

pub fn spawn<R: Runtime>(app: AppHandle<R>) {
    tauri::async_runtime::spawn(async move {
        let mut watcher = Watcher::default();
        loop {
            let delay = watcher.tick(&app).await;
            tokio::time::sleep(delay).await;
        }
    });
}

#[derive(Default)]
pub(crate) struct Watcher {
    /// u.gg primary roles (empty until loaded).
    roles: HashMap<u32, Vec<Role>>,
    roles_loaded: bool,
    roles_last_try: Option<Instant>,
    /// Auto-import done/attempted in the current champ select (reset when
    /// leaving champ select).
    last_import: Option<LastImport>,
    /// Consecutive polls that looked like "not in champ select".
    polls_outside: u32,
    last_error: Option<String>,
}

struct LastImport {
    champion_id: u32,
    at: Instant,
    /// True once the build was pushed to the client (whatever the outcome).
    /// False when fetching the build failed → retry after `IMPORT_RETRY`.
    done: bool,
}

impl Watcher {
    /// One iteration; returns how long to sleep before the next one.
    pub(crate) async fn tick<R: Runtime>(&mut self, app: &AppHandle<R>) -> Duration {
        let state = app.state::<AppState>();

        // Clone the client so no lock is held across HTTP calls.
        let existing = state.lcu.read().await.clone();
        let client = match existing {
            Some(client) => client,
            None => {
                let found = tauri::async_runtime::spawn_blocking(discover)
                    .await
                    .ok()
                    .flatten();
                match found {
                    Some(client) => {
                        *state.lcu.write().await = Some(client.clone());
                        client
                    }
                    None => {
                        self.disconnected(app, &state).await;
                        return DISCOVER_INTERVAL;
                    }
                }
            }
        };

        let status = match client.status().await {
            Ok(status) => status,
            Err(e) => {
                self.log(format!("League client disconnected: {e:#}"));
                *state.lcu.write().await = None;
                self.disconnected(app, &state).await;
                return DISCOVER_INTERVAL;
            }
        };
        let in_champ_select = status.phase == "ChampSelect";
        let near_champ_select = near_champ_select(&status.phase);
        set_status(app, &state, status).await;

        if !in_champ_select {
            self.left_champ_select(app, &state).await;
            // Stay fast right after champ select too, so a blip or a dodge
            // is counted (LEAVE_POLLS) within seconds.
            return if near_champ_select || self.polls_outside < LEAVE_POLLS {
                POLL_INTERVAL
            } else {
                IDLE_POLL_INTERVAL
            };
        }

        self.load_roles(&state).await;
        match client.champ_select(&self.roles).await {
            Ok(cs) => {
                set_champ_select(app, &state, cs.clone()).await;
                if cs.in_champ_select {
                    self.polls_outside = 0;
                    self.auto_import(app, &state, &client, &cs).await;
                } else {
                    // Session 404 while the phase still says ChampSelect:
                    // treat as transient, never as a new champ select.
                    self.polls_outside = self.polls_outside.saturating_add(1);
                    if self.polls_outside >= LEAVE_POLLS {
                        self.last_import = None;
                    }
                }
            }
            Err(e) => self.log(format!("Reading champ select failed: {e:#}")),
        }
        POLL_INTERVAL
    }

    /// The client is gone or stopped answering. Clears the UI but keeps
    /// `last_import`: a hiccup (timeout, stale port) in the middle of champ
    /// select must not cause a second auto-import when we reconnect into the
    /// same champ select. It's forgotten once the client is seen outside
    /// champ select (`left_champ_select`).
    async fn disconnected<R: Runtime>(&mut self, app: &AppHandle<R>, state: &AppState) {
        set_status(
            app,
            state,
            LcuStatus {
                connected: false,
                summoner_name: None,
                phase: "None".into(),
            },
        )
        .await;
        set_champ_select(app, state, ChampSelectState::default()).await;
    }

    /// Clear champ select (emits once) and forget what was imported, so the
    /// next champ select (e.g. after a dodge) imports again.
    async fn left_champ_select<R: Runtime>(&mut self, app: &AppHandle<R>, state: &AppState) {
        set_champ_select(app, state, ChampSelectState::default()).await;
        self.polls_outside = self.polls_outside.saturating_add(1);
        if self.polls_outside >= LEAVE_POLLS {
            self.last_import = None;
        }
    }

    /// Fetch u.gg primary roles once; retry later if it failed.
    async fn load_roles(&mut self, state: &AppState) {
        if self.roles_loaded
            || self
                .roles_last_try
                .is_some_and(|t| t.elapsed() < ROLES_RETRY)
        {
            return;
        }
        self.roles_last_try = Some(Instant::now());
        match state.ugg.primary_roles().await {
            Ok(roles) => {
                self.roles = roles;
                self.roles_loaded = true;
            }
            Err(e) => self.log(format!("Loading champion roles failed: {e:#}")),
        }
    }

    /// Import runes + item set exactly once per champ select, when the local
    /// player locks in, using the lane opponent known at that moment. Later
    /// changes (enemy locks, trades, the user editing pages) never trigger a
    /// re-import. ARAM has no lock-in: there, a champion change from a
    /// reroll / bench swap counts as a new pick and is imported once.
    async fn auto_import<R: Runtime>(
        &mut self,
        app: &AppHandle<R>,
        state: &AppState,
        client: &LcuClient,
        cs: &ChampSelectState,
    ) {
        if !cs.my_champion_locked {
            return;
        }
        let (Some(champion_id), Some(queue)) = (cs.my_champion_id, cs.queue) else {
            return;
        };
        if !import_due(
            self.last_import.as_ref(),
            champion_id,
            queue,
            Instant::now(),
        ) {
            return;
        }
        let settings = state.settings.read().await.clone();
        // Nothing to import when runes and item set are both switched off (an
        // empty result would show an "import failed" toast every game).
        let anything = settings.import_runes || settings.import_item_set;
        if !settings.auto_import || !anything {
            // Off at the moment of lock-in: this pick is done. Switching
            // auto-import on later in this champ select must not import (the
            // user may already have set up their runes by hand).
            self.last_import = Some(LastImport {
                champion_id,
                at: Instant::now(),
                done: true,
            });
            return;
        }
        // Remember before the (slow) work so a failure isn't retried every second.
        self.last_import = Some(LastImport {
            champion_id,
            at: Instant::now(),
            done: false,
        });

        let build = match state
            .ugg
            .build(
                champion_id,
                cs.my_role,
                cs.lane_opponent_id,
                queue,
                &settings,
            )
            .await
        {
            Ok(build) => build,
            Err(e) => {
                self.log(format!(
                    "Auto-import: no build for champion {champion_id}: {e:#}"
                ));
                return;
            }
        };
        let static_data = state.static_data().await.ok();
        let result = client
            .import_build(&build, &settings, static_data.as_ref(), None)
            .await;
        self.last_import = Some(LastImport {
            champion_id,
            at: Instant::now(),
            done: true,
        });

        let event = AutoImportEvent {
            champion_id,
            opponent_id: cs.lane_opponent_id,
            result,
        };
        if let Err(e) = app.emit("auto-imported", &event) {
            self.log(format!("emit auto-imported failed: {e}"));
        }
    }

    /// Log errors to stderr, without repeating the same line every second.
    fn log(&mut self, message: String) {
        if self.last_error.as_deref() != Some(message.as_str()) {
            eprintln!("[watcher] {message}");
            self.last_error = Some(message);
        }
    }
}

/// Find a running League client. Unit tests drive the watcher against a
/// fake client and must never connect to (and import into) a real one.
fn discover() -> Option<LcuClient> {
    if cfg!(test) {
        return None;
    }
    LcuClient::discover()
}

/// Gameflow phases from which champ select can start within seconds. Every
/// other phase ("None" = home screen, "InProgress", "EndOfGame", …) is polled
/// at `IDLE_POLL_INTERVAL`.
fn near_champ_select(phase: &str) -> bool {
    matches!(
        phase,
        "Lobby" | "Matchmaking" | "ReadyCheck" | "CheckedIntoTournament" | "ChampSelect"
    )
}

/// Should the locked-in champion be auto-imported now, given what was
/// already imported in this champ select?
fn import_due(last: Option<&LastImport>, champion_id: u32, queue: Queue, now: Instant) -> bool {
    let Some(last) = last else { return true };
    // Draft/blind: once per champ select. ARAM: once per champion.
    let same_pick = !queue.is_aram() || last.champion_id == champion_id;
    if !same_pick {
        return true;
    }
    // Retry only when fetching the build failed (nothing was imported).
    !last.done && now.saturating_duration_since(last.at) >= IMPORT_RETRY
}

/// Store + emit `lcu-status` when it changed.
async fn set_status<R: Runtime>(app: &AppHandle<R>, state: &AppState, status: LcuStatus) {
    {
        let mut current = state.lcu_status.write().await;
        if *current == status {
            return;
        }
        *current = status.clone();
    }
    let _ = app.emit("lcu-status", &status);
}

/// Store + emit `champ-select` when it changed.
async fn set_champ_select<R: Runtime>(app: &AppHandle<R>, state: &AppState, cs: ChampSelectState) {
    {
        let mut current = state.champ_select.write().await;
        if *current == cs {
            return;
        }
        *current = cs.clone();
    }
    let _ = app.emit("champ-select", &cs);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn last(champion_id: u32, done: bool, ago: Duration) -> LastImport {
        LastImport {
            champion_id,
            at: Instant::now() - ago,
            done,
        }
    }

    #[test]
    fn first_lock_imports() {
        assert!(import_due(None, 83, Queue::RankedSolo, Instant::now()));
        assert!(import_due(None, 83, Queue::Aram, Instant::now()));
    }

    #[test]
    fn draft_imports_once_per_champ_select() {
        let now = Instant::now();
        let done = last(83, true, Duration::from_secs(60));
        assert!(!import_due(Some(&done), 83, Queue::RankedSolo, now));
        // Even if the champion changes later (trade), no re-import.
        assert!(!import_due(Some(&done), 86, Queue::RankedSolo, now));
    }

    #[test]
    fn aram_imports_once_per_champion() {
        let now = Instant::now();
        let done = last(83, true, Duration::from_secs(1));
        assert!(!import_due(Some(&done), 83, Queue::Aram, now));
        assert!(import_due(Some(&done), 22, Queue::Aram, now));
    }

    #[test]
    fn failed_build_fetch_retries_after_delay() {
        let now = Instant::now();
        let recent = last(83, false, Duration::from_secs(2));
        assert!(!import_due(Some(&recent), 83, Queue::RankedSolo, now));
        let old = last(83, false, IMPORT_RETRY + Duration::from_secs(1));
        assert!(import_due(Some(&old), 83, Queue::RankedSolo, now));
    }

    // --- the loop itself, against a mock League client ----------------------

    use crate::lcu::tests::{fixture, mock_client, unreachable_client};
    use tauri::test::MockRuntime;
    use tokio::sync::RwLock;

    /// Offline app state: u.gg from fixtures, static data preloaded (so Data
    /// Dragon is never contacted), connected to `client`.
    fn manage_state(
        app: &tauri::App<MockRuntime>,
        name: &str,
        client: LcuClient,
        settings: Settings,
    ) {
        let dir = crate::http_cache::test_util::temp_dir(name);
        app.manage(AppState {
            ugg: crate::ugg::tests::seeded_ugg(name).0,
            ddragon: crate::ddragon::DDragon::new(dir.join("ddragon")),
            settings: RwLock::new(settings),
            settings_path: dir.join("settings.json"),
            lcu: RwLock::new(Some(client)),
            lcu_status: RwLock::new(LcuStatus::default()),
            champ_select: RwLock::new(ChampSelectState::default()),
            static_data: RwLock::new(Some(StaticData {
                version: "16.19.1".into(),
                ugg_patch: "16_19".into(),
                champions: Vec::new(),
                items: Vec::new(),
                rune_styles: Vec::new(),
                shards: Vec::new(),
                spells: Vec::new(),
            })),
        });
    }

    const RUNE_POST: (&str, &str) = ("POST", "/lol-perks/v1/pages");

    #[test]
    fn reconnecting_mid_champ_select_does_not_import_again() {
        let app = tauri::test::mock_app();
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            // Locked in as Yorick (ranked).
            let (mock, client) =
                mock_client(|m| m.session = Some(fixture("champ_select_ranked.json"))).await;
            manage_state(
                &app,
                "watcher-reconnect",
                client.clone(),
                Settings::default(),
            );
            let (handle, state) = (app.handle(), app.state::<AppState>());
            let posts = || mock.lock().unwrap().requests_to(RUNE_POST.0, RUNE_POST.1);
            let mut w = Watcher::default();

            w.tick(handle).await;
            assert_eq!(posts(), 1, "auto-import at lock-in");

            // The client stops answering for a moment (timeout / restart)…
            *state.lcu.write().await = Some(unreachable_client().await);
            w.tick(handle).await;
            assert!(state.lcu.read().await.is_none());
            assert!(!state.lcu_status.read().await.connected);
            // …and is back, still in the same champ select: the user may have
            // edited the imported page by now, so nothing is imported again.
            *state.lcu.write().await = Some(client.clone());
            w.tick(handle).await;
            w.tick(handle).await;
            assert!(state.champ_select.read().await.in_champ_select);
            assert_eq!(posts(), 1, "re-imported after a reconnect");

            // The next champ select imports again.
            mock.lock().unwrap().session = None;
            for _ in 0..LEAVE_POLLS {
                w.tick(handle).await;
            }
            mock.lock().unwrap().session = Some(fixture("champ_select_ranked.json"));
            w.tick(handle).await;
            assert_eq!(posts(), 2);
        });
    }

    #[test]
    fn switching_auto_import_on_after_lock_in_does_not_import() {
        let app = tauri::test::mock_app();
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let (mock, client) =
                mock_client(|m| m.session = Some(fixture("champ_select_ranked.json"))).await;
            let off = Settings {
                auto_import: false,
                ..Settings::default()
            };
            manage_state(&app, "watcher-toggle", client, off);
            let (handle, state) = (app.handle(), app.state::<AppState>());
            let posts = || mock.lock().unwrap().requests_to(RUNE_POST.0, RUNE_POST.1);
            let mut w = Watcher::default();

            w.tick(handle).await;
            assert_eq!(posts(), 0, "auto-import is off");
            // Turned on after locking in: it was off at lock-in, so no import.
            state.settings.write().await.auto_import = true;
            w.tick(handle).await;
            assert_eq!(posts(), 0, "imported after lock-in");

            // The next champ select imports at lock-in.
            mock.lock().unwrap().session = None;
            for _ in 0..LEAVE_POLLS {
                w.tick(handle).await;
            }
            mock.lock().unwrap().session = Some(fixture("champ_select_ranked.json"));
            w.tick(handle).await;
            assert_eq!(posts(), 1);
        });
    }
}
