//! u.gg data source. See DESIGN.md "u.gg data" for the file formats.
//! OWNER: data agent. Public signatures are a contract — don't change them.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::model::*;

pub struct Ugg {
    #[allow(dead_code)]
    cache_dir: PathBuf,
}

impl Ugg {
    pub fn new(cache_dir: PathBuf) -> Self {
        Ugg { cache_dir }
    }

    /// Latest u.gg patch, e.g. "16_19".
    pub async fn latest_patch(&self) -> anyhow::Result<String> {
        anyhow::bail!("not implemented")
    }

    /// Champion id → roles, most played first.
    pub async fn primary_roles(&self) -> anyhow::Result<HashMap<u32, Vec<Role>>> {
        anyhow::bail!("not implemented")
    }

    /// Recommended build. `role: None` → the champion's most played role
    /// (ignored for ARAM). `opponent_id` → matchup-specific build, falling
    /// back to the general build when the matchup has too little data.
    pub async fn build(
        &self,
        _champion_id: u32,
        _role: Option<Role>,
        _opponent_id: Option<u32>,
        _queue: Queue,
        _settings: &Settings,
    ) -> anyhow::Result<Build> {
        anyhow::bail!("not implemented")
    }

    /// How `champion_id` does vs every opponent in `role`.
    pub async fn matchups(
        &self,
        _champion_id: u32,
        _role: Role,
        _queue: Queue,
        _settings: &Settings,
    ) -> anyhow::Result<Vec<MatchupStat>> {
        anyhow::bail!("not implemented")
    }

    /// Best counter-picks vs `enemy_id` in `role`, best first.
    pub async fn counters(
        &self,
        _enemy_id: u32,
        _role: Role,
        _queue: Queue,
        _settings: &Settings,
    ) -> anyhow::Result<Vec<Counter>> {
        anyhow::bail!("not implemented")
    }
}
