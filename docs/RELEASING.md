# Making a release

A **release** is a permanent, numbered download on the repository's
**Releases** page (unlike the Actions builds, which expire after 14 days).
The **Release** workflow (`.github/workflows/release.yml`) builds the
Windows installers and attaches them to a **draft** release. You check the
draft and publish it.

## 1. Pick the new version number

Use three numbers, like `0.2.0`. **Numbers only**: the `.msi` installer
can't handle versions like `0.2.0-beta`.

- Small fixes: last number (`0.2.0` → `0.2.1`)
- New features: middle number (`0.2.1` → `0.3.0`)

## 2. Put the version in the code

The same version must be in **three files**:

- `package.json` → `"version"`
- `src-tauri/Cargo.toml` → `version` under `[package]`
- `src-tauri/tauri.conf.json` → `"version"` (the release uses this one)

Easiest: ask Claude Code, *"Bump the version to 0.2.0 everywhere, then
commit and push to main."* (It also updates `src-tauri/Cargo.lock`.)

## 3. Start the release

Do **one** of these:

- **On the GitHub website**: **Actions** tab → **Release** (left list) →
  **Run workflow** → branch **main** → **Run workflow**. The workflow tags
  the latest commit as `v0.2.0` for you.
- **From a terminal** (PC or Claude):
  ```sh
  git pull
  git tag v0.2.0
  git push origin v0.2.0
  ```
  The tag must be `v` + the version from step 2, or the workflow stops and
  tells you so.

Don't use GitHub's **"Draft a new release"** button yourself. The workflow
creates the release, and a release you make by hand gets in its way.

## 4. Publish

The build takes about 15–25 minutes. Then:

1. Open **Releases**. There's a draft called **Champ Select Helper v0.2.0**
   with the installers attached.
2. Click the pencil (**Edit**), add a few words about what changed if you
   like, and click **Publish release**.

## If the workflow fails

Open the failed run (Actions → Release → the run with the red X) and read
the first red step. Common messages:

| Message | Fix |
|---|---|
| *Tag vX doesn't match the app version* | The tag and `tauri.conf.json` disagree. Fix the version (step 2), push, then delete the tag (`git push origin :refs/tags/vX` and `git tag -d vX`) and tag again. |
| *Tag vX already exists* | That version was already released. Bump the version (step 2) first. |
| A build error | Something in the code doesn't compile on Windows. Paste the error into Claude Code. |

## Good to know

- The app isn't code-signed, so Windows SmartScreen warns on first run
  ("More info" → "Run anyway"). Code signing needs a paid certificate.
- GitHub gives private repositories a monthly allowance of free build
  minutes, and Windows builds use it up faster than Linux ones. You can
  check what's left under your GitHub **Settings → Billing**.
