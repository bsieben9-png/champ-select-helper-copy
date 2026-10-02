//! Efficiency measurements (needs the internet the first time; files are
//! cached in `CSH_BENCH_CACHE`, default `<tmp>/csh-bench`). Run by hand:
//!
//! ```text
//! cd src-tauri
//! cargo test --release efficiency -- --ignored --nocapture --test-threads=1
//! ```
//!
//! Prints: heap kept by the u.gg memory cache after typical lookups (exact
//! numbers from a counting global allocator — test builds only), the heap
//! peak while parsing, time per lookup from the disk cache, the JSON size of
//! every command's answer (what crosses the IPC bridge to the webview),
//! the cost of one League client discovery scan and of one champ select poll.

use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use crate::ddragon::DDragon;
use crate::model::*;
use crate::ugg::Ugg;

// ---------------------------------------------------------------------------
// Counting allocator (test binary only)
// ---------------------------------------------------------------------------

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

struct Counting;

fn add(n: usize) {
    let live = LIVE.fetch_add(n, Ordering::Relaxed) + n;
    PEAK.fetch_max(live, Ordering::Relaxed);
}

fn sub(n: usize) {
    LIVE.fetch_sub(n, Ordering::Relaxed);
}

// SAFETY: forwards every call to the system allocator unchanged; only counts.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            add(layout.size());
        }
        p
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc_zeroed(layout) };
        if !p.is_null() {
            add(layout.size());
        }
        p
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        sub(layout.size());
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let p = unsafe { System.realloc(ptr, layout, new_size) };
        if !p.is_null() {
            sub(layout.size());
            add(new_size);
        }
        p
    }
}

#[global_allocator]
static ALLOC: Counting = Counting;

fn live() -> usize {
    LIVE.load(Ordering::Relaxed)
}

/// Heap delta and peak (above the starting level) of `f`.
async fn measure<T>(f: impl std::future::Future<Output = T>) -> (T, isize, usize, Duration) {
    let before = live();
    PEAK.store(before, Ordering::Relaxed);
    let t0 = Instant::now();
    let out = f.await;
    let elapsed = t0.elapsed();
    let kept = live() as isize - before as isize;
    let peak = PEAK.load(Ordering::Relaxed).saturating_sub(before);
    (out, kept, peak, elapsed)
}

fn kb(n: impl Into<f64>) -> String {
    format!("{:.1} KB", n.into() / 1024.0)
}

fn json_len<T: serde::Serialize>(v: &T) -> usize {
    serde_json::to_vec(v).map_or(0, |b| b.len())
}

// ---------------------------------------------------------------------------

const YORICK: u32 = 83;
const GWEN: u32 = 887;
/// (champion, role, lane opponent) — 10 popular picks across all roles.
const TEN: [(u32, Role, u32); 10] = [
    (83, Role::Top, 887),       // Yorick vs Gwen
    (122, Role::Top, 86),       // Darius vs Garen
    (64, Role::Jungle, 121),    // Lee Sin vs Kha'Zix
    (104, Role::Jungle, 64),    // Graves vs Lee Sin
    (103, Role::Mid, 238),      // Ahri vs Zed
    (99, Role::Mid, 103),       // Lux vs Ahri
    (222, Role::Adc, 145),      // Jinx vs Kai'Sa
    (81, Role::Adc, 222),       // Ezreal vs Jinx
    (412, Role::Support, 89),   // Thresh vs Leona
    (117, Role::Support, 412),  // Lulu vs Thresh
];

fn cache_dir() -> PathBuf {
    std::env::var_os("CSH_BENCH_CACHE")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("csh-bench"))
}

/// Everything one Lookup of a champion asks for (all tabs).
async fn lookup(ugg: &Ugg, s: &Settings, (champ, role, opp): (u32, Role, u32)) -> usize {
    let q = Queue::RankedSolo;
    let mut bytes = 0;
    bytes += json_len(&ugg.build(champ, Some(role), None, q, s).await.unwrap());
    bytes += json_len(&ugg.build(champ, Some(role), Some(opp), q, s).await.unwrap());
    bytes += json_len(&ugg.counters(champ, role, q, s).await.unwrap());
    bytes += json_len(&ugg.matchups(champ, role, q, s).await.unwrap());
    bytes += json_len(&ugg.tier_list(role, q, s).await.unwrap());
    bytes
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "measurements; needs the internet the first time"]
async fn efficiency_report() {
    let dir = cache_dir();
    let s = Settings::default();
    println!("cache dir: {}", dir.display());

    // Fill the disk cache first (network), so the numbers below are the
    // usual "app restarted, files on disk" case.
    {
        let ugg = Ugg::new(dir.join("ugg"));
        for t in TEN {
            lookup(&ugg, &s, t).await;
        }
        ugg.build(YORICK, None, None, Queue::AramMayhem, &s)
            .await
            .unwrap();
    }

    println!("\n== u.gg memory cache (heap kept / peak while loading / time) ==");
    let base = live();
    let (ugg, ..) = measure(async { Ugg::new(dir.join("ugg")) }).await;
    let step = |name: &str, kept: isize, peak: usize, t: Duration| {
        println!(
            "{name:<44} kept {:>10}  peak {:>10}  {:>7.1} ms  (cache total {})",
            kb(kept as f64),
            kb(peak as f64),
            t.as_secs_f64() * 1e3,
            kb((live() - base) as f64)
        );
    };
    let (_, k, p, t) = measure(ugg.latest_patch()).await;
    step("versions file", k, p, t);
    let (roles, k, p, t) = measure(ugg.primary_roles()).await;
    step("primary roles (+ returned copy)", k, p, t);
    let roles = roles.unwrap();
    let (b, k, p, t) =
        measure(ugg.build(YORICK, Some(Role::Top), None, Queue::RankedSolo, &s)).await;
    step("build Yorick top (overview file)", k, p, t);
    let build_json = json_len(&b.unwrap());
    let (b, k, p, t) =
        measure(ugg.build(YORICK, Some(Role::Top), Some(GWEN), Queue::RankedSolo, &s)).await;
    step("build Yorick vs Gwen (+ matchup overview)", k, p, t);
    let matchup_build_json = json_len(&b.unwrap());
    let (c, k, p, t) = measure(ugg.counters(YORICK, Role::Top, Queue::RankedSolo, &s)).await;
    step("counters vs Yorick (matchups file)", k, p, t);
    let counters_json = json_len(&c.unwrap());
    let (m, k, p, t) = measure(ugg.matchups(YORICK, Role::Top, Queue::RankedSolo, &s)).await;
    step("matchups Yorick (same file)", k, p, t);
    let matchups_json = json_len(&m.unwrap());
    let (tl, k, p, t) = measure(ugg.tier_list(Role::Top, Queue::RankedSolo, &s)).await;
    step("tier list top (ranking file)", k, p, t);
    let tier_json = json_len(&tl.unwrap());
    let one = live() - base;
    let (b, k, p, t) =
        measure(ugg.build(YORICK, None, None, Queue::AramMayhem, &s)).await;
    step("Mayhem build Yorick (+ augments)", k, p, t);
    let mayhem_json = json_len(&b.unwrap());
    let (_, k, p, t) = measure(async {
        for t in TEN {
            lookup(&ugg, &s, t).await;
        }
    })
    .await;
    step("10 champions × all Lookup tabs", k, p, t);
    let ten = live() - base;
    let (_, k, p, t) = measure(async {
        for t in TEN {
            lookup(&ugg, &s, t).await;
        }
    })
    .await;
    step("same 10 again (memory hits)", k, p, t);
    println!(
        "=> u.gg cache after 1 champion: {}, after 10 champions: {}",
        kb(one as f64),
        kb(ten as f64)
    );
    drop(ugg);

    println!("\n== Data Dragon static data ==");
    let dd = DDragon::new(dir.join("ddragon"));
    dd.static_data("16_19", &roles).await.unwrap(); // fill the disk cache
    let dd = DDragon::new(dir.join("ddragon"));
    let (sd, k, p, t) = measure(dd.static_data("16_19", &roles)).await;
    let sd = sd.unwrap();
    step("static data (kept = StaticData itself)", k, p, t);
    let item_desc: usize = sd.items.iter().map(|i| i.description.len()).sum();
    println!(
        "items {} · champions {} · rune styles {} · spells {} · item description text {}",
        sd.items.len(),
        sd.champions.len(),
        sd.rune_styles.len(),
        sd.spells.len(),
        kb(item_desc as f64)
    );

    println!("\n== IPC answer sizes (JSON) ==");
    let sizes = [
        ("get_static_data", json_len(&sd)),
        ("  .items", json_len(&sd.items)),
        ("  .champions", json_len(&sd.champions)),
        ("  .rune_styles", json_len(&sd.rune_styles)),
        ("  .spells", json_len(&sd.spells)),
        ("get_build (general)", build_json),
        ("get_build (matchup)", matchup_build_json),
        ("get_build (Mayhem)", mayhem_json),
        ("get_counters", counters_json),
        ("get_matchups", matchups_json),
        ("get_tier_list", tier_json),
    ];
    for (name, n) in sizes {
        println!("{name:<24} {:>10}", kb(n as f64));
    }

    println!("\n== League client polling ==");
    let n = 50;
    let t0 = Instant::now();
    for _ in 0..n {
        let _ = crate::lcu::LcuClient::discover();
    }
    let procs = std::fs::read_dir("/proc")
        .map(|d| {
            d.flatten()
                .filter(|e| e.file_name().to_string_lossy().bytes().all(|b| b.is_ascii_digit()))
                .count()
        })
        .unwrap_or(0);
    println!(
        "discovery scan (League not running): {:.2} ms each ({procs} processes)",
        t0.elapsed().as_secs_f64() * 1e3 / n as f64
    );
    let session = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/lcu/champ_select_ranked.json"),
    )
    .unwrap();
    let roles: HashMap<u32, Vec<Role>> = roles;
    let n = 2000;
    let t0 = Instant::now();
    for _ in 0..n {
        let v: serde_json::Value = serde_json::from_slice(&session).unwrap();
        std::hint::black_box(crate::lcu::parse_champ_select(&v, Some(420), &roles));
    }
    println!(
        "champ select tick (parse {} session + roles): {:.1} µs",
        kb(session.len() as f64),
        t0.elapsed().as_secs_f64() * 1e6 / n as f64
    );
}
