//! End-to-end tests (test builds only). See docs/TESTING.md.
//!
//! - `fake_lcu`: a fake League client (LCU) HTTP server with in-memory rune
//!   pages / item sets / champ select that records every request.
//! - `scenarios`: the real background watcher (`watcher::Watcher::tick`) and
//!   the real `import_build` command, driven against the fake client with a
//!   mock Tauri app and offline u.gg / Data Dragon fixtures.
//! - `live_sweep`: `#[ignore]`d sweep over every champion against live u.gg.

mod fake_lcu;
mod live_sweep;
mod scenarios;
