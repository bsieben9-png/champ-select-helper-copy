//! Riot Data Dragon static data (names, icons, runes, items, spells).
//! OWNER: data agent. Public signatures are a contract — don't change them.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::model::*;

pub struct DDragon {
    #[allow(dead_code)]
    cache_dir: PathBuf,
}

impl DDragon {
    pub fn new(cache_dir: PathBuf) -> Self {
        DDragon { cache_dir }
    }

    /// Load all static data for the latest Data Dragon version.
    /// `ugg_patch` and `roles` are copied into the result.
    pub async fn static_data(
        &self,
        _ugg_patch: &str,
        _roles: &HashMap<u32, Vec<Role>>,
    ) -> anyhow::Result<StaticData> {
        anyhow::bail!("not implemented")
    }
}
