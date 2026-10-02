//! Live data sweep against u.gg + Data Dragon (needs the internet; ~1–2k
//! requests, a few minutes). Run it by hand:
//!
//! ```text
//! cd src-tauri
//! cargo test live_sweep -- --ignored --nocapture
//! ```
//!
//! For EVERY Data Dragon champion: the general build, then for each role u.gg
//! has: the role build, counters and matchups. Plus every role's tier list,
//! ~30 random matchup builds and ~20 ARAM / ARAM Mayhem builds. Everything is
//! checked against Data Dragon (rune trees and rows, shards, spells, items,
//! champions) and for sane numbers. Every problem is printed with the
//! champion/role; the test fails if there is any.
//!
//! Environment (all optional):
//! - `CSH_SWEEP_CACHE`: cache dir (default `<tmp>/csh-live-sweep`; reused, so
//!   a second run is fast and offline-friendly)
//! - `CSH_SWEEP_LIMIT`: only the first N champions (quick runs)
//! - `CSH_SWEEP_CONCURRENCY`: parallel champions (default 4 — be polite)
//! - `CSH_SWEEP_SEED`: seed for the random matchup / ARAM picks

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use crate::ddragon::DDragon;
use crate::model::*;
use crate::ugg::Ugg;

const RANDOM_MATCHUP_BUILDS: usize = 30;
const RANDOM_ARAM_BUILDS: usize = 20;
/// Stat shard rows as in the client (offense, flex, defense).
const SHARD_ROWS: [&[u32]; 3] = [
    &[5008, 5005, 5007],
    &[5008, 5010, 5001],
    &[5011, 5013, 5001],
];

fn env_usize(key: &str) -> Option<usize> {
    std::env::var(key).ok()?.parse().ok()
}

/// Tiny deterministic PRNG (xorshift64*), so a failing run can be repeated.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

/// Lookup sets built from Data Dragon.
struct Known {
    champions: HashMap<u32, String>,
    items: HashSet<u32>,
    spells: HashSet<u32>,
    shards: HashSet<u32>,
    /// style id → rows of perk ids (row 0 = keystones).
    styles: HashMap<u32, Vec<Vec<u32>>>,
}

impl Known {
    fn new(sd: &StaticData) -> Known {
        Known {
            champions: sd
                .champions
                .iter()
                .map(|c| (c.id, c.name.clone()))
                .collect(),
            items: sd.items.iter().map(|i| i.id).collect(),
            spells: sd.spells.iter().map(|s| s.id).collect(),
            shards: sd.shards.iter().map(|s| s.id).collect(),
            styles: sd
                .rune_styles
                .iter()
                .map(|s| {
                    let rows = s
                        .slots
                        .iter()
                        .map(|row| row.iter().map(|r| r.id).collect())
                        .collect();
                    (s.id, rows)
                })
                .collect(),
        }
    }

    fn name(&self, id: u32) -> String {
        self.champions
            .get(&id)
            .cloned()
            .unwrap_or_else(|| format!("#{id}"))
    }
}

fn rate_ok(x: f64) -> bool {
    x.is_finite() && (0.0..=1.0).contains(&x)
}

/// Problems with one build (empty = fine).
fn check_build(b: &Build, k: &Known) -> Vec<String> {
    let mut p = Vec::new();
    let mut rate = |what: &str, x: f64| {
        if !rate_ok(x) {
            p.push(format!("{what} win rate {x} not in [0,1]"));
        }
    };
    rate("build", b.win_rate);
    rate("runes", b.runes.win_rate);
    rate("spells", b.spells.win_rate);
    rate("starting items", b.starting_items.win_rate);
    rate("core items", b.core_items.win_rate);
    for o in b
        .fourth_items
        .iter()
        .chain(&b.fifth_items)
        .chain(&b.sixth_items)
    {
        rate(&format!("item option {}", o.item_id), o.win_rate);
    }
    if b.games == 0 {
        p.push("0 games".into());
    }
    if b.patch.is_empty() || b.rank.is_empty() || b.region.is_empty() {
        p.push(format!(
            "empty patch/rank/region ({:?}/{:?}/{:?})",
            b.patch, b.rank, b.region
        ));
    }

    // Runes.
    let r = &b.runes;
    if r.perks.len() != 6 {
        p.push(format!(
            "{} perks instead of 6: {:?}",
            r.perks.len(),
            r.perks
        ));
    }
    if r.shards.len() != 3 {
        p.push(format!(
            "{} shards instead of 3: {:?}",
            r.shards.len(),
            r.shards
        ));
    }
    match (k.styles.get(&r.primary_style), k.styles.get(&r.sub_style)) {
        (Some(primary), Some(sub)) if r.primary_style != r.sub_style => {
            let row_in =
                |rows: &Vec<Vec<u32>>, perk: u32| rows.iter().position(|row| row.contains(&perk));
            let mut primary_rows = Vec::new();
            let mut sub_rows = Vec::new();
            for &perk in &r.perks {
                match (row_in(primary, perk), row_in(sub, perk)) {
                    (Some(row), _) => primary_rows.push(row),
                    (None, Some(row)) => sub_rows.push(row),
                    (None, None) => p.push(format!(
                        "perk {perk} is in neither style {} nor {}",
                        r.primary_style, r.sub_style
                    )),
                }
            }
            primary_rows.sort_unstable();
            if primary_rows != [0, 1, 2, 3] {
                p.push(format!(
                    "primary tree rows {primary_rows:?} (want keystone + one per row) in {:?}",
                    r.perks
                ));
            }
            sub_rows.sort_unstable();
            if sub_rows.len() != 2 || sub_rows.contains(&0) || sub_rows[0] == sub_rows[1] {
                p.push(format!(
                    "secondary tree rows {sub_rows:?} (want 2 different non-keystone rows) in {:?}",
                    r.perks
                ));
            }
        }
        _ => p.push(format!(
            "bad rune styles {} / {}",
            r.primary_style, r.sub_style
        )),
    }
    for (row, shard) in r.shards.iter().enumerate() {
        if !k.shards.contains(shard) {
            p.push(format!("unknown stat shard {shard}"));
        } else if !SHARD_ROWS.get(row).is_some_and(|ids| ids.contains(shard)) {
            p.push(format!(
                "stat shard {shard} isn't valid in shard row {row}: {:?}",
                r.shards
            ));
        }
    }

    // Spells (recommendation only, but they must exist).
    let [s1, s2] = b.spells.ids;
    for s in [s1, s2] {
        if !k.spells.contains(&s) {
            p.push(format!("unknown summoner spell {s}"));
        }
    }
    if s1 == s2 {
        p.push(format!("same spell twice ({s1})"));
    }

    // Items.
    let groups: [(&str, Vec<u32>); 5] = [
        ("starting", b.starting_items.items.clone()),
        ("core", b.core_items.items.clone()),
        ("4th", b.fourth_items.iter().map(|o| o.item_id).collect()),
        ("5th", b.fifth_items.iter().map(|o| o.item_id).collect()),
        ("6th", b.sixth_items.iter().map(|o| o.item_id).collect()),
    ];
    for (what, ids) in &groups {
        for id in ids {
            if !k.items.contains(id) {
                p.push(format!("{what} item {id} not in Data Dragon"));
            }
        }
    }
    if b.starting_items.items.is_empty() {
        p.push("no starting items".into());
    }
    if b.core_items.items.is_empty() {
        p.push("no core items".into());
    }

    // Skills.
    if b.skill_order.is_empty() {
        p.push("empty skill order".into());
    }
    if let Some(bad) = b
        .skill_order
        .iter()
        .find(|s| !matches!(s.as_str(), "Q" | "W" | "E" | "R"))
    {
        p.push(format!("bad skill letter {bad:?}"));
    }
    if b.skill_order.len() > 18 {
        p.push(format!("skill order has {} levels", b.skill_order.len()));
    }
    let prio: Vec<char> = b.skill_priority.chars().collect();
    let unique: HashSet<&char> = prio.iter().collect();
    if prio.is_empty()
        || unique.len() != prio.len()
        || prio.iter().any(|c| !matches!(c, 'Q' | 'W' | 'E' | 'R'))
    {
        p.push(format!("bad skill priority {:?}", b.skill_priority));
    }

    // Augments (ARAM Mayhem only).
    if b.queue != Queue::AramMayhem && !b.augments.is_empty() {
        p.push(format!("{} augments outside ARAM Mayhem", b.augments.len()));
    }
    let mut seen = HashSet::new();
    for a in &b.augments {
        if a.name.trim().is_empty() || a.icon.is_empty() {
            p.push(format!("augment {} has no name/icon", a.id));
        }
        if !matches!(a.rarity.as_str(), "prismatic" | "gold" | "silver") {
            p.push(format!("augment {} has rarity {:?}", a.id, a.rarity));
        }
        if !seen.insert(a.id) {
            p.push(format!("augment {} listed twice", a.id));
        }
        if !rate_ok(a.win_rate) || !rate_ok(a.pick_rate) {
            p.push(format!("augment {} rates out of range", a.id));
        }
    }
    p
}

/// Are the perks already in slot order (primary rows 0-3, then sub rows)?
fn perks_in_slot_order(r: &RunePage, k: &Known) -> bool {
    let (Some(primary), Some(sub)) = (k.styles.get(&r.primary_style), k.styles.get(&r.sub_style))
    else {
        return true;
    };
    let key = |perk: &u32| {
        let row_in = |rows: &Vec<Vec<u32>>| rows.iter().position(|row| row.contains(perk));
        row_in(primary)
            .map(|row| (0, row))
            .or_else(|| row_in(sub).map(|row| (1, row)))
    };
    let keys: Vec<_> = r.perks.iter().map(key).collect();
    keys.windows(2).all(|w| w[0] <= w[1])
}

fn check_counters(enemy: u32, list: &[Counter], k: &Known) -> Vec<String> {
    let mut p = Vec::new();
    let mut seen = HashSet::new();
    for c in list {
        if !k.champions.contains_key(&c.champion_id) {
            p.push(format!("counter {} not in Data Dragon", c.champion_id));
        }
        if c.champion_id == enemy {
            p.push("counter is the enemy itself".into());
        }
        if !seen.insert(c.champion_id) {
            p.push(format!("counter {} listed twice", c.champion_id));
        }
        if !rate_ok(c.win_rate) {
            p.push(format!("counter {} win rate {}", c.champion_id, c.win_rate));
        }
        if c.games < crate::ugg::COUNTERS_RELAX_FLOOR {
            p.push(format!(
                "counter {} has only {} games",
                c.champion_id, c.games
            ));
        }
    }
    if list.windows(2).any(|w| w[0].win_rate < w[1].win_rate) {
        p.push("counters not sorted by win rate".into());
    }
    if list.len() > crate::ugg::COUNTERS_MAX {
        p.push(format!(
            "{} counters (max {})",
            list.len(),
            crate::ugg::COUNTERS_MAX
        ));
    }
    p
}

fn check_matchups(list: &[MatchupStat], k: &Known) -> Vec<String> {
    let mut p = Vec::new();
    let mut seen = HashSet::new();
    for m in list {
        if !k.champions.contains_key(&m.opponent_id) {
            p.push(format!("opponent {} not in Data Dragon", m.opponent_id));
        }
        if !seen.insert(m.opponent_id) {
            p.push(format!("opponent {} listed twice", m.opponent_id));
        }
        if m.games == 0 || m.wins > m.games {
            p.push(format!(
                "opponent {}: {} wins / {} games",
                m.opponent_id, m.wins, m.games
            ));
        }
        if !rate_ok(m.win_rate)
            || (m.games > 0 && (m.win_rate - f64::from(m.wins) / f64::from(m.games)).abs() > 1e-9)
        {
            p.push(format!(
                "opponent {}: win rate {}",
                m.opponent_id, m.win_rate
            ));
        }
    }
    p
}

fn check_tier_list(list: &[TierEntry], k: &Known) -> Vec<String> {
    let mut p = Vec::new();
    if list.is_empty() {
        p.push("empty tier list".into());
    }
    let mut seen = HashSet::new();
    for t in list {
        if !k.champions.contains_key(&t.champion_id) {
            p.push(format!("champion {} not in Data Dragon", t.champion_id));
        }
        if !seen.insert(t.champion_id) {
            p.push(format!("champion {} listed twice", t.champion_id));
        }
        for (what, x) in [
            ("win", t.win_rate),
            ("pick", t.pick_rate),
            ("ban", t.ban_rate),
        ] {
            if !rate_ok(x) {
                p.push(format!("champion {} {what} rate {x}", t.champion_id));
            }
        }
        if t.games == 0 {
            p.push(format!("champion {} has 0 games", t.champion_id));
        }
    }
    if list.windows(2).any(|w| w[0].win_rate < w[1].win_rate) {
        p.push("tier list not sorted by win rate".into());
    }
    p
}

/// (champion, role, its matchups) — for picking random matchup builds.
type RoleMatchups = (u32, Role, Vec<MatchupStat>);

#[derive(Default)]
struct Report {
    failures: Vec<String>,
    builds: usize,
    counters: usize,
    matchups: usize,
    tier_lists: usize,
    /// Role builds where u.gg's role (`Build.role`) != the requested role.
    role_mismatch: usize,
    fell_back: usize,
    /// Builds whose 6 perks u.gg lists out of the client's slot order (the
    /// import reorders them; informational).
    perks_out_of_slot_order: usize,
    /// Role builds from fewer than `GENERAL_MIN_GAMES` games at every
    /// rank/region level (off-roles; informational).
    thin_role_builds: Vec<String>,
    /// (champion, role) with no counters / no matchups (small samples).
    empty_counters: Vec<String>,
    empty_matchups: Vec<String>,
    /// Champions u.gg has no SR build for at all.
    no_data: Vec<String>,
}

impl Report {
    fn fail(&mut self, ctx: &str, problems: Vec<String>) {
        for problem in problems {
            self.failures.push(format!("{ctx}: {problem}"));
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "live: sweeps every champion on u.gg (network, minutes); run by hand"]
async fn live_sweep() {
    let started = Instant::now();
    let cache = std::env::var_os("CSH_SWEEP_CACHE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("csh-live-sweep"));
    let ugg = Arc::new(Ugg::new(cache.join("ugg")));
    let ddragon = DDragon::new(cache.join("ddragon"));
    let settings = Arc::new(Settings::auto_on());
    let patch = ugg.latest_patch().await.expect("u.gg versions");
    let roles = ugg.primary_roles().await.expect("u.gg primary roles");
    let sd = ddragon
        .static_data(&patch, &roles)
        .await
        .expect("Data Dragon static data");
    let known = Arc::new(Known::new(&sd));
    let mut champions: Vec<u32> = sd.champions.iter().map(|c| c.id).collect();
    if let Some(limit) = env_usize("CSH_SWEEP_LIMIT") {
        champions.truncate(limit);
    }
    let seed = env_usize("CSH_SWEEP_SEED").map_or_else(
        || {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64
                | 1
        },
        |s| s as u64 | 1,
    );
    println!(
        "live sweep: u.gg patch {patch}, Data Dragon {} ({} champions, {} items), seed {seed}, cache {}",
        sd.version,
        sd.champions.len(),
        sd.items.len(),
        cache.display()
    );
    let report = Arc::new(Mutex::new(Report::default()));

    // u.gg role data vs Data Dragon.
    {
        let mut r = report.lock().unwrap();
        for id in roles.keys() {
            if !known.champions.contains_key(id) {
                r.failures
                    .push(format!("primary_roles: champion {id} not in Data Dragon"));
            }
        }
        for c in &sd.champions {
            if c.roles.is_empty() {
                r.failures
                    .push(format!("{} ({}): no u.gg primary roles", c.name, c.id));
            }
        }
    }

    // Tier lists: every role + ARAM.
    for role in Role::ALL {
        let ctx = format!("tier list {role:?}");
        match ugg.tier_list(role, Queue::RankedSolo, &settings).await {
            Ok(list) => {
                let mut r = report.lock().unwrap();
                r.tier_lists += 1;
                let problems = check_tier_list(&list, &known);
                r.fail(&ctx, problems);
            }
            Err(e) => report
                .lock()
                .unwrap()
                .failures
                .push(format!("{ctx}: ERROR {e:#}")),
        }
    }
    match ugg.tier_list(Role::Top, Queue::Aram, &settings).await {
        Ok(list) => {
            let mut r = report.lock().unwrap();
            r.tier_lists += 1;
            let problems = check_tier_list(&list, &known);
            r.fail("tier list ARAM", problems);
        }
        Err(e) => report
            .lock()
            .unwrap()
            .failures
            .push(format!("tier list ARAM: ERROR {e:#}")),
    }

    // Every champion × every role it has.
    let concurrency = env_usize("CSH_SWEEP_CONCURRENCY").unwrap_or(4).max(1);
    let gate = Arc::new(Semaphore::new(concurrency));
    let mut tasks = JoinSet::new();
    let role_table: Arc<Mutex<Vec<RoleMatchups>>> = Arc::default();
    for &champ in &champions {
        let (ugg, settings, known, report, gate, role_table) = (
            ugg.clone(),
            settings.clone(),
            known.clone(),
            report.clone(),
            gate.clone(),
            role_table.clone(),
        );
        tasks.spawn(async move {
            let _permit = gate.acquire_owned().await.unwrap();
            let name = known.name(champ);
            let general = match ugg
                .build(champ, None, None, Queue::RankedSolo, &settings)
                .await
            {
                Ok(b) => b,
                Err(e) => {
                    let mut r = report.lock().unwrap();
                    r.no_data.push(format!("{name} ({champ})"));
                    r.failures
                        .push(format!("{name} ({champ}) general build: ERROR {e:#}"));
                    return;
                }
            };
            {
                let mut r = report.lock().unwrap();
                r.builds += 1;
                if !perks_in_slot_order(&general.runes, &known) {
                    r.perks_out_of_slot_order += 1;
                }
                let mut problems = check_build(&general, &known);
                if general.available_roles.is_empty() {
                    problems.push("no available roles".into());
                }
                if general.role != general.available_roles.first().copied() {
                    problems.push(format!(
                        "general build role {:?} != most played {:?}",
                        general.role,
                        general.available_roles.first()
                    ));
                }
                r.fail(&format!("{name} ({champ}) general"), problems);
            }
            for &role in &general.available_roles {
                let ctx = format!("{name} ({champ}) {role:?}");
                match ugg
                    .build(champ, Some(role), None, Queue::RankedSolo, &settings)
                    .await
                {
                    Ok(b) => {
                        let mut r = report.lock().unwrap();
                        r.builds += 1;
                        if b.games < crate::ugg::GENERAL_MIN_GAMES {
                            r.thin_role_builds
                                .push(format!("{ctx} ({} games)", b.games));
                        }
                        let mut problems = check_build(&b, &known);
                        if b.role != Some(role) {
                            r.role_mismatch += 1;
                            problems.push(format!("asked for {role:?}, got {:?}", b.role));
                        }
                        r.fail(&format!("{ctx} build"), problems);
                    }
                    Err(e) => report
                        .lock()
                        .unwrap()
                        .failures
                        .push(format!("{ctx} build: ERROR {e:#}")),
                }
                match ugg
                    .counters(champ, role, Queue::RankedSolo, &settings)
                    .await
                {
                    Ok(list) => {
                        let mut r = report.lock().unwrap();
                        r.counters += 1;
                        if list.is_empty() {
                            r.empty_counters.push(ctx.clone());
                        }
                        let problems = check_counters(champ, &list, &known);
                        r.fail(&format!("{ctx} counters"), problems);
                    }
                    Err(e) => report
                        .lock()
                        .unwrap()
                        .failures
                        .push(format!("{ctx} counters: ERROR {e:#}")),
                }
                match ugg
                    .matchups(champ, role, Queue::RankedSolo, &settings)
                    .await
                {
                    Ok(list) => {
                        let mut r = report.lock().unwrap();
                        r.matchups += 1;
                        if list.is_empty() {
                            r.empty_matchups.push(ctx.clone());
                        }
                        let problems = check_matchups(&list, &known);
                        r.fail(&format!("{ctx} matchups"), problems);
                        drop(r);
                        role_table.lock().unwrap().push((champ, role, list));
                    }
                    Err(e) => report
                        .lock()
                        .unwrap()
                        .failures
                        .push(format!("{ctx} matchups: ERROR {e:#}")),
                }
            }
            eprint!(".");
        });
    }
    while let Some(res) = tasks.join_next().await {
        if let Err(e) = res {
            report
                .lock()
                .unwrap()
                .failures
                .push(format!("champion task panicked: {e}"));
        }
    }
    eprintln!();

    // Random matchup builds: champion/role with an opponent from its matchups.
    let mut rng = Rng(seed);
    let table = std::mem::take(&mut *role_table.lock().unwrap());
    let candidates: Vec<&RoleMatchups> = table.iter().filter(|(_, _, m)| !m.is_empty()).collect();
    let mut matchup_builds = 0;
    let mut matchup_fell_back = 0;
    for _ in 0..RANDOM_MATCHUP_BUILDS.min(candidates.len() * 3) {
        let (champ, role, list) = candidates[rng.below(candidates.len())];
        // Mostly common matchups, sometimes rare ones.
        let top = if rng.below(4) == 0 {
            list.len()
        } else {
            list.len().min(10)
        };
        let opp = list[rng.below(top)].opponent_id;
        let ctx = format!(
            "{} ({champ}) {role:?} vs {} ({opp})",
            known.name(*champ),
            known.name(opp)
        );
        match ugg
            .build(*champ, Some(*role), Some(opp), Queue::RankedSolo, &settings)
            .await
        {
            Ok(b) => {
                matchup_builds += 1;
                let mut problems = check_build(&b, &known);
                if b.opponent_id != Some(opp) {
                    problems.push(format!("opponent_id {:?}", b.opponent_id));
                }
                if b.role != Some(*role) {
                    problems.push(format!("role {:?}", b.role));
                }
                if b.fell_back_to_general {
                    matchup_fell_back += 1;
                } else if b.games < crate::ugg::MATCHUP_MIN_GAMES_OVERALL {
                    problems.push(format!("matchup build with only {} games", b.games));
                }
                let mut r = report.lock().unwrap();
                r.builds += 1;
                if b.fell_back_to_general {
                    r.fell_back += 1;
                }
                r.fail(&format!("{ctx} matchup build"), problems);
            }
            Err(e) => report
                .lock()
                .unwrap()
                .failures
                .push(format!("{ctx} matchup build: ERROR {e:#}")),
        }
    }

    // Random ARAM / ARAM Mayhem builds.
    let mut aram_builds = 0;
    let mut mayhem_with_augments = 0;
    for i in 0..RANDOM_ARAM_BUILDS {
        let champ = champions[rng.below(champions.len())];
        let queue = if i % 2 == 0 {
            Queue::AramMayhem
        } else {
            Queue::Aram
        };
        let ctx = format!("{} ({champ}) {queue:?}", known.name(champ));
        match ugg.build(champ, None, None, queue, &settings).await {
            Ok(b) => {
                aram_builds += 1;
                let mut problems = check_build(&b, &known);
                if b.role.is_some() || b.opponent_id.is_some() {
                    problems.push(format!("role {:?} / opponent {:?}", b.role, b.opponent_id));
                }
                if queue == Queue::AramMayhem {
                    if b.augments.is_empty() {
                        problems.push("no Mayhem augments".into());
                    } else {
                        mayhem_with_augments += 1;
                    }
                }
                let mut r = report.lock().unwrap();
                r.builds += 1;
                r.fail(&ctx, problems);
            }
            Err(e) => report
                .lock()
                .unwrap()
                .failures
                .push(format!("{ctx}: ERROR {e:#}")),
        }
    }

    let r = report.lock().unwrap();
    println!("\n=== live sweep summary ({:.0?}) ===", started.elapsed());
    println!(
        "champions: {} | builds: {} | counters: {} | matchups: {} | tier lists: {}",
        champions.len(),
        r.builds,
        r.counters,
        r.matchups,
        r.tier_lists
    );
    println!(
        "random matchup builds: {matchup_builds} ({matchup_fell_back} fell back to general) | \
         ARAM/Mayhem builds: {aram_builds} ({mayhem_with_augments} Mayhem with augments)"
    );
    println!(
        "general builds whose perks u.gg lists out of slot order: {} of {}",
        r.perks_out_of_slot_order,
        champions.len()
    );
    println!(
        "role builds where u.gg answered another role: {}",
        r.role_mismatch
    );
    println!(
        "role builds from < {} games ({}), e.g. {:?}",
        crate::ugg::GENERAL_MIN_GAMES,
        r.thin_role_builds.len(),
        r.thin_role_builds.iter().take(8).collect::<Vec<_>>()
    );
    println!("champions without u.gg data: {:?}", r.no_data);
    println!(
        "empty counters ({}): {:?}",
        r.empty_counters.len(),
        r.empty_counters
    );
    println!(
        "empty matchups ({}): {:?}",
        r.empty_matchups.len(),
        r.empty_matchups
    );
    println!("failures: {}", r.failures.len());
    let mut by_kind: HashMap<String, usize> = HashMap::new();
    for f in &r.failures {
        let kind = f.rsplit(": ").next().unwrap_or(f);
        let kind: String = kind
            .chars()
            .map(|c| if c.is_ascii_digit() { '#' } else { c })
            .collect();
        *by_kind.entry(kind).or_default() += 1;
    }
    let mut kinds: Vec<_> = by_kind.into_iter().collect();
    kinds.sort_by_key(|k| std::cmp::Reverse(k.1));
    for (kind, n) in &kinds {
        println!("  {n:>4} × {kind}");
    }
    for f in &r.failures {
        println!("FAIL {f}");
    }
    assert!(
        r.failures.is_empty(),
        "{} problems (see above)",
        r.failures.len()
    );
}
