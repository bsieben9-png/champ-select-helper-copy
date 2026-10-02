mod ddragon;
#[cfg(test)]
mod e2e;
mod http_cache;
mod lcu;
mod lobby;
pub mod model;
mod settings;
mod ugg;
mod watcher;

use std::path::PathBuf;

use tauri::{Manager, State};
use tokio::sync::RwLock;

use model::*;

/// Shared app state, available to commands and the background watcher.
pub struct AppState {
    pub ugg: ugg::Ugg,
    pub ddragon: ddragon::DDragon,
    pub settings: RwLock<Settings>,
    pub settings_path: PathBuf,
    pub lcu: RwLock<Option<lcu::LcuClient>>,
    pub lcu_status: RwLock<LcuStatus>,
    pub champ_select: RwLock<ChampSelectState>,
    pub static_data: RwLock<Option<StaticData>>,
}

impl AppState {
    /// Static data, loaded once and cached in memory.
    pub async fn static_data(&self) -> anyhow::Result<StaticData> {
        if let Some(sd) = self.static_data.read().await.as_ref() {
            return Ok(sd.clone());
        }
        // Names and icons come from Data Dragon: u.gg being unreachable must
        // not leave the whole UI (champ select view, lookup) without them.
        // Without the u.gg parts the result isn't kept, so they're retried.
        let patch = self.ugg.latest_patch().await;
        let roles = match &patch {
            Ok(_) => self.ugg.primary_roles().await.ok(),
            Err(_) => None,
        };
        let complete = patch.is_ok() && roles.is_some();
        let sd = self
            .ddragon
            .static_data(&patch.unwrap_or_default(), &roles.unwrap_or_default())
            .await?;
        if complete {
            *self.static_data.write().await = Some(sd.clone());
        }
        Ok(sd)
    }
}

type CmdResult<T> = Result<T, String>;

fn err(e: anyhow::Error) -> String {
    format!("{e:#}")
}

#[tauri::command]
async fn get_static_data(state: State<'_, AppState>) -> CmdResult<StaticData> {
    state.static_data().await.map_err(err)
}

#[tauri::command]
async fn get_build(
    state: State<'_, AppState>,
    champion_id: u32,
    role: Option<Role>,
    opponent_id: Option<u32>,
    queue: Queue,
) -> CmdResult<Build> {
    let settings = state.settings.read().await.clone();
    state
        .ugg
        .build(champion_id, role, opponent_id, queue, &settings)
        .await
        .map_err(err)
}

#[tauri::command]
async fn get_counters(
    state: State<'_, AppState>,
    enemy_id: u32,
    role: Role,
    queue: Queue,
) -> CmdResult<Vec<Counter>> {
    let settings = state.settings.read().await.clone();
    state
        .ugg
        .counters(enemy_id, role, queue, &settings)
        .await
        .map_err(err)
}

#[tauri::command]
async fn get_tier_list(
    state: State<'_, AppState>,
    role: Role,
    queue: Queue,
) -> CmdResult<Vec<TierEntry>> {
    let settings = state.settings.read().await.clone();
    state
        .ugg
        .tier_list(role, queue, &settings)
        .await
        .map_err(err)
}

#[tauri::command]
async fn get_matchups(
    state: State<'_, AppState>,
    champion_id: u32,
    role: Role,
    queue: Queue,
) -> CmdResult<Vec<MatchupStat>> {
    let settings = state.settings.read().await.clone();
    state
        .ugg
        .matchups(champion_id, role, queue, &settings)
        .await
        .map_err(err)
}

#[tauri::command]
async fn get_settings(state: State<'_, AppState>) -> CmdResult<Settings> {
    Ok(state.settings.read().await.clone())
}

#[tauri::command]
async fn save_settings(state: State<'_, AppState>, new_settings: Settings) -> CmdResult<()> {
    settings::save(&state.settings_path, &new_settings).map_err(err)?;
    *state.settings.write().await = new_settings;
    Ok(())
}

#[tauri::command]
async fn get_champ_select(state: State<'_, AppState>) -> CmdResult<ChampSelectState> {
    Ok(state.champ_select.read().await.clone())
}

/// The lobby before queueing (my Swiftplay / Quickplay slots).
#[tauri::command]
async fn get_lobby(store: State<'_, lobby::LobbyStore>) -> CmdResult<LobbyState> {
    Ok(store.lobby.read().await.clone())
}

#[tauri::command]
async fn get_lcu_status(state: State<'_, AppState>) -> CmdResult<LcuStatus> {
    Ok(state.lcu_status.read().await.clone())
}

/// `overwrite_page_id`: the user agreed to replace this rune page (from
/// `ImportResult.needs_confirmation`) because all rune page slots are used.
#[tauri::command]
async fn import_build(
    state: State<'_, AppState>,
    build: Build,
    overwrite_page_id: Option<u64>,
) -> CmdResult<ImportResult> {
    let settings = state.settings.read().await.clone();
    let static_data = state.static_data().await.ok();
    // Clone the client so the lock isn't held during the import.
    let Some(client) = state.lcu.read().await.clone() else {
        return Err("League client is not running".into());
    };
    Ok(client
        .import_build(&build, &settings, static_data.as_ref(), overwrite_page_id)
        .await)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            let cache_dir = app.path().app_cache_dir()?;
            std::fs::create_dir_all(&config_dir)?;
            std::fs::create_dir_all(&cache_dir)?;
            let settings_path = config_dir.join("settings.json");
            let state = AppState {
                ugg: ugg::Ugg::new(cache_dir.join("ugg")),
                ddragon: ddragon::DDragon::new(cache_dir.join("ddragon")),
                settings: RwLock::new(settings::load(&settings_path)),
                settings_path,
                lcu: RwLock::new(None),
                lcu_status: RwLock::new(LcuStatus::default()),
                champ_select: RwLock::new(ChampSelectState::default()),
                static_data: RwLock::new(None),
            };
            app.manage(state);
            app.manage(lobby::LobbyStore::default());
            watcher::spawn(app.handle().clone());
            lobby::spawn(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_static_data,
            get_build,
            get_counters,
            get_tier_list,
            get_matchups,
            get_settings,
            save_settings,
            get_champ_select,
            get_lcu_status,
            import_build,
            get_lobby,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Champ Select Helper");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app_state(ugg: ugg::Ugg, ddragon: ddragon::DDragon) -> AppState {
        AppState {
            ugg,
            ddragon,
            settings: RwLock::new(Settings::default()),
            settings_path: PathBuf::new(),
            lcu: RwLock::new(None),
            lcu_status: RwLock::new(LcuStatus::default()),
            champ_select: RwLock::new(ChampSelectState::default()),
            static_data: RwLock::new(None),
        }
    }

    #[tokio::test]
    async fn static_data_does_not_need_ugg() {
        // u.gg unreachable and nothing cached: every u.gg file is "missing".
        let (ugg, ugg_dir) = ugg::tests::seeded_ugg("static-no-ugg");
        std::fs::remove_dir_all(&ugg_dir).unwrap();
        let (dd, _) = ddragon::tests::seeded("static-no-ugg-dd");
        let state = app_state(ugg, dd);
        let sd = state.static_data().await.expect("Data Dragon data");
        assert!(sd.champions.iter().any(|c| c.name == "Yorick"));
        assert_eq!(sd.ugg_patch, "");
        // Not kept: the u.gg parts are retried next time.
        assert!(state.static_data.read().await.is_none());

        // With u.gg: complete and kept.
        let (ugg, _) = ugg::tests::seeded_ugg("static-with-ugg");
        let (dd, _) = ddragon::tests::seeded("static-with-ugg-dd");
        let state = app_state(ugg, dd);
        let sd = state.static_data().await.unwrap();
        assert_eq!(sd.ugg_patch, "16_19");
        let yorick = sd.champions.iter().find(|c| c.id == 83).unwrap();
        assert_eq!(yorick.roles.first(), Some(&Role::Top));
        assert!(state.static_data.read().await.is_some());
    }
}
