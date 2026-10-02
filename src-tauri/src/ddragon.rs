//! Riot Data Dragon static data (names, icons, runes, items, spells).
//! OWNER: data agent. Public signatures are a contract — don't change them.
//!
//! Files under `/cdn/{version}/` never change, so they are cached on disk
//! forever (older versions are deleted once a newer one loaded); the versions
//! list is refreshed every 6 h. Offline → the cached copies are used.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{anyhow, Context};
use serde_json::Value;

use crate::http_cache::{parse_json, HttpCache};
use crate::model::*;

const DDRAGON: &str = "https://ddragon.leagueoflegends.com";
const VERSIONS_URL: &str = "https://ddragon.leagueoflegends.com/api/versions.json";
const VERSIONS_TTL: Duration = Duration::from_secs(6 * 60 * 60);
/// Versioned files are immutable.
const VERSIONED_TTL: Duration = Duration::from_secs(10 * 365 * 24 * 60 * 60);
/// If the newest version can't be loaded, try this many versions in total.
const VERSIONS_TO_TRY: usize = 2;

/// Stat shards aren't in runesReforged.json: (id, name, description, icon file).
const SHARDS: &[(u32, &str, &str, &str)] = &[
    (
        5001,
        "Health Scaling",
        "+10-180 Health (based on level)",
        "StatModsHealthScalingIcon.png",
    ),
    (
        5005,
        "Attack Speed",
        "+10% Attack Speed",
        "StatModsAttackSpeedIcon.png",
    ),
    (
        5007,
        "Ability Haste",
        "+8 Ability Haste",
        "StatModsCDRScalingIcon.png",
    ),
    (
        5008,
        "Adaptive Force",
        "+9 Adaptive Force",
        "StatModsAdaptiveForceIcon.png",
    ),
    (
        5010,
        "Move Speed",
        "+2% Move Speed",
        "StatModsMovementSpeedIcon.png",
    ),
    (5011, "Health", "+65 Health", "StatModsHealthPlusIcon.png"),
    (
        5013,
        "Tenacity and Slow Resist",
        "+10% Tenacity and Slow Resist",
        "StatModsTenacityIcon.png",
    ),
];

pub struct DDragon {
    http: HttpCache,
}

impl DDragon {
    pub fn new(cache_dir: PathBuf) -> Self {
        DDragon {
            http: HttpCache::new(cache_dir, 8),
        }
    }

    /// Load all static data for the latest Data Dragon version.
    /// `ugg_patch` and `roles` are copied into the result.
    pub async fn static_data(
        &self,
        ugg_patch: &str,
        roles: &HashMap<u32, Vec<Role>>,
    ) -> anyhow::Result<StaticData> {
        let versions = self
            .http
            .get(VERSIONS_URL, VERSIONS_TTL, parse_versions)
            .await?
            .ok_or_else(|| anyhow!("Data Dragon versions list not found"))?;
        let mut first_err = None;
        for (i, version) in versions.iter().take(VERSIONS_TO_TRY).enumerate() {
            match self.load(version, ugg_patch, roles).await {
                Ok(data) => {
                    // The caller keeps StaticData; the parsed JSON isn't needed.
                    self.http.clear_memory();
                    if i == 0 {
                        self.prune_other_versions(version);
                    }
                    return Ok(data);
                }
                Err(e) => {
                    eprintln!("ddragon: version {version}: {e:#}");
                    first_err.get_or_insert(e);
                }
            }
        }
        Err(first_err.unwrap_or_else(|| anyhow!("Data Dragon versions list is empty")))
    }

    async fn load(
        &self,
        version: &str,
        ugg_patch: &str,
        roles: &HashMap<u32, Vec<Role>>,
    ) -> anyhow::Result<StaticData> {
        let (champions, items, runes, spells) = tokio::try_join!(
            self.data_file(version, "champion"),
            self.data_file(version, "item"),
            self.data_file(version, "runesReforged"),
            self.data_file(version, "summoner"),
        )?;
        Ok(StaticData {
            version: version.to_string(),
            ugg_patch: ugg_patch.to_string(),
            champions: champions_from_json(&champions, version, roles)?,
            items: items_from_json(&items, version)?,
            rune_styles: rune_styles_from_json(&runes)?,
            shards: shards(),
            spells: spells_from_json(&spells, version)?,
        })
    }

    async fn data_file(&self, version: &str, name: &str) -> anyhow::Result<std::sync::Arc<Value>> {
        let url = format!("{DDRAGON}/cdn/{version}/data/en_US/{name}.json");
        self.http
            .get(&url, VERSIONED_TTL, parse_json)
            .await?
            .with_context(|| format!("{url} not found"))
    }

    /// Delete cached `/cdn/{other version}/` files.
    fn prune_other_versions(&self, version: &str) {
        let keep = format!("~cdn~{version}~");
        self.http.prune(|name, _| {
            !name.ends_with(".tmp") && (!name.contains("~cdn~") || name.contains(&keep))
        });
    }
}

/// `["16.19.1", "16.18.1", …, "lolpatch_7.20", …]` → numeric versions, in order.
fn parse_versions(body: &[u8]) -> anyhow::Result<Vec<String>> {
    let list: Vec<String> = serde_json::from_slice::<Vec<Value>>(body)?
        .iter()
        .filter_map(Value::as_str)
        .filter(|v| {
            !v.is_empty()
                && v.split('.')
                    .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
        })
        .map(str::to_string)
        .collect();
    if list.is_empty() {
        anyhow::bail!("Data Dragon versions list is empty");
    }
    Ok(list)
}

fn data_object<'a>(v: &'a Value, what: &str) -> anyhow::Result<&'a serde_json::Map<String, Value>> {
    v.get("data")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("{what}: no \"data\" object"))
}

fn str_field<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// `image.full`, or `{fallback}.png`.
fn image_file(v: &Value, fallback: &str) -> String {
    v.get("image")
        .and_then(|i| i.get("full"))
        .and_then(Value::as_str)
        .map_or_else(|| format!("{fallback}.png"), str::to_string)
}

fn champions_from_json(
    v: &Value,
    version: &str,
    roles: &HashMap<u32, Vec<Role>>,
) -> anyhow::Result<Vec<ChampionInfo>> {
    let mut out: Vec<ChampionInfo> = data_object(v, "champion.json")?
        .iter()
        .filter_map(|(dd_id, c)| {
            let id: u32 = str_field(c, "key").parse().ok()?;
            let key = c
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or(dd_id)
                .to_string();
            let image = image_file(c, &key);
            Some(ChampionInfo {
                id,
                name: str_field(c, "name").to_string(),
                icon: format!("{DDRAGON}/cdn/{version}/img/champion/{image}"),
                roles: roles.get(&id).cloned().unwrap_or_default(),
                key,
            })
        })
        .collect();
    out.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then(a.id.cmp(&b.id))
    });
    Ok(out)
}

/// Every item with a name (map flags in item.json are unreliable, e.g. jungle
/// pets / support upgrades), sorted by id.
fn items_from_json(v: &Value, version: &str) -> anyhow::Result<Vec<ItemInfo>> {
    let mut out: Vec<ItemInfo> = data_object(v, "item.json")?
        .iter()
        .filter_map(|(id, item)| {
            let id: u32 = id.parse().ok()?;
            let name = str_field(item, "name").trim();
            if name.is_empty() {
                return None;
            }
            let image = image_file(item, &id.to_string());
            Some(ItemInfo {
                id,
                name: name.to_string(),
                icon: format!("{DDRAGON}/cdn/{version}/img/item/{image}"),
                gold: item
                    .get("gold")
                    .and_then(|g| g.get("total"))
                    .and_then(Value::as_u64)
                    .map_or(0, |g| g.min(u64::from(u32::MAX)) as u32),
                description: strip_html(str_field(item, "description")),
            })
        })
        .collect();
    out.sort_by_key(|i| i.id);
    Ok(out)
}

fn rune_info(r: &Value) -> Option<RuneInfo> {
    Some(RuneInfo {
        id: u32::try_from(r.get("id")?.as_u64()?).ok()?,
        name: str_field(r, "name").to_string(),
        icon: format!("{DDRAGON}/cdn/img/{}", str_field(r, "icon")),
        short_desc: strip_html(str_field(r, "shortDesc")),
    })
}

fn rune_styles_from_json(v: &Value) -> anyhow::Result<Vec<RuneStyle>> {
    let styles = v
        .as_array()
        .ok_or_else(|| anyhow!("runesReforged.json: expected an array"))?;
    Ok(styles
        .iter()
        .filter_map(|style| {
            let info = rune_info(style)?;
            let slots = style
                .get("slots")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|slot| {
                    slot.get("runes")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(rune_info)
                        .collect()
                })
                .collect();
            Some(RuneStyle {
                id: info.id,
                name: info.name,
                icon: info.icon,
                slots,
            })
        })
        .collect())
}

fn shards() -> Vec<RuneInfo> {
    SHARDS
        .iter()
        .map(|&(id, name, desc, icon)| RuneInfo {
            id,
            name: name.to_string(),
            icon: format!("{DDRAGON}/cdn/img/perk-images/StatMods/{icon}"),
            short_desc: desc.to_string(),
        })
        .collect()
}

fn spells_from_json(v: &Value, version: &str) -> anyhow::Result<Vec<SpellInfo>> {
    let mut out: Vec<SpellInfo> = data_object(v, "summoner.json")?
        .iter()
        .filter_map(|(dd_id, s)| {
            let id: u32 = str_field(s, "key").parse().ok()?;
            let dd_id = s.get("id").and_then(Value::as_str).unwrap_or(dd_id);
            let image = image_file(s, dd_id);
            Some(SpellInfo {
                id,
                name: str_field(s, "name").to_string(),
                icon: format!("{DDRAGON}/cdn/{version}/img/spell/{image}"),
            })
        })
        .collect();
    out.sort_by_key(|s| s.id);
    Ok(out)
}

/// HTML → plain text: tags removed, `<br>` (and `<li>`, `<p>`) → newline,
/// common entities decoded, whitespace tidied (at most one blank line).
pub fn strip_html(s: &str) -> String {
    let mut text = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(lt) = rest.find('<') {
        text.push_str(&rest[..lt]);
        let after = &rest[lt + 1..];
        let looks_like_tag =
            after.starts_with(|c: char| c.is_ascii_alphabetic() || c == '/' || c == '!');
        match after.find('>').filter(|_| looks_like_tag) {
            Some(gt) => {
                let name: String = after[..gt]
                    .trim_start_matches('/')
                    .chars()
                    .take_while(char::is_ascii_alphanumeric)
                    .collect::<String>()
                    .to_ascii_lowercase();
                if matches!(name.as_str(), "br" | "li" | "p") {
                    text.push('\n');
                }
                rest = &after[gt + 1..];
            }
            None => {
                text.push('<');
                rest = after;
            }
        }
    }
    text.push_str(rest);

    let text = text
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&amp;", "&");

    let mut out = String::with_capacity(text.len());
    let mut blank_lines = 0;
    for line in text.lines() {
        let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.is_empty() {
            blank_lines += 1;
            if blank_lines > 1 || out.is_empty() {
                continue;
            }
        } else {
            blank_lines = 0;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&line);
    }
    out.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http_cache::test_util::{fixture, temp_dir};

    const V: &str = "16.19.1";

    fn seeded(name: &str) -> (DDragon, PathBuf) {
        let dir = temp_dir(name);
        let dd = DDragon {
            http: HttpCache::new_offline(dir.clone(), 8),
        };
        let seed = |url: &str, fixture_name: &str| {
            std::fs::write(dd.http.path_for(url), fixture(fixture_name)).unwrap();
        };
        seed(VERSIONS_URL, "ddragon_versions.json");
        for name in ["champion", "item", "runesReforged", "summoner"] {
            seed(
                &format!("{DDRAGON}/cdn/{V}/data/en_US/{name}.json"),
                &format!("ddragon_{name}.json"),
            );
        }
        (dd, dir)
    }

    #[test]
    fn strip_html_basics() {
        assert_eq!(
            strip_html("<mainText><stats><attention>36</attention> Attack Damage<br><attention>30%</attention> Attack Speed</stats><br><br><passive>Spellblade</passive><br>After using an Ability.<br> <br><br>x</mainText>"),
            "36 Attack Damage\n30% Attack Speed\n\nSpellblade\nAfter using an Ability.\n\nx"
        );
        assert_eq!(
            strip_html("deals <lol-uikit-tooltipped-keyword key='X'><font color='#48C4B7'>adaptive damage</font></lol-uikit-tooltipped-keyword>."),
            "deals adaptive damage."
        );
        assert_eq!(
            strip_html("a < b &amp; c&nbsp;>&nbsp;d<br/>e<br />f"),
            "a < b & c > d\ne\nf"
        );
        assert_eq!(strip_html(""), "");
    }

    #[test]
    fn versions_skip_non_numeric() {
        let v = parse_versions(&fixture("ddragon_versions.json")).unwrap();
        assert_eq!(v, ["16.19.1", "16.18.1", "16.17.1"]);
        assert!(parse_versions(b"[]").is_err());
    }

    #[tokio::test]
    async fn static_data_offline() {
        let (dd, dir) = seeded("ddragon");
        let roles = HashMap::from([(83, vec![Role::Top, Role::Mid])]);
        let sd = dd.static_data("16_19", &roles).await.unwrap();
        assert_eq!((sd.version.as_str(), sd.ugg_patch.as_str()), (V, "16_19"));

        // Champions: numeric id, ddragon key, sorted by name.
        let names: Vec<&str> = sd.champions.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["Aatrox", "Gwen", "K'Sante", "Wukong", "Yorick"]);
        let wukong = sd.champions.iter().find(|c| c.id == 62).unwrap();
        assert_eq!(wukong.key, "MonkeyKing");
        assert_eq!(
            wukong.icon,
            "https://ddragon.leagueoflegends.com/cdn/16.19.1/img/champion/MonkeyKing.png"
        );
        assert!(wukong.roles.is_empty());
        let yorick = sd.champions.iter().find(|c| c.id == 83).unwrap();
        assert_eq!(yorick.roles, [Role::Top, Role::Mid]);

        // Items: nameless 2008 dropped, HTML stripped, sorted by id.
        let ids: Vec<u32> = sd.items.iter().map(|i| i.id).collect();
        assert_eq!(ids, [1054, 1101, 2003, 3047, 3078, 3869, 6694]);
        let tf = sd.items.iter().find(|i| i.id == 3078).unwrap();
        assert_eq!((tf.name.as_str(), tf.gold), ("Trinity Force", 3333));
        assert_eq!(
            tf.icon,
            "https://ddragon.leagueoflegends.com/cdn/16.19.1/img/item/3078.png"
        );
        assert!(tf.description.starts_with("36 Attack Damage\n"));
        assert!(!tf.description.contains('<'));

        // Runes.
        let resolve = sd.rune_styles.iter().find(|s| s.id == 8400).unwrap();
        assert_eq!(resolve.name, "Resolve");
        assert_eq!(
            resolve.icon,
            "https://ddragon.leagueoflegends.com/cdn/img/perk-images/Styles/7204_Resolve.png"
        );
        assert_eq!(resolve.slots.len(), 4);
        let grasp = &resolve.slots[0][0];
        assert_eq!(grasp.id, 8437);
        assert!(!grasp.short_desc.is_empty() && !grasp.short_desc.contains('<'));

        // Shards and spells.
        let shard_ids: Vec<u32> = sd.shards.iter().map(|s| s.id).collect();
        assert_eq!(shard_ids, [5001, 5005, 5007, 5008, 5010, 5011, 5013]);
        let flash = sd.spells.iter().find(|s| s.id == 4).unwrap();
        assert_eq!(flash.name, "Flash");
        assert_eq!(
            flash.icon,
            "https://ddragon.leagueoflegends.com/cdn/16.19.1/img/spell/SummonerFlash.png"
        );
        assert!(sd.spells.windows(2).all(|w| w[0].id < w[1].id));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn falls_back_to_previous_version_and_prunes() {
        let (dd, dir) = seeded("ddragon-fallback");
        // Newest list entry is 16.20.1, which isn't cached (offline = missing).
        std::fs::write(dd.http.path_for(VERSIONS_URL), br#"["16.20.1","16.19.1"]"#).unwrap();
        let sd = dd.static_data("16_19", &HashMap::new()).await.unwrap();
        assert_eq!(sd.version, V);
        // Nothing pruned when falling back.
        let item = dd
            .http
            .path_for(&format!("{DDRAGON}/cdn/{V}/data/en_US/item.json"));
        assert!(item.exists());
        // Loading the newest version prunes the others.
        dd.prune_other_versions("16.20.1");
        assert!(!item.exists());
        assert!(dd.http.path_for(VERSIONS_URL).exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Live check against Data Dragon: `cargo test -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn live_static_data() {
        let dir = temp_dir("ddragon-live");
        let dd = DDragon::new(dir.clone());
        let sd = dd.static_data("16_19", &HashMap::new()).await.unwrap();
        println!(
            "ddragon {}: {} champions, {} items, {} rune styles, {} spells",
            sd.version,
            sd.champions.len(),
            sd.items.len(),
            sd.rune_styles.len(),
            sd.spells.len()
        );
        assert!(sd.champions.len() > 150);
        assert_eq!(sd.rune_styles.len(), 5);
        assert!(sd.spells.iter().any(|s| s.id == 4) && sd.spells.iter().any(|s| s.id == 14));
        // Every item / rune id in the u.gg fixtures has an entry.
        let ugg: Value =
            serde_json::from_slice(&fixture("overview_83_ranked_solo_5x5.json")).unwrap();
        let entry = &ugg["12"]["17"]["4"][0];
        for idx in [2, 3] {
            for item in entry[idx][2].as_array().unwrap() {
                let item = item.as_u64().unwrap() as u32;
                assert!(sd.items.iter().any(|i| i.id == item), "item {item}");
            }
        }
        for perk in entry[0][4].as_array().unwrap() {
            let perk = perk.as_u64().unwrap() as u32;
            assert!(
                sd.rune_styles
                    .iter()
                    .flat_map(|s| s.slots.iter().flatten())
                    .any(|r| r.id == perk),
                "perk {perk}"
            );
        }
        // Second load comes from disk.
        let again = DDragon::new(dir.clone())
            .static_data("16_19", &HashMap::new())
            .await
            .unwrap();
        assert_eq!(again, sd);
        let _ = std::fs::remove_dir_all(dir);
    }
}
