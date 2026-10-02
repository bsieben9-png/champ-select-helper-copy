#!/bin/bash
# SessionStart hook: gets a Claude Code *cloud* session (Linux container)
# ready to build and test Champ Select Helper straight away.
#
#   1. Linux system libraries Tauri needs to compile (WebKitGTK, GTK, ...)
#   2. npm packages  (npm ci, skipped if package-lock.json is unchanged)
#   3. Rust crates   (cargo fetch, so builds don't wait on downloads)
#
# On your Windows PC this does nothing: it only runs when CLAUDE_CODE_REMOTE=true.
# Safe to run any number of times; re-runs take about a second.
#
# Whatever this prints on stdout is shown to Claude, so progress goes to
# stderr and only a short summary goes to stdout.
set -euo pipefail

if [ "${CLAUDE_CODE_REMOTE:-}" != "true" ]; then
  exit 0
fi

ROOT="${CLAUDE_PROJECT_DIR:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
cd "$ROOT"

log() { echo "[session-start] $*" >&2; }
warnings=()

# --- 1. System libraries for Tauri on Linux ---------------------------------
APT_PKGS=(
  libwebkit2gtk-4.1-dev
  libgtk-3-dev
  librsvg2-dev
  libayatana-appindicator3-dev
  build-essential
  pkg-config
)
if command -v pkg-config >/dev/null 2>&1 && pkg-config --exists webkit2gtk-4.1; then
  log "Tauri system libraries already installed"
elif command -v apt-get >/dev/null 2>&1; then
  log "Installing Tauri system libraries (apt)..."
  SUDO=""
  if [ "$(id -u)" -ne 0 ]; then SUDO="sudo"; fi
  export DEBIAN_FRONTEND=noninteractive
  if $SUDO apt-get update -qq >/dev/null 2>&1 \
    && $SUDO apt-get install -y -qq --no-install-recommends "${APT_PKGS[@]}" >/dev/null 2>&1; then
    log "System libraries installed"
  else
    warnings+=("apt install of Tauri libraries failed: Rust builds/tests may not compile")
  fi
else
  warnings+=("apt-get not found: install WebKitGTK 4.1 dev libraries manually")
fi

# --- 2. npm packages ---------------------------------------------------------
# npm ci gives exactly what package-lock.json says (and never edits it).
# A stamp file stores the lockfile's hash so re-runs skip the reinstall.
STAMP="node_modules/.csh-lockfile.sha256"
if [ ! -f package-lock.json ]; then
  log "No package-lock.json: running npm install..."
  npm install --no-audit --no-fund --loglevel=error >&2 \
    || warnings+=("npm install failed: run it again to see the error")
elif [ -f "$STAMP" ] && [ "$(cat "$STAMP")" = "$(sha256sum package-lock.json | cut -d' ' -f1)" ]; then
  log "npm packages up to date"
else
  lock_hash="$(sha256sum package-lock.json | cut -d' ' -f1)"
  log "Installing npm packages (npm ci)..."
  if npm ci --no-audit --no-fund --loglevel=error >&2; then
    echo "$lock_hash" > "$STAMP"
  else
    warnings+=("npm ci failed: run 'npm install' to see the error")
  fi
fi

# --- 3. Rust crates ----------------------------------------------------------
if command -v cargo >/dev/null 2>&1; then
  log "Fetching Rust crates (cargo fetch)..."
  if ! (cd src-tauri && cargo fetch --quiet >&2); then
    warnings+=("cargo fetch failed: check network / Cargo.toml")
  fi
else
  warnings+=("cargo not found: install Rust with rustup (https://rustup.rs)")
fi

# --- Summary (stdout -> Claude's context) -----------------------------------
if [ ${#warnings[@]} -eq 0 ]; then
  echo "Cloud setup OK: Tauri Linux libs, node_modules and Rust crates are ready. Try 'npm run check', 'npm run build', and 'cargo test' in src-tauri."
else
  echo "Cloud setup finished with warnings:"
  printf -- '- %s\n' "${warnings[@]}"
fi
exit 0
