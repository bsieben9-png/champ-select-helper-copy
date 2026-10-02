//! Background loop: connects to the League client, polls champ select,
//! emits `lcu-status` / `champ-select` / `auto-imported` events, and runs
//! auto-import. OWNER: client agent.

use tauri::AppHandle;

pub fn spawn(_app: AppHandle) {}
