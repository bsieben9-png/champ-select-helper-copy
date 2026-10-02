//! Small HTTP cache shared by the u.gg and Data Dragon sources.
//!
//! - One `reqwest::Client` per cache (browser User-Agent, gzip, 15 s timeout).
//! - Disk cache: one file per URL in `dir`; freshness = file modification time.
//! - Memory cache: the *parsed* value per URL (so a 2 MB file is parsed once),
//!   bounded to `max_entries` (least recently used is dropped).
//! - 403/404 = "file doesn't exist" → `Ok(None)` (remembered for a while).
//! - Network/server errors → stale memory or disk copy if there is one.
//! - Concurrent requests for the same URL wait for one download.

use std::any::Any;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant, SystemTime};

use anyhow::{anyhow, bail, Context};
use tokio::sync::Mutex;

pub const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
     (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36";
const TIMEOUT: Duration = Duration::from_secs(15);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(8);
/// How long a 403/404 ("no such file") answer is remembered in memory.
const MISSING_TTL: Duration = Duration::from_secs(60 * 60);
/// After falling back to a stale copy, retry the network after this long.
const STALE_RETRY: Duration = Duration::from_secs(2 * 60);

/// A parser turns the raw response body into the value kept in memory.
pub type Parser<T> = fn(&[u8]) -> anyhow::Result<T>;

#[derive(Clone)]
enum Cached {
    Missing,
    Data(Arc<dyn Any + Send + Sync>),
}

#[derive(Default)]
struct Slot {
    value: Option<Cached>,
    expires: Option<Instant>,
}

impl Slot {
    fn set(&mut self, value: Cached, valid_for: Duration) {
        self.value = Some(value);
        self.expires = Some(Instant::now() + valid_for);
    }

    /// The cached value if it is of type `T` (fresh or not).
    fn get<T: Send + Sync + 'static>(&self) -> Option<Option<Arc<T>>> {
        match self.value.as_ref()? {
            Cached::Missing => Some(None),
            Cached::Data(any) => any.clone().downcast::<T>().ok().map(Some),
        }
    }

    fn is_fresh(&self) -> bool {
        self.expires.is_some_and(|e| Instant::now() < e)
    }
}

/// URL → (slot, last used).
type SlotMap = HashMap<String, (Arc<Mutex<Slot>>, Instant)>;

enum Fetched {
    Body(Vec<u8>),
    Missing,
}

pub struct HttpCache {
    client: reqwest::Client,
    dir: PathBuf,
    max_entries: usize,
    /// Tests: never touch the network; files not on disk count as missing.
    offline: bool,
    slots: StdMutex<SlotMap>,
    /// Tests: URL substrings whose requests hang until un-stalled (a server
    /// that accepts the connection but doesn't answer).
    #[cfg(test)]
    stalled: tokio::sync::watch::Sender<Vec<String>>,
}

impl HttpCache {
    pub fn new(dir: PathBuf, max_entries: usize) -> Self {
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .gzip(true)
            .timeout(TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            .build()
            .unwrap_or_else(|e| {
                eprintln!("http_cache: client build failed ({e}), using defaults");
                reqwest::Client::new()
            });
        HttpCache {
            client,
            dir,
            max_entries: max_entries.max(1),
            offline: false,
            slots: StdMutex::new(HashMap::new()),
            #[cfg(test)]
            stalled: tokio::sync::watch::Sender::new(Vec::new()),
        }
    }

    #[cfg(test)]
    pub fn new_offline(dir: PathBuf, max_entries: usize) -> Self {
        HttpCache {
            offline: true,
            ..HttpCache::new(dir, max_entries)
        }
    }

    /// Tests: make every request whose URL contains `pattern` hang (`true`)
    /// until it is released again (`false`).
    #[cfg(test)]
    pub fn set_stalled(&self, pattern: &str, stalled: bool) {
        self.stalled.send_modify(|patterns| {
            patterns.retain(|p| p != pattern);
            if stalled {
                patterns.push(pattern.to_string());
            }
        });
    }

    #[cfg(test)]
    async fn wait_while_stalled(&self, url: &str) {
        let mut rx = self.stalled.subscribe();
        loop {
            let stalled = rx.borrow_and_update().iter().any(|p| url.contains(p));
            if !stalled || rx.changed().await.is_err() {
                return;
            }
        }
    }

    /// Path of the disk cache file for `url`.
    pub fn path_for(&self, url: &str) -> PathBuf {
        self.dir.join(cache_file_name(url))
    }

    /// GET `url` (JSON or anything `parse` understands), using the caches.
    /// `ttl` is how long a downloaded copy counts as fresh.
    /// `Ok(None)` means the server says the file doesn't exist (403/404).
    pub async fn get<T: Send + Sync + 'static>(
        &self,
        url: &str,
        ttl: Duration,
        parse: Parser<T>,
    ) -> anyhow::Result<Option<Arc<T>>> {
        #[cfg(test)]
        self.wait_while_stalled(url).await;
        let slot = self.slot(url);
        let mut slot = slot.lock().await;

        // 1. Memory.
        if slot.is_fresh() {
            if let Some(v) = slot.get::<T>() {
                return Ok(v);
            }
        }

        // 2. Fresh disk copy.
        let path = self.path_for(url);
        if let Some(age) = file_age(&path).await {
            if age < ttl {
                match read_and_parse(&path, parse).await {
                    Ok(v) => {
                        let v = Arc::new(v);
                        slot.set(Cached::Data(v.clone()), ttl - age);
                        return Ok(Some(v));
                    }
                    Err(e) => eprintln!(
                        "http_cache: ignoring bad cache file {}: {e:#}",
                        path.display()
                    ),
                }
            }
        }

        // 3. Network.
        let err = match self.fetch(url).await {
            Ok(Fetched::Missing) => {
                slot.set(Cached::Missing, MISSING_TTL);
                return Ok(None);
            }
            Ok(Fetched::Body(body)) => match parse_blocking(parse, body).await {
                Ok((v, body)) => {
                    write_atomic(&path, &body).await;
                    let v = Arc::new(v);
                    slot.set(Cached::Data(v.clone()), ttl);
                    return Ok(Some(v));
                }
                Err(e) => e.context(format!("bad response from {url}")),
            },
            Err(e) => e,
        };

        // 4. Network failed: stale memory, then stale disk.
        if let Some(v) = slot.get::<T>() {
            eprintln!("http_cache: {err:#}; using stale copy of {url}");
            let value = slot.value.clone().expect("checked above");
            slot.set(value, STALE_RETRY);
            return Ok(v);
        }
        if path.exists() {
            if let Ok(v) = read_and_parse(&path, parse).await {
                eprintln!("http_cache: {err:#}; using stale disk copy of {url}");
                let v = Arc::new(v);
                slot.set(Cached::Data(v.clone()), STALE_RETRY);
                return Ok(Some(v));
            }
        }
        Err(err)
    }

    async fn fetch(&self, url: &str) -> anyhow::Result<Fetched> {
        if self.offline {
            return Ok(Fetched::Missing);
        }
        let resp = self
            .client
            .get(url)
            .send()
            .await
            .with_context(|| format!("GET {url}"))?;
        let status = resp.status();
        if status == reqwest::StatusCode::FORBIDDEN || status == reqwest::StatusCode::NOT_FOUND {
            return Ok(Fetched::Missing);
        }
        if !status.is_success() {
            bail!("GET {url}: HTTP {status}");
        }
        let body = resp
            .bytes()
            .await
            .with_context(|| format!("reading {url}"))?;
        Ok(Fetched::Body(body.to_vec()))
    }

    /// The memory slot for `url` (created on demand). Drops the least
    /// recently used slot when there are too many.
    fn slot(&self, url: &str) -> Arc<Mutex<Slot>> {
        let mut map = self.slots.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        let entry = map
            .entry(url.to_string())
            .or_insert_with(|| (Arc::new(Mutex::new(Slot::default())), now));
        entry.1 = now;
        let slot = entry.0.clone();
        while map.len() > self.max_entries {
            let oldest = map
                .iter()
                .filter(|(k, _)| k.as_str() != url)
                .min_by_key(|(_, (_, used))| *used)
                .map(|(k, _)| k.clone());
            match oldest {
                Some(k) => map.remove(&k),
                None => break,
            };
        }
        slot
    }

    /// Drop all parsed values from memory (disk copies stay).
    pub fn clear_memory(&self) {
        self.slots.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }

    /// Delete cache files whose name makes `keep` return false. Best effort.
    pub fn prune(&self, keep: impl Fn(&str, Duration) -> bool) {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return;
        };
        let now = SystemTime::now();
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let age = entry
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|m| now.duration_since(m).ok())
                .unwrap_or_default();
            if !keep(&name, age) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
}

/// Windows-safe file name for a URL: scheme/query dropped, `/` → `~`,
/// anything else unusual → `_`. E.g.
/// `https://stats2.u.gg/lol/1.5/overview/16_19/ranked_solo_5x5/83/1.5.0.json`
/// → `stats2.u.gg~lol~1.5~overview~16_19~ranked_solo_5x5~83~1.5.0.json`.
pub fn cache_file_name(url: &str) -> String {
    let s = url.split_once("://").map_or(url, |(_, rest)| rest);
    let s = s.split(['?', '#']).next().unwrap_or(s);
    let mut name: String = s
        .chars()
        .map(|c| match c {
            '/' => '~',
            c if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') => c,
            _ => '_',
        })
        .collect();
    // Windows dislikes trailing dots/spaces.
    while name.ends_with('.') {
        name.pop();
    }
    name
}

async fn file_age(path: &Path) -> Option<Duration> {
    let modified = tokio::fs::metadata(path).await.ok()?.modified().ok()?;
    Some(
        SystemTime::now()
            .duration_since(modified)
            .unwrap_or(Duration::ZERO),
    )
}

async fn read_and_parse<T: Send + 'static>(path: &Path, parse: Parser<T>) -> anyhow::Result<T> {
    let body = tokio::fs::read(path).await?;
    Ok(parse_blocking(parse, body).await?.0)
}

/// Parse off the async worker threads (matchup files are ~2 MB of JSON).
async fn parse_blocking<T: Send + 'static>(
    parse: Parser<T>,
    body: Vec<u8>,
) -> anyhow::Result<(T, Vec<u8>)> {
    tokio::task::spawn_blocking(move || parse(&body).map(|v| (v, body)))
        .await
        .map_err(|e| anyhow!("parser task failed: {e}"))?
}

/// Write via a temp file + rename so a crash never leaves half a file.
async fn write_atomic(path: &Path, body: &[u8]) {
    let result = async {
        if let Some(dir) = path.parent() {
            tokio::fs::create_dir_all(dir).await?;
        }
        let mut tmp = path.as_os_str().to_owned();
        tmp.push(".tmp");
        tokio::fs::write(&tmp, body).await?;
        tokio::fs::rename(&tmp, path).await
    }
    .await;
    if let Err(e) = result {
        eprintln!("http_cache: could not write {}: {e}", path.display());
    }
}

/// `serde_json::from_slice` as a [`Parser`].
pub fn parse_json(body: &[u8]) -> anyhow::Result<serde_json::Value> {
    Ok(serde_json::from_slice(body)?)
}

#[cfg(test)]
pub mod test_util {
    use std::path::PathBuf;

    /// A fresh, empty temp directory for one test.
    pub fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "csh-test-{name}-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    pub fn fixture(name: &str) -> Vec<u8> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name);
        std::fs::read(&path).unwrap_or_else(|e| panic!("fixture {}: {e}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::test_util::temp_dir;
    use super::*;

    fn parse_len(b: &[u8]) -> anyhow::Result<usize> {
        Ok(b.len())
    }

    #[test]
    fn file_names_are_windows_safe() {
        assert_eq!(
            cache_file_name("https://stats2.u.gg/lol/1.5/overview/16_19/ranked_solo_5x5/matchups/83_887/1.5.0.json"),
            "stats2.u.gg~lol~1.5~overview~16_19~ranked_solo_5x5~matchups~83_887~1.5.0.json"
        );
        assert_eq!(cache_file_name("https://a.b/c:d?x=1"), "a.b~c_d");
        let name =
            cache_file_name("https://ddragon.leagueoflegends.com/cdn/16.19.1/data/en_US/item.json");
        assert!(!name.contains(['<', '>', ':', '"', '/', '\\', '|', '?', '*']));
    }

    #[tokio::test]
    async fn fresh_disk_copy_is_used_and_missing_is_none() {
        let dir = temp_dir("http-fresh");
        let cache = HttpCache::new_offline(dir.clone(), 4);
        let url = "https://example.invalid/a/b.json";
        std::fs::write(cache.path_for(url), b"12345").unwrap();
        let v = cache
            .get(url, Duration::from_secs(60), parse_len)
            .await
            .unwrap();
        assert_eq!(v.as_deref(), Some(&5));
        // Served from memory now, even if the file disappears.
        std::fs::remove_file(cache.path_for(url)).unwrap();
        let v = cache
            .get(url, Duration::from_secs(60), parse_len)
            .await
            .unwrap();
        assert_eq!(v.as_deref(), Some(&5));
        // Unknown file (offline cache treats it like a 403).
        let v = cache
            .get(
                "https://example.invalid/missing.json",
                Duration::from_secs(60),
                parse_len,
            )
            .await
            .unwrap();
        assert!(v.is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn stale_disk_copy_is_used_when_network_fails() {
        let dir = temp_dir("http-stale");
        // An online cache pointed at a closed local port: the request fails.
        let cache = HttpCache {
            client: reqwest::Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(3))
                .build()
                .unwrap(),
            ..HttpCache::new(dir.clone(), 4)
        };
        let url = "http://127.0.0.1:9/stale.json";
        let path = cache.path_for(url);
        std::fs::write(&path, b"abc").unwrap();
        let old = SystemTime::now() - Duration::from_secs(48 * 3600);
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(old)
            .unwrap();
        let v = cache
            .get(url, Duration::from_secs(3600), parse_len)
            .await
            .unwrap();
        assert_eq!(v.as_deref(), Some(&3));
        // No cache file at all → error.
        let err = cache
            .get(
                "http://127.0.0.1:9/none.json",
                Duration::from_secs(3600),
                parse_len,
            )
            .await;
        assert!(err.is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn memory_is_bounded() {
        let dir = temp_dir("http-bounded");
        let cache = HttpCache::new_offline(dir.clone(), 2);
        for i in 0..5 {
            let url = format!("https://example.invalid/{i}.json");
            std::fs::write(cache.path_for(&url), b"x").unwrap();
            cache
                .get(&url, Duration::from_secs(60), parse_len)
                .await
                .unwrap();
        }
        assert_eq!(cache.slots.lock().unwrap().len(), 2);
        let _ = std::fs::remove_dir_all(dir);
    }
}
