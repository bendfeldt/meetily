# onboarding-settings-pane

## 1. Branch + commit
- Branch `fix/onboarding-settings-pane` (based on upstream/devtest 24f4533), pushed to origin (bendfeldt/meetily).
- Commit `e24d027` fix(onboarding): pass preference pane to open_system_settings
- Commit `bd3b320` test(onboarding): cover settings fallback alert in PermissionsStep

## 2. What changed
- `frontend/src/components/onboarding/steps/PermissionsStep.tsx`: the two `invoke('open_system_settings')` calls now pass `{ preferencePane: 'Privacy_Microphone' }` (microphone handler) and `{ preferencePane: 'Privacy_ScreenCapture' }` (system audio handler), matching `PermissionWarning.tsx`.
- `frontend/tests/lib/permissions-step-settings.test.tsx` (new): renders PermissionsStep with react-test-renderer, both permissions mocked as `denied`, mocked `invoke`; calls each row's `onAction` and asserts the invoke args.
  - Before fix: FAIL (calls received with no args object). After fix: pass.
  - Second test (commit bd3b320): invoke rejects -> each button shows its manual-instructions alert (Microphone / Audio Capture). File total: 2 pass, 0 fail.
- `pnpm exec tsc --noEmit`: exit 0. All other frontend test files (bun per file + node --test .mjs) pass.
- Rust side: `open_system_settings(preference_pane: String)` exists only under `#[cfg(target_os = "macos")]` (utils.rs:118-121 on devtest) and is registered only on macOS (lib.rs:945-946). No non-macOS variant; there the invoke fails and the existing alert fallback shows, unchanged. No Rust changes.

## 3. Issue
**ISSUE TITLE:** Onboarding "Open settings" for denied permissions always fails: open_system_settings called without preferencePane

**ISSUE BODY:**
### Description
In the onboarding Permissions step, when the microphone or system audio permission is denied, clicking the row's action is supposed to open macOS System Settings. The frontend calls `invoke('open_system_settings')` with no arguments, but the macOS command requires `preference_pane: String`. Tauri rejects the call for the missing argument, so the catch block runs and the user only gets an `alert()` with manual instructions.

### Steps to reproduce
1. On macOS, run onboarding up to the Permissions step.
2. Deny microphone (or system audio) access so the row shows the denied state.
3. Click the row's action button.

### Expected
System Settings opens on Privacy & Security > Microphone (or Screen & System Audio Recording), as `PermissionWarning` does.

### Actual
System Settings does not open; an alert asks the user to navigate there manually.

### Affected code (devtest 24f4533)
- `frontend/src/components/onboarding/steps/PermissionsStep.tsx:33` and `:65`: `await invoke('open_system_settings');`
- `frontend/src-tauri/src/utils.rs:121`: `pub async fn open_system_settings(preference_pane: String)`
- Correct usage for reference: `frontend/src/components/PermissionWarning.tsx:37`, `:47`

## 4. PR
**PR TITLE:** fix(onboarding): pass preference pane to open_system_settings

**PR BODY:**
## Description
`PermissionsStep` called `open_system_settings` without the required `preferencePane` argument, so the "open settings" action after a denied permission always failed and fell back to an alert. This passes `Privacy_Microphone` for the microphone row and `Privacy_ScreenCapture` for the system audio row, the same panes `PermissionWarning` already uses. Adds a small Bun test that clicks both actions with a mocked `invoke` and checks the arguments.

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

New test `frontend/tests/lib/permissions-step-settings.test.tsx` fails before the fix and passes after (`bun test tests/lib/permissions-step-settings.test.tsx`). `pnpm exec tsc --noEmit` passes and the other frontend test files pass. A second test covers the fallback: when the invoke rejects, each button shows its manual-instructions alert. Run on Linux only; no manual test on macOS (System Settings opening was not verified live).

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
The command only exists on macOS; on other platforms the invoke still fails and the existing alert fallback is shown, as before.

Pane choice for system audio: the backend uses Core Audio taps (NSAudioCaptureUsageDescription). On macOS 14.4+/15 that permission is listed as "System Audio Recording Only" under Privacy & Security > Screen & System Audio Recording ([Apple Developer Forums 771864](https://developer.apple.com/forums/thread/771864)), which is the pane `Privacy_ScreenCapture` opens. I found no documented `Privacy_AudioCapture` anchor. Not verified on a Mac.

## 5. Gaps / risk
- No live macOS check. Reviewer asked about Audio Capture vs ScreenCapture. Research: Apple Developer Forums thread 771864 places "System Audio Recording Only" (the Core Audio tap permission) under Privacy & Security > Screen & System Audio Recording, i.e. the Privacy_ScreenCapture pane. Searches found no source for a `Privacy_AudioCapture` anchor (GitHub code search was blocked in this session; recall.ai/mbsplugins pages unreachable). Evidence is moderate: one forum source plus Apple Support's "Control access to screen and system audio recording" page, no Mac test. The step's alert text says "Audio Capture", which may not match the pane name exactly (left unchanged, out of scope).
- frontend/node_modules in the worktree may be a symlink to the main checkout's (gitignored, not committed).
