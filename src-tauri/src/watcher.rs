//! Background loop: connects to the League client, polls champ select,
//! emits `lcu-status` / `champ-select` / `auto-imported` events, and runs
//! auto-import. OWNER: client agent.
//!
//! The 1-second poll only talks to the League client (local and fast).
//! Everything that needs u.gg — the champion roles used to guess enemy lanes,
//! and the build for the auto-import — runs in background tasks whose
//! results the next poll picks up, so a slow or hanging u.gg never holds
//! back champ select updates in the UI.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tokio::task::{JoinError, JoinHandle};

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
/// row in another gameflow phase (Lobby, Matchmaking, GameStart, …). Error
/// answers never count as leaving: a phase 503 (read as "None"), "None"
/// while the client restarts, or a session 404 while the phase still says
/// ChampSelect — else the next poll would re-import over the user's edits.
/// A real dodge/requeue takes far longer.
const LEAVE_POLLS: u32 = 3;
/// Retry loading u.gg primary roles after this long when it failed.
const ROLES_RETRY: Duration = Duration::from_secs(30);
/// How often to check whether u.gg moved to a new patch (→ reload roles).
const ROLES_REFRESH: Duration = Duration::from_secs(60 * 60);
/// Right after startup, hold the auto-import up to this long for the first
/// load of champion roles (the lane opponent is guessed from them).
const ROLES_WAIT: Duration = Duration::from_secs(5);
/// A saved "already imported" marker older than this is ignored (no champ
/// select lasts that long).
const MARKER_MAX_AGE: Duration = Duration::from_secs(2 * 60 * 60);

/// Run the loop. `marker_path`: where to remember the last auto-import, so
/// restarting the app in the middle of a champ select doesn't import again.
pub fn spawn<R: Runtime>(app: AppHandle<R>, marker_path: PathBuf) {
    tauri::async_runtime::spawn(async move {
        let mut watcher = Watcher::new(marker_path);
        loop {
            let delay = watcher.tick(&app).await;
            tokio::time::sleep(delay).await;
        }
    });
}

#[derive(Default)]
pub(crate) struct Watcher {
    roles: Roles,
    /// Auto-import done/attempted in the current champ select (kept while
    /// the client hiccups; reset when leaving champ select).
    last_import: Option<LastImport>,
    /// The auto-import running in the background (at most one at a time).
    import_task: Option<ImportTask>,
    /// Bumped when a champ select is over (left, or a different one seen):
    /// an import still running for it then writes nothing.
    generation: Arc<AtomicU64>,
    /// Consecutive polls not in champ select, for any reason…
    polls_outside: u32,
    /// …and of those, polls in another gameflow phase (a real leave).
    polls_elsewhere: u32,
    /// The "already imported" marker file (None: not persisted).
    marker_path: Option<PathBuf>,
    last_error: Option<String>,
    /// Tests: imports left running for a champ select that is over.
    #[cfg(test)]
    detached: Vec<JoinHandle<Outcome>>,
}

/// u.gg primary roles, loaded and refreshed in the background.
#[derive(Default)]
struct Roles {
    /// Empty until loaded: enemy roles aren't guessed meanwhile.
    map: HashMap<u32, Vec<Role>>,
    /// The u.gg patch `map` is from (None until loaded).
    patch: Option<String>,
    /// When the last load/check started, and whether it worked.
    checked: Option<Instant>,
    ok: bool,
    task: Option<(Instant, JoinHandle<RolesResult>)>,
}

/// `None`: still the same patch, nothing reloaded.
type RolesResult = anyhow::Result<Option<(String, HashMap<u32, Vec<Role>>)>>;

struct LastImport {
    champion_id: u32,
    /// Which champ select (`lcu::session_identity`), if the client says.
    identity: Option<String>,
    // Once set (attempt started, imported, auto-import off at lock-in, or the
    // one attempt failed) this pick is dealt with. Owner rule: ONE auto-import
    // chance, at lock-in; never retried later; the user can press Import.
}

struct ImportTask {
    champion_id: u32,
    identity: Option<String>,
    /// `Watcher::generation` when it started.
    generation: u64,
    handle: JoinHandle<Outcome>,
}

/// How a background auto-import ended.
#[derive(Debug)]
enum Outcome {
    /// Pushed to the client (whatever the result); `auto-imported` emitted.
    Imported,
    /// Nothing was written: no build from u.gg, the champ select ended, or
    /// the client didn't answer the last check.
    NothingWritten(String),
    /// My champion changed (a trade) before anything was written.
    ChampionChanged,
}

impl Watcher {
    /// A watcher that remembers its auto-import in `marker_path` and starts
    /// from a recent marker there (the app was restarted mid champ select).
    pub(crate) fn new(marker_path: PathBuf) -> Watcher {
        let last_import = Marker::load(&marker_path).map(|m| LastImport {
            champion_id: m.champion_id,
            identity: Some(m.champ_select),
        });
        Watcher {
            last_import,
            marker_path: Some(marker_path),
            ..Watcher::default()
        }
    }

    /// One iteration; returns how long to sleep before the next one. Never
    /// waits for u.gg.
    pub(crate) async fn tick<R: Runtime>(&mut self, app: &AppHandle<R>) -> Duration {
        self.collect_finished().await;
        self.refresh_roles(app);
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
        // "None" is also what an error answer or a starting client reports.
        let elsewhere = !in_champ_select && status.phase != "None";
        let near_champ_select = near_champ_select(&status.phase);
        set_status(app, &state, status).await;

        if !in_champ_select {
            self.not_in_champ_select(app, &state, elsewhere).await;
            // Stay fast right after champ select too, so a blip or a dodge
            // is counted (LEAVE_POLLS) within seconds.
            return if near_champ_select || self.polls_outside < LEAVE_POLLS {
                POLL_INTERVAL
            } else {
                IDLE_POLL_INTERVAL
            };
        }

        match client.champ_select_with_identity(&self.roles.map).await {
            Ok((cs, identity)) if cs.in_champ_select => {
                self.polls_outside = 0;
                self.polls_elsewhere = 0;
                self.check_identity(identity.as_deref());
                set_champ_select(app, &state, cs.clone()).await;
                self.auto_import(app, &state, &client, &cs, identity).await;
            }
            // Session 404 while the phase still says ChampSelect: transient.
            Ok(_) => self.not_in_champ_select(app, &state, false).await,
            Err(e) => self.log(format!("Reading champ select failed: {e:#}")),
        }
        POLL_INTERVAL
    }

    /// Tests: wait for the background work (roles, auto-import) to finish
    /// and take in its results, as the next poll would.
    #[cfg(test)]
    pub(crate) async fn settle(&mut self) {
        if let Some((_, handle)) = self.roles.task.take() {
            let result = handle.await;
            self.roles_loaded(result);
        }
        if let Some(mut task) = self.import_task.take() {
            let outcome = (&mut task.handle).await;
            self.import_finished(&task, outcome);
        }
        for handle in std::mem::take(&mut self.detached) {
            let _ = handle.await;
        }
    }

    /// Tests: pretend the hourly roles check is due.
    #[cfg(test)]
    pub(crate) fn expire_roles_check(&mut self) {
        self.roles.checked = None;
    }

    /// Take in the results of background tasks that have finished.
    async fn collect_finished(&mut self) {
        if let Some((_, handle)) = self.roles.task.take_if(|(_, h)| h.is_finished()) {
            let result = handle.await;
            self.roles_loaded(result);
        }
        if let Some(mut task) = self.import_task.take_if(|t| t.handle.is_finished()) {
            let outcome = (&mut task.handle).await;
            self.import_finished(&task, outcome);
        }
    }

    /// The client is gone or stopped answering. Clears the UI but keeps
    /// `last_import`: a hiccup (timeout, stale port) in the middle of champ
    /// select must not cause a second auto-import when we reconnect into the
    /// same champ select. It's forgotten once the client is seen elsewhere
    /// (`not_in_champ_select`).
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

    /// Not in champ select this poll. `elsewhere`: the client reports another
    /// phase (a real leave). Otherwise it's an error answer / a starting
    /// client, which is shown only if it lasts (the UI would lose what it
    /// knows about this champ select, e.g. the auto-import it uses for the
    /// "Import matchup build?" hint) and never forgets the import. After a
    /// real leave the next champ select (e.g. after a dodge) imports again.
    async fn not_in_champ_select<R: Runtime>(
        &mut self,
        app: &AppHandle<R>,
        state: &AppState,
        elsewhere: bool,
    ) {
        self.polls_outside = self.polls_outside.saturating_add(1);
        if elsewhere {
            self.polls_elsewhere = self.polls_elsewhere.saturating_add(1);
        }
        if elsewhere || self.polls_outside >= LEAVE_POLLS {
            set_champ_select(app, state, ChampSelectState::default()).await;
        }
        if self.polls_elsewhere >= LEAVE_POLLS
            && (self.last_import.is_some() || self.import_task.is_some())
        {
            self.forget_import();
        }
    }

    /// In champ select: if it's a different one than the remembered import
    /// is for (a dodge and requeue, or the app restarted into a new one),
    /// start over.
    fn check_identity(&mut self, identity: Option<&str>) {
        let known = self
            .last_import
            .as_ref()
            .and_then(|l| l.identity.as_deref());
        if let (Some(known), Some(current)) = (known, identity) {
            if known != current {
                self.forget_import();
            }
        }
    }

    /// The champ select the import memory is for is over.
    fn forget_import(&mut self) {
        // An import still running for it sees this and writes nothing.
        self.generation.fetch_add(1, Ordering::SeqCst);
        #[cfg(test)]
        self.detached
            .extend(self.import_task.take().map(|task| task.handle));
        self.import_task = None; // left running, detached
        self.last_import = None;
        if let Some(path) = &self.marker_path {
            let _ = std::fs::remove_file(path);
        }
    }

    /// Load the u.gg champion roles in the background; afterwards check
    /// hourly whether u.gg moved to a new patch and reload them if so.
    fn refresh_roles<R: Runtime>(&mut self, app: &AppHandle<R>) {
        if self.roles.task.is_some() {
            return;
        }
        let interval = if self.roles.ok {
            ROLES_REFRESH
        } else {
            ROLES_RETRY
        };
        if self.roles.checked.is_some_and(|t| t.elapsed() < interval) {
            return;
        }
        let now = Instant::now();
        self.roles.checked = Some(now);
        let loaded = self.roles.patch.clone();
        let app = app.clone();
        let handle = tokio::spawn(async move {
            let state = app.state::<AppState>();
            // The versions file is re-read from u.gg at most hourly.
            if loaded.is_some() && loaded == Some(state.ugg.latest_patch().await?) {
                return Ok(None);
            }
            state.ugg.primary_roles_with_patch().await.map(Some)
        });
        self.roles.task = Some((now, handle));
    }

    fn roles_loaded(&mut self, result: Result<RolesResult, JoinError>) {
        self.roles.ok = match result {
            Ok(Ok(Some((patch, map)))) => {
                self.roles.map = map;
                self.roles.patch = Some(patch);
                true
            }
            Ok(Ok(None)) => true,
            Ok(Err(e)) => {
                self.log(format!("Loading champion roles failed: {e:#}"));
                false
            }
            Err(e) => {
                self.log(format!("Loading champion roles failed: {e}"));
                false
            }
        };
    }

    /// The first roles load is still running (started moments ago).
    fn roles_pending(&self) -> bool {
        self.roles.patch.is_none()
            && self
                .roles
                .task
                .as_ref()
                .is_some_and(|(started, _)| started.elapsed() < ROLES_WAIT)
    }

    /// Import runes + item set exactly once per champ select, when the local
    /// player locks in, using the lane opponent known at that moment. Later
    /// changes (enemy locks, trades, the user editing pages) never trigger a
    /// re-import.
    ///
    /// The build is fetched and imported in the background (`ImportJob`).
    async fn auto_import<R: Runtime>(
        &mut self,
        app: &AppHandle<R>,
        state: &AppState,
        client: &LcuClient,
        cs: &ChampSelectState,
        identity: Option<String>,
    ) {
        if !cs.my_champion_locked || self.import_task.is_some() {
            return;
        }
        let (Some(champion_id), Some(queue)) = (cs.my_champion_id, cs.queue) else {
            return;
        };
        if !import_due(self.last_import.as_ref(), champion_id, queue) {
            return;
        }
        // The lane opponent is guessed from the roles: give their first load
        // a moment rather than importing without the opponent.
        if self.roles_pending() {
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
            self.import_done(champion_id, identity);
            return;
        }
        let generation = self.generation.load(Ordering::SeqCst);
        let job = ImportJob {
            app: app.clone(),
            client: client.clone(),
            champion_id,
            role: cs.my_role,
            opponent_id: cs.lane_opponent_id,
            queue,
            settings,
            identity: identity.clone(),
            generation: self.generation.clone(),
            started_in: generation,
        };
        self.last_import = Some(LastImport {
            champion_id,
            identity: identity.clone(),
        });
        self.import_task = Some(ImportTask {
            champion_id,
            identity,
            generation,
            handle: tokio::spawn(job.run()),
        });
    }

    fn import_finished(&mut self, task: &ImportTask, outcome: Result<Outcome, JoinError>) {
        if task.generation != self.generation.load(Ordering::SeqCst) {
            return; // for a champ select that is over
        }
        let done = match outcome {
            Ok(Outcome::Imported) => true,
            // A trade before the import landed. Once per champ select:
            // nothing for the traded champion (manual Import is there).
            Ok(Outcome::ChampionChanged) => {
                self.log("Auto-import skipped: my champion changed first".into());
                true
            }
            // One chance only: a failed attempt is never retried (the user
            // was told and can press Import).
            Ok(Outcome::NothingWritten(why)) => {
                self.log(format!("Auto-import: {why}"));
                true
            }
            Err(e) => {
                self.log(format!("Auto-import task failed: {e}"));
                true
            }
        };
        if done {
            self.import_done(task.champion_id, task.identity.clone());
        }
    }

    /// This champ select's pick is dealt with; remember it, also on disk.
    fn import_done(&mut self, champion_id: u32, identity: Option<String>) {
        if let (Some(path), Some(champ_select)) = (&self.marker_path, &identity) {
            let marker = Marker {
                champ_select: champ_select.clone(),
                champion_id,
                saved_at: unix_now(),
            };
            if let Err(e) = marker.save(path) {
                let message = format!("Saving {} failed: {e:#}", path.display());
                self.log(message);
            }
        }
        self.last_import = Some(LastImport {
            champion_id,
            identity,
        });
    }

    /// Log errors to stderr, without repeating the same line every second.
    fn log(&mut self, message: String) {
        if self.last_error.as_deref() != Some(message.as_str()) {
            eprintln!("[watcher] {message}");
            self.last_error = Some(message);
        }
    }
}

/// One auto-import, run off the poll loop: fetch the build (u.gg, may be
/// slow), then — only if it's still the same champ select and pick — push it
/// into the client and tell the UI.
struct ImportJob<R: Runtime> {
    app: AppHandle<R>,
    client: LcuClient,
    champion_id: u32,
    role: Option<Role>,
    opponent_id: Option<u32>,
    queue: Queue,
    /// As at lock-in.
    settings: Settings,
    identity: Option<String>,
    generation: Arc<AtomicU64>,
    started_in: u64,
}

impl<R: Runtime> ImportJob<R> {
    async fn run(self) -> Outcome {
        let state = self.app.state::<AppState>();
        let build = match state
            .ugg
            .build(
                self.champion_id,
                self.role,
                self.opponent_id,
                self.queue,
                &self.settings,
            )
            .await
        {
            Ok(build) => build,
            Err(e) => {
                self.notify_failed("Couldn't load the recommended build");
                return Outcome::NothingWritten(format!(
                    "no build for champion {}: {e:#}",
                    self.champion_id
                ));
            }
        };
        let static_data = state.static_data().await.ok();
        if let Err(outcome) = self.still_current().await {
            if matches!(&outcome, Outcome::NothingWritten(why) if why.starts_with("League client"))
            {
                self.notify_failed("The League client didn't answer");
            }
            return outcome;
        }
        let result = self
            .client
            .import_build(&build, &self.settings, static_data.as_ref(), None)
            .await;
        let event = AutoImportEvent {
            champion_id: self.champion_id,
            opponent_id: self.opponent_id,
            result,
        };
        if let Err(e) = self.app.emit("auto-imported", &event) {
            eprintln!("[watcher] emit auto-imported failed: {e}");
        }
        Outcome::Imported
    }

    /// Tell the UI the one auto-import attempt failed (nothing was written).
    fn notify_failed(&self, what: &str) {
        let event = AutoImportEvent {
            champion_id: self.champion_id,
            opponent_id: self.opponent_id,
            result: ImportResult {
                messages: vec![format!(
                    "{what}, so nothing was imported. Press Import to try again."
                )],
                ..ImportResult::default()
            },
        };
        if let Err(e) = self.app.emit("auto-imported", &event) {
            eprintln!("[watcher] emit auto-imported failed: {e}");
        }
    }

    /// Fetching the build can take a while: is this still the champ select
    /// and pick it was started for? Asks the client itself, as the user may
    /// have left a moment ago.
    async fn still_current(&self) -> Result<(), Outcome> {
        let ended = || Outcome::NothingWritten("the champ select ended first".into());
        if self.generation.load(Ordering::SeqCst) != self.started_in {
            return Err(ended());
        }
        match self.client.current_pick().await {
            Ok(Some((identity, champion))) => {
                if self.identity.is_some() && identity.is_some() && identity != self.identity {
                    Err(ended())
                } else if champion != Some(self.champion_id) {
                    Err(Outcome::ChampionChanged)
                } else {
                    Ok(())
                }
            }
            Ok(None) => Err(ended()),
            Err(e) => Err(Outcome::NothingWritten(format!(
                "League client didn't answer: {e:#}"
            ))),
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
fn import_due(last: Option<&LastImport>, _champion_id: u32, _queue: Queue) -> bool {
    // Once per champ select. A later champion change (a trade) is not imported.
    last.is_none()
}

/// "This champ select was auto-imported (or auto-import was off at lock-in)",
/// kept on disk so an app restart mid champ select doesn't import again.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Marker {
    /// `lcu::session_identity` of the champ select.
    champ_select: String,
    champion_id: u32,
    /// Unix seconds.
    saved_at: u64,
}

impl Marker {
    /// The marker in `path` if there is one and it's recent.
    fn load(path: &Path) -> Option<Marker> {
        let marker: Marker = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
        let age = unix_now().checked_sub(marker.saved_at)?;
        (age <= MARKER_MAX_AGE.as_secs()).then_some(marker)
    }

    fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string(self)?)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
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

    fn last(champion_id: u32) -> LastImport {
        LastImport {
            champion_id,
            identity: None,
        }
    }

    #[test]
    fn first_lock_imports() {
        assert!(import_due(None, 83, Queue::RankedSolo));
        assert!(import_due(None, 83, Queue::Swiftplay));
    }

    #[test]
    fn draft_imports_once_per_champ_select() {
        let done = last(83);
        assert!(!import_due(Some(&done), 83, Queue::RankedSolo));
        // Even if the champion changes later (trade), no re-import.
        assert!(!import_due(Some(&done), 86, Queue::RankedSolo));
    }

    #[test]
    fn a_failed_attempt_is_never_retried() {
        // Attempted (in flight, imported or failed): no second chance.
        assert!(!import_due(Some(&last(83)), 83, Queue::RankedSolo));
    }

    #[test]
    fn marker_round_trip_and_expiry() {
        let dir = crate::http_cache::test_util::temp_dir("watcher-marker");
        let path = dir.join("sub").join("last_import.json");
        assert_eq!(Marker::load(&path), None, "no file");
        let marker = Marker {
            champ_select: "game:7212345678".into(),
            champion_id: 83,
            saved_at: unix_now() - 60,
        };
        marker.save(&path).unwrap();
        assert_eq!(Marker::load(&path), Some(marker));
        // Too old, from the future, or garbage: ignored.
        for saved_at in [
            unix_now() - MARKER_MAX_AGE.as_secs() - 60,
            unix_now() + 3600,
        ] {
            Marker {
                champ_select: "game:1".into(),
                champion_id: 83,
                saved_at,
            }
            .save(&path)
            .unwrap();
            assert_eq!(Marker::load(&path), None, "saved_at {saved_at}");
        }
        std::fs::write(&path, "{not json").unwrap();
        assert_eq!(Marker::load(&path), None);
        // A watcher started without a usable marker imports normally.
        assert!(Watcher::new(path.clone()).last_import.is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    // --- the loop itself, against a mock League client ----------------------

    use crate::lcu::tests::{fixture, mock_client, unreachable_client};
    use serde_json::{json, Value};
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

    /// The ranked champ select fixture as a different game (the next champ
    /// select after a dodge has a new game id).
    fn next_champ_select() -> Value {
        let mut session = fixture("champ_select_ranked.json");
        session["gameId"] = json!(7299999999u64);
        session
    }

    /// One poll plus whatever background work it started.
    async fn tick(w: &mut Watcher, app: &AppHandle<MockRuntime>) {
        w.tick(app).await;
        w.settle().await;
    }

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
                Settings::auto_on(),
            );
            let (handle, state) = (app.handle(), app.state::<AppState>());
            let posts = || mock.lock().unwrap().requests_to(RUNE_POST.0, RUNE_POST.1);
            let mut w = Watcher::default();

            // (The first poll waits for the champion roles.)
            tick(&mut w, handle).await;
            tick(&mut w, handle).await;
            assert_eq!(posts(), 1, "auto-import at lock-in");

            // The client stops answering for a moment (timeout / restart)…
            *state.lcu.write().await = Some(unreachable_client().await);
            tick(&mut w, handle).await;
            assert!(state.lcu.read().await.is_none());
            assert!(!state.lcu_status.read().await.connected);
            // …and is back, still in the same champ select: the user may have
            // edited the imported page by now, so nothing is imported again.
            *state.lcu.write().await = Some(client.clone());
            tick(&mut w, handle).await;
            tick(&mut w, handle).await;
            assert!(state.champ_select.read().await.in_champ_select);
            assert_eq!(posts(), 1, "re-imported after a reconnect");

            // The session is missing for a few polls (phase still says
            // ChampSelect): a hiccup, not a new champ select.
            mock.lock().unwrap().session = None;
            for _ in 0..LEAVE_POLLS + 2 {
                tick(&mut w, handle).await;
            }
            mock.lock().unwrap().session = Some(fixture("champ_select_ranked.json"));
            tick(&mut w, handle).await;
            assert_eq!(posts(), 1, "re-imported after a missing session");

            // The next champ select imports again.
            mock.lock().unwrap().session = Some(next_champ_select());
            tick(&mut w, handle).await;
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
                ..Settings::auto_on()
            };
            manage_state(&app, "watcher-toggle", client, off);
            let (handle, state) = (app.handle(), app.state::<AppState>());
            let posts = || mock.lock().unwrap().requests_to(RUNE_POST.0, RUNE_POST.1);
            let mut w = Watcher::default();

            tick(&mut w, handle).await;
            tick(&mut w, handle).await;
            assert_eq!(posts(), 0, "auto-import is off");
            // Turned on after locking in: it was off at lock-in, so no import.
            state.settings.write().await.auto_import = true;
            tick(&mut w, handle).await;
            assert_eq!(posts(), 0, "imported after lock-in");

            // The next champ select imports at lock-in.
            mock.lock().unwrap().session = Some(next_champ_select());
            tick(&mut w, handle).await;
            assert_eq!(posts(), 1);
        });
    }
}
