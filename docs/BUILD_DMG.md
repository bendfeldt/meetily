# Building the macOS .dmg (fork)

How to get an installable `meetily.dmg` for Apple Silicon Macs from this fork,
either from GitHub Actions (recommended) or on your own Mac. Fork-only document.

## What "verified" means here

| Check | Where | Proves |
|---|---|---|
| `Fork - CI` → `ui` | pushes / PRs that change UI files | TypeScript compiles, frontend unit tests pass, the Next.js UI exports |
| `Fork - CI` → `rust-macos` | every push / PR touching Rust | Rust core (incl. macOS-only Teams detector and pop-up) compiles and its unit tests pass on macOS |
| `Fork - macOS DMG` | on demand / `fork-v*` tag | the full app bundles, sidecars (FFmpeg, llama-helper) are inside, the signature is consistent, the `.dmg` is valid, and the app copied out of the `.dmg` starts and stays up for 30 s |
| You, on your Mac | after installing | recording, transcription, Teams pop-up (see the checklist below) |

CI runners have no microphone and no Teams, so recording, transcription
quality and the Teams pop-up can only be confirmed on a real Mac.

Tip: in GitHub → Settings → Branches (or Rules), require the status check
**Fork CI result** on `main` so nothing merges red.

## Option A: GitHub Actions (recommended)

### One-off build

1. GitHub → **Actions** → **Fork - macOS DMG** → **Run workflow**.
2. Pick the branch (usually `main`) and `release`, then **Run workflow**.
3. Wait ~25–40 min (first run; later runs are cached and faster).
4. Open the run → **Artifacts** → download
   `meetily-fork-<version>-<sha>-macos-arm64-release` (a zip containing the `.dmg`).

With the GitHub CLI on your Mac:

```bash
gh workflow run "Fork - macOS DMG" -R bendfeldt/meetily --ref main
gh run list -R bendfeldt/meetily --workflow "Fork - macOS DMG" --limit 1   # note the run id
gh run watch <run-id> -R bendfeldt/meetily
gh run download <run-id> -R bendfeldt/meetily -D ~/Downloads/meetily-dmg
```

### Release build (keeps the .dmg on a Release page)

Tag a commit on `main` with `fork-v<upstream version>-<n>`:

```bash
git checkout main && git pull
git tag fork-v0.4.1-1
git push origin fork-v0.4.1-1
```

The workflow builds, smoke-tests and creates the GitHub Release
`fork-v0.4.1-1` with the `.dmg` attached
(https://github.com/bendfeldt/meetily/releases).

## Option B: build on your own Mac

Prerequisites (once): Xcode Command Line Tools (`xcode-select --install`),
Rust (`curl https://sh.rustup.rs -sSf | sh`), Node 20, pnpm 9
(`npm i -g pnpm@9.15.9`), and `brew install cmake`.

```bash
git clone https://github.com/bendfeldt/meetily.git && cd meetily
# (or: git checkout main && git pull)

# 1. Built-in summary model sidecar (same as CI)
cargo build --release -p llama-helper --features metal
mkdir -p frontend/src-tauri/binaries
cp target/release/llama-helper frontend/src-tauri/binaries/llama-helper-aarch64-apple-darwin

# 2. UI dependencies
cd frontend
pnpm install --frozen-lockfile

# 3. App + .dmg (runs the Next.js build first; FFmpeg is downloaded by build.rs)
pnpm tauri build --target aarch64-apple-darwin --bundles app,dmg \
  --config src-tauri/tauri.fork.conf.json
```

Result: `target/aarch64-apple-darwin/release/bundle/dmg/meetily_<version>_aarch64.dmg`
(the `target/` folder is at the repository root).

Always pass `--config src-tauri/tauri.fork.conf.json` for fork builds. It
turns off updater artifacts (they need upstream's private signing key) and
points the in-app updater at this fork's releases. Those publish no
`latest.json`, so the update check finds nothing and the app never offers to
"update" to upstream Meetily and drop the fork's features. (Upstream's
release build also passes license/Supabase secrets; this tree's code does
not read them, so fork builds don't need them.)

## Installing

1. Open the `.dmg` and drag **meetily** to **Applications**.
2. The build is ad-hoc signed, not notarized, so the first open is blocked:
   - Right-click **meetily** in Applications → **Open** → **Open**, or
   - System Settings → Privacy & Security → **Open Anyway**, or
   - Terminal: `xattr -dr com.apple.quarantine /Applications/meetily.app`
3. Grant **Microphone** and **Screen & System Audio Recording** when asked.
   Rebuilt (re-signed) versions can make macOS ask again; if recording is
   silent after an update, remove and re-add meetily in those two settings.

Note: the fork keeps upstream's app id (`com.meetily.ai`), so it uses the same
data folder (`~/Library/Application Support/com.meetily.ai`) and settings as
the official Meetily app. Do not run both at once.

## After installing: live checklist

- [ ] App opens; existing meetings are listed.
- [ ] Manual recording → stop → transcript appears; a silent recording is deleted.
- [ ] Settings → Recording → "Ask to record Teams meetings" on; join a Teams
      call → pop-up within ~2 s (also over a full-screen app) → Record →
      leave the call → recording stops ~5 s later → transcript after the meeting.
- [ ] Obsidian button exports a note; renaming the meeting renames the note.
- [ ] Logs (if something is off): quit the app and start it from Terminal,
      `RUST_LOG=info /Applications/meetily.app/Contents/MacOS/meetily`; the
      log prints there (look for `meeting_detector:` lines for Teams).

## Optional: signed and notarized builds

Only needed to share the app without the Gatekeeper prompt. Requires a paid
Apple Developer account and a "Developer ID Application" certificate. Add
repository secrets `APPLE_CERTIFICATE` (base64 .p12),
`APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`,
`APPLE_PASSWORD` (app-specific password) and `APPLE_TEAM_ID`, then pass them
as environment variables to the "Build app and .dmg" step; the Tauri CLI
signs and notarizes when they are set (upstream's `build-macos.yml` shows
the keychain import steps). Not set up in this fork.

## Troubleshooting

| Symptom | Fix |
|---|---|
| CI: `no .dmg or .app produced` | Read the "Build app and .dmg" log; most often a Rust compile error (also visible in `Fork - CI`). |
| CI: smoke test "App exited within 30 s" | The step prints the last 100 log lines; usually a startup panic or missing sidecar. |
| Local: `cmake not found` | `brew install cmake` |
| Local: `failed to bundle project: error running bundle_dmg.sh` | Give Terminal "Automation → Finder" permission once, or build with `--bundles app` and zip the `.app`. |
| App says "damaged and can't be opened" | Quarantine flag; run the `xattr` command above. |
