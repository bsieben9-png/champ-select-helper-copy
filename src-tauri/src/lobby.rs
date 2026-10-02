//! Swiftplay / Quickplay: there is no champ select. Champions (with their
//! runes and spells) are picked per position in the LOBBY, before queueing
//! (`localMember.playerSlots` in the client). This loop runs next to the
//! champ select watcher (`watcher.rs`) and emits `lobby` (my slots) while the
//! client is in Lobby / Matchmaking / ReadyCheck, so the Live view can show
//! each slot's build.
//!
//! READ ONLY (owner rule): the app never writes to the lobby / player-slots
//! API and never auto-imports here. The user imports a slot's build by hand
//! (Import button → a `CSH:` rune page + item set, like in champ select) and
//! picks that page for the slot in the client.
//!
//! It reads the gameflow phase the champ select watcher stores in
//! `AppState.lcu_status` and never connects by itself.

use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, Runtime};
use tokio::sync::RwLock;

use crate::model::*;
use crate::AppState;

/// Poll interval (the lobby changes at human speed).
const POLL_INTERVAL: Duration = Duration::from_secs(1);
/// Gameflow phases in which the lobby (and my slots) are shown.
const LOBBY_PHASES: [&str; 3] = ["Lobby", "Matchmaking", "ReadyCheck"];

/// The current lobby, for the `get_lobby` command.
#[derive(Default)]
pub struct LobbyStore {
    pub lobby: RwLock<LobbyState>,
}

pub fn spawn<R: Runtime>(app: AppHandle<R>) {
    tauri::async_runtime::spawn(async move {
        let mut watcher = LobbyWatcher::default();
        loop {
            let delay = watcher.tick(&app).await;
            tokio::time::sleep(delay).await;
        }
    });
}

#[derive(Default)]
pub(crate) struct LobbyWatcher {
    last_error: Option<String>,
}

impl LobbyWatcher {
    /// One iteration; returns how long to sleep before the next one.
    pub(crate) async fn tick<R: Runtime>(&mut self, app: &AppHandle<R>) -> Duration {
        let state = app.state::<AppState>();
        let status = state.lcu_status.read().await.clone();
        let client = state.lcu.read().await.clone();
        let Some(client) = client
            .filter(|_| status.connected && LOBBY_PHASES.contains(&status.phase.as_str()))
        else {
            // Champ select, in game, client closed: no lobby to show.
            set_lobby(app, LobbyState::default()).await;
            return POLL_INTERVAL;
        };
        match client.lobby().await {
            Ok(lobby) => set_lobby(app, lobby).await,
            // Keep showing the last state; the next poll retries.
            Err(e) => self.log(format!("Reading the lobby failed: {e:#}")),
        }
        POLL_INTERVAL
    }

    /// Log errors to stderr, without repeating the same line every second.
    fn log(&mut self, message: String) {
        if self.last_error.as_deref() != Some(message.as_str()) {
            eprintln!("[lobby] {message}");
            self.last_error = Some(message);
        }
    }
}

/// Store + emit `lobby` when it changed.
async fn set_lobby<R: Runtime>(app: &AppHandle<R>, lobby: LobbyState) {
    let Some(store) = app.try_state::<LobbyStore>() else {
        return;
    };
    {
        let mut current = store.lobby.write().await;
        if *current == lobby {
            return;
        }
        *current = lobby.clone();
    }
    let _ = app.emit("lobby", &lobby);
}
