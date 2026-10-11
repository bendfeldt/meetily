# command-hardening (Finding 29)

## 1. Branch + commit
- Branch: `fix/command-hardening` (based on upstream/devtest), pushed to origin (bendfeldt/meetily)
- Commits: `4738bf0` fix(security): drop save_transcript, restrict open_external_url
- `ee393fb` fix(security): parse external URLs and open them via rundll32 (review follow-up)

## 2. What changed
- `frontend/src-tauri/src/lib.rs`: removed the `save_transcript` Tauri command (it wrote any content to any path and created folders) and its `generate_handler!` entry. A grep for the exact command string `save_transcript` in frontend/src, frontend/tests and frontend/public found no callers; the UI uses `api_save_transcript`. (`lib_old_complex.rs` still has a copy, but no `mod` declaration includes it, so it is not compiled. Left alone.)
- `frontend/src-tauri/src/api/api.rs`: added a pure fn `parse_external_url(raw) -> Option<String>`. It rejects input that is over 2048 bytes, contains whitespace, control characters or `"`, or doesn't start with a literal `https://` (case-insensitive). It then parses with the existing `url` crate, requires scheme `https` and a non-empty host, and returns the serialized URL. `open_external_url` launches that serialized URL, or returns `Err("Only https:// URLs can be opened")`.
  - Windows: `rundll32 url.dll,FileProtocolHandler <url>` replaces `cmd /C start <url>`. No shell parsing; the URL goes to ShellExecute. No opener crate (`open`, tauri-plugin-opener, tauri-plugin-shell) is in Cargo.lock, so no dependency was added.
  - macOS `open` and Linux `xdg-open` are unchanged. All 4 UI callers (AnalyticsConsentSwitch.tsx:150, ModelSettingsModal.tsx:739 and 1236, About.tsx:26) pass hardcoded https URLs, so only https is allowed.
- Test: `api::api::tests::external_url_accepts_https_only`.
  - Accepts two https URLs.
  - Accepts `HTTPS://example.com` and normalizes it to `https://example.com/`. Accepting it is a deliberate choice: the scheme is case-insensitive.
  - Rejects `file:///etc/passwd`, `/Applications/Calculator.app`, `javascript:alert(1)`, `http://example.com`, `""`, ` https://example.com` (leading space), `https:example.com`, `https://`, an embedded `"`, an embedded `\n` and input over 2048 bytes.
  - Before each fix: `error[E0432]: unresolved import` of the helper (compile failure).
  - After the fix: `test api::api::tests::external_url_accepts_https_only ... ok`, 1 passed.
- `cargo check -p meetily --all-targets`: Finished, no errors.
- rustfmt: the only remaining diffs in api.rs/lib.rs are on unformatted upstream lines; none are on changed lines.
- clippy (`cargo clippy -p meetily --lib`): no warnings on changed lines in api.rs or lib.rs. The run ends with a pre-existing deny-level error in an unrelated file (`audio/capture/backend_config.rs`: `AudioCaptureBackend::to_string` shadows `Display`), which is upstream and out of scope.

## 3. Issue
**Title:** Tauri commands save_transcript and open_external_url accept arbitrary paths/URLs (Windows command injection)

**Description**
Two Tauri commands that the webview can call accept unchecked input:
1. `save_transcript(file_path, content)` writes any content to any file path and creates missing directories. Nothing in the UI calls it (the UI uses `api_save_transcript`), so it only adds attack surface.
2. `open_external_url(url)` hands any string to the OS opener. On Windows it runs `cmd /C start <url>`, and cmd treats `&` as a command separator. On macOS, `open` also launches local apps and files.

If anything ever runs script in the webview (XSS through rendered transcript/summary markdown, a compromised dependency), these commands turn that into arbitrary file writes and command execution.

**Steps to reproduce**
1. In the dev console of the Windows app: `__TAURI__.core.invoke('open_external_url', { url: 'https://example.com/?a=1&calc' })`
2. In the dev console: `__TAURI__.core.invoke('save_transcript', { filePath: '<any path>', content: 'x' })`
3. On macOS: `invoke('open_external_url', { url: '/System/Applications/Calculator.app' })`

**Expected**
Only https web links open in the browser. No command writes to arbitrary paths.

**Actual**
1. The browser opens and Calculator also starts.
2. The file is written, and missing directories are created.
3. Calculator launches.

**Affected code (devtest)**
- `frontend/src-tauri/src/lib.rs:411` (`save_transcript`), registered at `lib.rs:742`
- `frontend/src-tauri/src/api/api.rs:1159` (`open_external_url`), registered at `lib.rs:861`

## 4. PR
**Title:** fix(security): drop save_transcript, restrict open_external_url

**Body:**

## Description
- Removes the unused `save_transcript` Tauri command (arbitrary file write with directory creation) and its handler registration. No frontend code invokes it; the UI uses `api_save_transcript`.
- All existing callers pass hardcoded https links.
- `open_external_url` validates the URL by parsing it: https only, non-empty host, no whitespace, control characters or quotes, at most 2048 bytes. It launches the parsed, normalized URL.
- On Windows it uses `rundll32 url.dll,FileProtocolHandler <url>` instead of `cmd /C start <url>`. cmd parsed `&` in the URL as a command separator, so `https://x/?a=1&calc` started calc. macOS (`open`) and Linux (`xdg-open`) are unchanged apart from the scheme check.

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
- [x] Unit tests added/updated (`api::api::tests::external_url_accepts_https_only`)
- [ ] Manual testing performed
- [x] All tests pass (new test plus `cargo check --all-targets` on Linux)

Ran on Linux only: the unit test failed before the fix (helper missing) and passes after it. `cargo check -p meetily --all-targets` is clean. **The Windows rundll32 path was not run**, I did not test macOS live, and I did not run a live recording.

## Documentation
- [ ] Documentation updated
- [x] No documentation needed

## Checklist
- [x] Code follows project style
- [x] Self-reviewed the code
- [x] Added comments for complex code
- [ ] Updated README if needed
- [x] Branch is up to date with devtest
- [x] No merge conflicts

## Screenshots (if applicable)
N/A

## Additional Notes
`lib_old_complex.rs` still contains a copy of `save_transcript`, but it is not compiled (no `mod` declaration), so this PR leaves it alone.

## 5. Not done / risks
- The Windows rundll32 path and the macOS paths were not run (Linux only). The macOS CI check workflow was not triggered.
- URLs over 2048 bytes, and any with a literal space or quote, are rejected. Today's callers are short hardcoded links.
- The https-only restriction blocks any future `http://` or `mailto:` caller. None exist today.
- If a third-party plugin or an external script relied on `save_transcript`, it is gone. No in-repo caller exists.
