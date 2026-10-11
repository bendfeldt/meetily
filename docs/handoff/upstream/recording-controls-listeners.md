# recording-controls-listeners (findings 22 + 23)

## 1. Branch + commit
- Branch `fix/recording-controls-listeners` (based on upstream/devtest 24f4533), pushed to origin (bendfeldt/meetily).
- Commit `a3df438` fix(recording): keep recording on chunk errors, register listeners once
- Commit `8ef7bb6` fix(recording): still stop the UI when the transcription engine fails (review follow-up)

## 2. What changed
- `frontend/src/components/RecordingControls.tsx`: `onRecordingStop` and `onTranscriptionError` are read through refs; the listener effect now has `[]` deps and a `cancelled` guard (each unlisten is stored right after its `await`, and dropped if cleanup already ran), same pattern as `RecordingStateContext.tsx`. `transcription-error` now calls `onRecordingStop(false)` only for `phase === 'active' && actionable` (engine init failure, worker.rs:192-197, after which the worker returns); per-chunk errors (`actionable: false`) only report. The `transcript-error` handler's stop call is removed: no compiled Rust emits that event (only `lib_old_complex.rs`, not a module) and it has no fatal flag. Errors are still reported (`onTranscriptionError` for the legacy event; useModalState's global listener shows the toast / model selector for `transcription-error`).
- `frontend/tests/components/recording-controls-listeners.test.tsx` (new, Bun + react-test-renderer, mocks `@tauri-apps/api/event` `listen`, core, analytics, RecordingStateContext, and the radix tooltip wrapper):
  - `registers each listener once across parent re-renders`
  - `unmount before listen resolves does not leak listeners`
  - `only the latest render callbacks are called` (each render's callbacks write to their own arrays)
  - `a per-chunk transcription error does not stop the recording UI`
  - `a fatal engine-init transcription error still stops the recording UI`
- Results: devtest component 2 pass / 3 fail (once, no-leak, per-chunk). Commit a3df438: 4 pass / 1 fail (fatal case). Stale-closure mutation (refs replaced by the props): 3 pass / 2 fail (latest-callback, fatal). Final: 5 pass / 0 fail. All other bun test files under frontend/tests: 0 fail. `pnpm exec tsc --noEmit`: clean.

"Fatal" variant check: only `actionable: true` with phase `active` marks it (engine init failure, worker.rs:195; every per-chunk emit is `actionable: false`). That case keeps `onRecordingStop(false)`. Original notes: Payload is `{error, userMessage, actionable, phase: 'startup'|'active'}`. `phase: 'active'` is emitted for per-chunk failures (worker.rs ~L436/454/476 on devtest) and for engine-init failure (worker.rs ~L192, `actionable: true`), where only the transcription task returns; audio capture continues. Also `handleRecordingStop` (useRecordingStop.ts) never calls `stop_recording`, so `onRecordingStop(false)` only desynced the UI in every case. `transcript-error` is not emitted by any compiled Rust code on devtest (only `lib_old_complex.rs`, not a module).

## 3. Upstream issue
**ISSUE TITLE:** Recording UI stops/idles on a single transcription error while recording continues; RecordingControls re-registers event listeners every render

**ISSUE BODY:**
### Description
Two related problems in `RecordingControls.tsx`:
1. The `transcription-error` handler calls `onRecordingStop(false)` for `phase === 'active'`, and `transcript-error` always does. Rust emits `phase: "active"` for a single failed chunk and keeps recording. `handleRecordingStop` does not stop the backend, so the UI goes to STOPPING / "Waiting for transcription..." for up to 60 s and then shows idle while Rust is still recording.
2. The listener effect depends on `onRecordingStop` and `onTranscriptionError`, which `page.tsx` passes as new inline arrows every render. The page re-renders every few hundred ms while recording, so the three listeners are removed and re-added constantly. The unlisten functions are stored only after all three `await listen` calls, so a cleanup that runs before they resolve leaks them.

### Steps to reproduce
1. Start a recording.
2. Cause one chunk to fail transcription (e.g. engine error on a chunk, or a provider error).
3. Watch the recording controls.

### Expected
A toast reports the error; the recording keeps running and the Stop button stays available.

### Actual
The controls switch to the stopping/processing state, then go idle after ~60 s while the backend is still recording. Separately, `listen` is called for the three events on every re-render.

### Affected code (devtest)
- `frontend/src/components/RecordingControls.tsx:270` and `:295` (`onRecordingStop(false)`), `:250`/`:311` (unsubscribes filled after all awaits), `:332` (deps)
- `frontend/src/app/page.tsx:231`, `:236` (inline arrow props)
- `frontend/src-tauri/src/audio/transcription/worker.rs:192`, `:436`, `:454`, `:476` (`phase: "active"`, recording continues)
- `frontend/src/hooks/useRecordingStop.ts:121` (`handleRecordingStop` does not call `stop_recording`)

## 4. Upstream PR
**PR TITLE:** fix(recording): keep recording on chunk errors, register listeners once

**PR BODY:**
## Description
`RecordingControls` no longer calls `onRecordingStop(false)` for a per-chunk `transcription-error`. Rust emits those (`phase: "active"`, `actionable: false`) for a single failed chunk and keeps recording, so the old call put the UI into a stopping state and then idle while recording continued. The fatal case is kept: engine init failure (`phase: "active"`, `actionable: true`, worker.rs:192-197, after which the worker returns) still stops the UI. The `transcript-error` handler's stop call is removed too: no compiled Rust emits `transcript-error` (only `lib_old_complex.rs`, which is not a module). Errors are still reported (toast / model selector via `useModalState`, `onTranscriptionError` for the legacy event).

The listener effect is also registered once: `onRecordingStop` and `onTranscriptionError` are read through refs (page.tsx passes new arrows every render), deps are `[]`, and a `cancelled` guard drops listeners that resolve after unmount, matching `RecordingStateContext`.

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

New `frontend/tests/components/recording-controls-listeners.test.tsx` (Bun, react-test-renderer, mocked `listen`): listeners registered once across re-renders, no leak when unmounting before `listen` resolves, only the latest render's callbacks are called, a per-chunk error does not call `onRecordingStop`, and the engine-init error still does. On devtest the once, no-leak and per-chunk tests fail; all 5 pass with this PR. Other Bun test files pass, `pnpm exec tsc --noEmit` is clean. Run on Linux only; no live recording or macOS test was done.

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
Only `actionable: true` distinguishes the fatal engine-init error among `phase: "active"` errors; `onRecordingStop(false)` (as before) does not call `stop_recording`, so that case is unchanged apart from now using the latest callback.

## 5. Not done / risks
- No live recording test (needs desktop session); behavior verified by unit tests only.
- Engine-init failure (worker.rs:192) still calls `onRecordingStop(false)`, which (as before) does not call `stop_recording`; the backend keeps capturing audio. Unchanged pre-existing behaviour; making it really stop is a separate change.
- `onRecordingStop` prop stays in the interface (still used by the Stop button path, L162/L182).
