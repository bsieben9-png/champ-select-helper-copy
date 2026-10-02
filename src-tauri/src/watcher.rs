//! Background loop: connects to the League client, polls champ select,
//! emits `lcu-status` / `champ-select` / `auto-imported` events, and runs
//! auto-import. OWNER: client agent.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager};

use crate::lcu::LcuClient;
use crate::model::*;
use crate::AppState;

/// How often to look for the client while it isn't running.
const DISCOVER_INTERVAL: Duration = Duration::from_secs(3);
/// How often to poll status / champ select while connected.
const POLL_INTERVAL: Duration = Duration::from_secs(1);
/// Retry a failed auto-import (e.g. u.gg unreachable) after this long.
const IMPORT_RETRY: Duration = Duration::from_secs(15);
/// Retry loading u.gg primary roles after this long when it failed.
const ROLES_RETRY: Duration = Duration::from_secs(30);

pub fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut watcher = Watcher::default();
        loop {
            let delay = watcher.tick(&app).await;
            tokio::time::sleep(delay).await;
        }
    });
}

#[derive(Default)]
struct Watcher {
    /// u.gg primary roles (empty until loaded).
    roles: HashMap<u32, Vec<Role>>,
    roles_loaded: bool,
    roles_last_try: Option<Instant>,
    /// Auto-import done/attempted in the current champ select (reset when
    /// leaving champ select).
    last_import: Option<LastImport>,
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
    async fn tick(&mut self, app: &AppHandle) -> Duration {
        let state = app.state::<AppState>();

        // Clone the client so no lock is held across HTTP calls.
        let existing = state.lcu.read().await.clone();
        let client = match existing {
            Some(client) => client,
            None => {
                let found = tauri::async_runtime::spawn_blocking(LcuClient::discover)
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
        set_status(app, &state, status).await;

        if !in_champ_select {
            self.left_champ_select(app, &state).await;
            return POLL_INTERVAL;
        }

        self.load_roles(&state).await;
        match client.champ_select(&self.roles).await {
            Ok(cs) => {
                set_champ_select(app, &state, cs.clone()).await;
                if cs.in_champ_select {
                    self.auto_import(app, &state, &client, &cs).await;
                } else {
                    self.last_import = None;
                }
            }
            Err(e) => self.log(format!("Reading champ select failed: {e:#}")),
        }
        POLL_INTERVAL
    }

    async fn disconnected(&mut self, app: &AppHandle, state: &AppState) {
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
        self.left_champ_select(app, state).await;
    }

    /// Clear champ select (emits once) and forget what was imported, so the
    /// next champ select (e.g. after a dodge) imports again.
    async fn left_champ_select(&mut self, app: &AppHandle, state: &AppState) {
        set_champ_select(app, state, ChampSelectState::default()).await;
        self.last_import = None;
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
    async fn auto_import(
        &mut self,
        app: &AppHandle,
        state: &AppState,
        client: &LcuClient,
        cs: &ChampSelectState,
    ) {
        let settings = state.settings.read().await.clone();
        if !settings.auto_import || !cs.my_champion_locked {
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
async fn set_status(app: &AppHandle, state: &AppState, status: LcuStatus) {
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
async fn set_champ_select(app: &AppHandle, state: &AppState, cs: ChampSelectState) {
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
}
