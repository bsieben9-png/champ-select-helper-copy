//! League Client (LCU) connection: discovery, champ select parsing, import.
//! OWNER: client agent. Public signatures are a contract — don't change them.

use std::collections::HashMap;

use serde_json::Value;

use crate::model::*;

pub struct LcuClient {}

impl LcuClient {
    /// Find a running League client (lockfile). None if not running.
    pub fn discover() -> Option<LcuClient> {
        None
    }

    /// GET a JSON endpoint, e.g. "/lol-champ-select/v1/session".
    pub async fn get(&self, _path: &str) -> anyhow::Result<Value> {
        anyhow::bail!("not implemented")
    }

    /// Connection status + gameflow phase. Err means the client went away.
    pub async fn status(&self) -> anyhow::Result<LcuStatus> {
        anyhow::bail!("not implemented")
    }

    /// Current champ select state (default/not-in-champ-select when none).
    pub async fn champ_select(
        &self,
        _roles: &HashMap<u32, Vec<Role>>,
    ) -> anyhow::Result<ChampSelectState> {
        anyhow::bail!("not implemented")
    }

    /// Push runes / spells / item set (per settings toggles) into the client.
    /// Never panics; failures are reported in `ImportResult.messages`.
    pub async fn import_build(
        &self,
        _build: &Build,
        _settings: &Settings,
        _static_data: Option<&StaticData>,
    ) -> ImportResult {
        ImportResult::default()
    }
}

/// Pure: turn a `/lol-champ-select/v1/session` JSON into our state.
pub fn parse_champ_select(
    _session: &Value,
    _queue_id: Option<i64>,
    _roles: &HashMap<u32, Vec<Role>>,
) -> ChampSelectState {
    ChampSelectState::default()
}

/// Pure: best-guess role for each enemy champion id (0 = not picked yet).
pub fn infer_enemy_roles(
    _enemy_ids: &[u32],
    _roles: &HashMap<u32, Vec<Role>>,
) -> Vec<Option<Role>> {
    Vec::new()
}
