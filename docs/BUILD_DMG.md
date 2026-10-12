# Building the macOS .dmg (fork)

How to get an installable **Meetily Fork** `.dmg` for Apple Silicon Macs,
either from GitHub Actions or on your own Mac (terminal or VS Code).
Fork-only document.

## Runs next to upstream Meetily

The fork build is a separate app, designed to be installed and used next to
upstream Meetily (not yet tried on a real Mac; CI checks the name and id):

| | Upstream Meetily | Meetily Fork |
|---|---|---|
| App | `/Applications/meetily.app` | `/Applications/Meetily Fork.app` |
| App id | `com.meetily.ai` | `com.meetily.fork` |
| Database, settings, API keys, downloaded models | `~/Library/Application Support/com.meetily.ai` | `~/Library/Application Support/com.meetily.fork` |
| Recordings | `~/Movies/meetily-recordings` | `~/Movies/meetily-fork-recordings` |
| Microphone / screen-recording permission | granted separately | granted separately |
| Updates | upstream releases | none offered (see below) |

Set by `frontend/src-tauri/tauri.fork.conf.json` (name, id, updater) plus a
small startup check that picks the fork's recordings folder whenever the app
id is not upstream's. Still shared between the two apps:

- custom summary templates (`~/Library/Application Support/Meetily/templates`):
  both apps see the same templates;
- notification preferences (`~/Library/Application Support/meetily/notifications.json`):
  changing them in one app changes them in the other (last change wins);
- the in-app developer console filters macOS logs by process name `meetily`,
  so with both apps running it shows both apps' logs.

The fork starts with an empty meeting list; your upstream meetings stay in
upstream Meetily. Each app downloads its own transcription models (a few GB
for large models).

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

Prerequisites (once):

```bash
xcode-select --install                          # Apple build tools
curl https://sh.rustup.rs -sSf | sh             # Rust (restart the terminal after)
rustup target add aarch64-apple-darwin
brew install node@20 cmake                      # or Node 20 from nodejs.org
npm i -g pnpm@9.15.9
```

The script, VS Code tasks and fork config live on `main` once the
`enhance/fork-ci-dmg` PR is merged. Until then, use that branch:
`git fetch origin && git checkout enhance/fork-ci-dmg`.

### B1. In VS Code (recommended)

1. **File → Open Folder…** → your `meetily` clone; make sure it is up to date
   (Source Control → Pull; on `main` after the merge, otherwise on
   `enhance/fork-ci-dmg`).
2. **Cmd+Shift+P → Tasks: Run Task** and pick one of:

| Task | What it does |
|---|---|
| **Meetily Fork: build .dmg (release)** (also **Cmd+Shift+B**) | builds the `.dmg` and shows it in Finder |
| **Meetily Fork: build and install to /Applications** | builds and copies `Meetily Fork.app` to Applications, replacing the old fork build; refuses while Meetily Fork is running |
| **Meetily Fork: build .dmg (debug)** | faster build for testing |
| **Meetily Fork: run in dev mode** | starts the fork with hot reload (Ctrl+C in the terminal to stop); uses port 3118, so only one dev session (fork or upstream) at a time |
| **Meetily Fork: Rust tests** / **UI checks** | the same checks CI runs |

The tasks are in `.vscode/tasks.json` and call `scripts/fork-build-dmg.sh`.

### B2. In a terminal (VS Code's terminal works too)

```bash
cd ~/path/to/meetily
git checkout main && git pull   # or: git checkout enhance/fork-ci-dmg (until it is merged)
scripts/fork-build-dmg.sh              # release .dmg, revealed in Finder
scripts/fork-build-dmg.sh --install    # build + install to /Applications
scripts/fork-build-dmg.sh --debug      # debug build
scripts/fork-build-dmg.sh --dev        # development mode
```

The script runs exactly these steps, if you prefer doing them by hand:

```bash

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

Result: `target/aarch64-apple-darwin/release/bundle/dmg/Meetily Fork_<version>_aarch64.dmg`
(the `target/` folder is at the repository root). The first build takes
15–30 minutes; later builds reuse the cache.

Always pass `--config src-tauri/tauri.fork.conf.json` for fork builds,
also for `pnpm tauri dev`. Without it you build an app with upstream's name
and id that would overwrite and share data with upstream Meetily. It also
turns off updater artifacts (they need upstream's private signing key) and
points the in-app updater at this fork's releases. Those publish no
`latest.json`, so the update check finds nothing and the app never offers to
"update" to upstream Meetily and drop the fork's features. (Upstream's
release build also passes license/Supabase secrets; this tree's code does
not read them, so fork builds don't need them.)

## Installing

1. Open the `.dmg` and drag **Meetily Fork** to **Applications** (it does not
   replace upstream's **meetily**).
2. The build is ad-hoc signed, not notarized, so the first open is blocked:
   - Right-click **Meetily Fork** in Applications → **Open** → **Open**, or
   - System Settings → Privacy & Security → **Open Anyway**, or
   - Terminal: `xattr -dr com.apple.quarantine "/Applications/Meetily Fork.app"`
3. Grant **Microphone** and **Screen & System Audio Recording** when asked
   (separately from upstream Meetily). Rebuilt (re-signed) versions can make
   macOS ask again; if recording is silent after an update, remove and re-add
   Meetily Fork in those two settings.

Both apps can be open at once, but let only one record a given meeting:
both would capture the same microphone, and Teams pop-ups exist only in the fork.

## After installing: live checklist

- [ ] App opens as "Meetily Fork" (Dock, menu bar); upstream Meetily still
      opens and still has its meetings.
- [ ] Manual recording → stop → transcript appears; a silent recording is deleted.
- [ ] Settings → Recording → "Ask to record Teams meetings" on; join a Teams
      call → pop-up within ~2 s (also over a full-screen app) → Record →
      leave the call → recording stops ~5 s later → transcript after the meeting.
- [ ] Obsidian button exports a note; renaming the meeting renames the note.
- [ ] Logs (if something is off): quit the app and start it from Terminal,
      `RUST_LOG=info "/Applications/Meetily Fork.app/Contents/MacOS/meetily"`; the
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
| A build shows up as "meetily" and replaced upstream's app | It was built without `--config src-tauri/tauri.fork.conf.json`; use the script or VS Code task. Reinstall upstream Meetily from its release page. |
| VS Code task: "permission denied" | `chmod +x scripts/fork-build-dmg.sh` |
