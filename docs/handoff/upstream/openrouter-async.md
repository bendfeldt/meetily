# openrouter-async

## 1. Branch + commit
- Branch `fix/openrouter-async` (based on upstream/devtest 24f4533), pushed to origin (bendfeldt/meetily).
- Commit `bfbdb5f` fix(openrouter): fetch model list asynchronously with a timeout
- Commit `8a47d2a` fix(openrouter): keep 30s budget and unit-test model mapping (review follow-up)

## 2. What changed
- `frontend/src-tauri/src/openrouter/openrouter.rs`: `get_openrouter_models` is now `async fn` using an async `reqwest::Client` built with `connect_timeout(5s)` (fails fast when offline) and `timeout(30s)` (keeps the old effective budget, the reqwest blocking default, since the total timeout also covers the large body download). `.await` on `send()` and `json()`. The mapping moved into a pure `map_models` fn with unchanged behavior. Return type and error strings unchanged; there is one new error path if building the client fails.
- `frontend/src-tauri/Cargo.toml`: removed `"blocking"` from the regular `[dependencies]` reqwest entry (L114). The `[build-dependencies]` reqwest entry (L57) KEEPS `blocking`: `build/onnxruntime.rs:194` uses `reqwest::blocking::Client` from build.rs. macOS target entry (L169) never had it. llama-helper does not depend on reqwest. Cargo.lock unchanged (features are not recorded in the lock; workspace uses resolver 2, so build-dep features no longer leak into the app crate).
- Frontend caller `frontend/src/components/ModelSettingsModal.tsx:494` `invoke('get_openrouter_models')` already awaits a promise; no change needed (sync vs async commands look identical from JS).
- Test: `openrouter::openrouter::tests::map_models_preserves_fallbacks_and_precedence` checks that the name falls back to "Unknown", that top_provider.context_length takes precedence over the model's context_length, and that pricing is copied or None. `cargo test -p meetily --lib -- openrouter` gives 1 passed. Mutation check: changing the "Unknown" fallback makes it FAIL. The network path itself has no test. Also ran `cargo check -p meetily --all-targets` (OK) and `cargo test -p meetily --lib` (252 passed, 0 failed, 2 ignored). Clippy: no warnings on changed lines (only pre-existing `module_inception` on `openrouter/mod.rs`). rustfmt: the file was unformatted upstream; left upstream formatting as is.

## 3. Issue
**Title:** OpenRouter model list fetch blocks the main thread and freezes the app when network is slow/offline

**Description**
`get_openrouter_models` is a synchronous Tauri command that uses `reqwest::blocking::Client` with no explicit timeout. Tauri runs non-async commands on the main thread, so while the HTTP request is pending the entire window is unresponsive.

**Steps to reproduce**
1. Disconnect from the network (or use a very slow / black-holed connection to openrouter.ai).
2. Open Model Settings and select the OpenRouter provider.

**Expected**
UI stays responsive; the model list either loads or shows an error within a few seconds.

**Actual**
The whole app window freezes until reqwest's default request timeout/connection failure resolves (can be tens of seconds on a black-holed network).

**Affected code (devtest)**
- `frontend/src-tauri/src/openrouter/openrouter.rs:3` (`use reqwest::blocking::Client`)
- `frontend/src-tauri/src/openrouter/openrouter.rs:41-73` (`#[command] pub fn get_openrouter_models`)
- `frontend/src/components/ModelSettingsModal.tsx:494` (caller)

## 4. PR
**Title:** fix(openrouter): fetch model list asynchronously with a timeout

**Body:**
## Description
`get_openrouter_models` was a synchronous `#[command]` using `reqwest::blocking::Client` without a timeout. Tauri runs sync commands on the main thread, so opening the OpenRouter settings on a slow or offline network froze the whole window.

This makes the command `async` and uses the async `reqwest::Client` with a 5s connect timeout (fails fast offline) and a 30s total timeout, which keeps the previous effective budget (the reqwest blocking default). The response mapping is extracted into a pure `map_models` fn with a unit test. Return type and output are unchanged, so the frontend `invoke('get_openrouter_models')` call needs no change.

Since this was the only `reqwest::blocking` user in the app crate, the `blocking` feature is dropped from the regular `[dependencies]` reqwest entry. The `[build-dependencies]` entry keeps it because `build/onnxruntime.rs` downloads ONNX Runtime with `reqwest::blocking`. `Cargo.lock` is unchanged.

## Related Issue
Fixes #<issue>

## Type of Change
- [x] Bug fix
- [ ] New feature
- [ ] Documentation update
- [ ] Performance improvement
- [ ] Code refactoring
- [ ] Other (please describe)

## Testing
- [x] Unit tests added/updated
- [ ] Manual testing performed
- [x] All tests pass

Ran on Linux: `cargo check -p meetily --all-targets` and `cargo test -p meetily --lib` (252 passed, 2 ignored, before the new test), plus the new `map_models` unit test. The HTTP path has no test (no network test harness). Not tested live in the app and not tested on macOS/Windows.

## Documentation
- [ ] Documentation updated
- [x] No documentation needed

## Checklist
- [x] Code follows project style
- [x] Self-reviewed the code
- [ ] Added comments for complex code
- [ ] Updated README if needed
- [x] Branch is up to date with devtest
- [x] No merge conflicts

## Screenshots (if applicable)
N/A

## Additional Notes
The 30s total timeout covers the whole request including the body, the same budget the blocking client had by default. The 5s connect timeout only affects establishing the connection.

🤖 Generated with [Claude Code](https://claude.com/claude-code)

https://claude.ai/code/session_014xnK9q3eoz3HD4vE1Kr9Lh

## 5. Risks / not done
- No live/network test, no macOS/Windows build. The total budget stays at 30s, so there is no regression on slow links. An unreachable host now fails after 5s instead of up to 30s.
- `blocking` could not be removed entirely: the build script (`build/onnxruntime.rs`) needs it in `[build-dependencies]`.
- `lib_old_complex.rs` also references the command but is not compiled; untouched.
