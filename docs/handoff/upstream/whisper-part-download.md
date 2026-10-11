# whisper-part-download (Finding 18)

## 1. Branch + commit
- Branch: `fix/whisper-part-download` (based on upstream/devtest 24f4533), pushed to origin (bendfeldt/meetily).
- Commits:
  - `40dcb03` fix(whisper): download models to .part and rename after validation
  - `81b7f57` fix(whisper): keep installed model when a re-download fails (follow-up from the independent review)
  - `5caf896` fix(whisper): clean stale .part and keep status of kept model files (final-review edge cases)

## 2. What changed
`frontend/src-tauri/src/whisper_engine/whisper_engine.rs` (only file):
- New helper `partial_download_path(final) -> <final>.part`.
- `download_model_with_owner` now creates `ggml-<name>.bin.part`, not the final file.
- `finish_download` runs the existing header + >=90% size validation on the `.part` file, then `fs::rename`s it to `ggml-<name>.bin` only if both pass. Failure/cancel cleanup now removes the `.part` file and, as before, any file at the final path. `terminal_download_error_releases_the_active_transfer` relies on that existing behavior.
- Discovery: `discover_models` (whisper_engine.rs) and `discover_models_standalone` (commands.rs) both look up exact `WHISPER_MODEL_CATALOG` filenames (`models_dir.join(filename)`), so they already ignore `.part` files. No discovery change was needed.

Test choice: because discovery already ignored `.part`, a "discovery ignores .part" test would have passed before the fix. So the failing test checks the download path instead:
- NEW `interrupted_download_writes_to_part_file_not_final_model_path`: a local HTTP server sends `Content-Length: 8` with 4 body bytes and then stalls. The test asserts the bytes land in `ggml-tiny.bin.part` and `ggml-tiny.bin` does not exist. After cancel, both files are gone.
  - Before the fix it FAILED: `panicked ... in-progress bytes should be written to the .part file: Elapsed(())`, because the bytes went to the final path.
  - After the fix it passes.
- Updated `finish_download_publishes_available_after_releasing_active_owner`, `finish_download_rejects_invalid_header_and_cleans_file` and `late_cancellation_preserves_a_completed_model_state` to stage the completed transfer in the `.part` file. They now also assert the rename or cleanup. No assertions were removed.

Checks:
- `cargo test -p meetily --lib -- whisper_engine`: 21 passed, 0 failed (whisper_engine::whisper_engine: 14 passed).
- `cargo check -p meetily --all-targets`: finished. The only warnings are pre-existing ones.
- `cargo clippy -p meetily --lib`: no warnings on changed lines. Hits in whisper_engine.rs are all pre-existing (lines 155, 233, 242, 257, 547, 1025 `&PathBuf` signature, 1104).
- `rustfmt --edition 2021 --check`: the file was already unformatted upstream (57 hunks before and after). The only remaining hunk near my change is the pre-existing unsorted `use` block that contains the line I edited. The new test is rustfmt-clean.

### Follow-up commit 81b7f57 (review fixes)
- Failure and cancel cleanup now removes **only** the `.part` file. The pre-existing final file is kept, because the attempt never wrote to it. Before this, a failed re-download deleted an installed or user-kept model.
- If the final rename fails, the `.part` is removed, the existing final file is kept, and the error reads "Downloaded model could not be installed: ...".
- `delete_model` also removes a stale `<model>.part`, using the new helper `remove_stale_partial_download`. This cleans up after a crash.
- `terminal_download_error_releases_the_active_transfer` used to assert that the pre-existing final file was deleted on an HTTP 500. It now asserts the file is **kept** and still reported as Corrupted. The commit body explains why.
- New tests: `finish_download_replaces_a_pre_existing_corrupted_model`, `failed_download_keeps_a_pre_existing_installed_model` and `delete_model_removes_a_stale_partial_download`.
  - I did not re-run these against 40dcb03. By construction, the last two fail there: it deleted the final file and did not touch the `.part` file in delete_model.
- `cargo test -p meetily --lib -- whisper_engine`: 24 passed.
- clippy: no new warnings.
- rustfmt: changed lines are clean. The one remaining hunk near my change (`reserve_active_download`) is pre-existing.

### Follow-up commit 5caf896 (final-review edge cases)
- `discover_models` deletes a model's `.part` when no active download owns that model. The check runs under the active-downloads lock. This covers an interrupted fresh download, where only a `.part` exists, the status is Missing and delete_model refuses to act.
- When a failed or cancelled re-download keeps the existing final file, the cached status is re-derived from that file. It uses discovery's own check, now moved into `disk_model_status`, so the model stays Available or Corrupted instead of becoming Missing.
- The cancellation flag is checked again right before `fs::rename`, behind a test-only hook point `rename_reached`/`continue_rename`. If cancelled, the `.part` is removed, the existing final file is kept, and the status comes from the kept file.
- Tests that stage a `.part` now run discovery before staging it, because discovery now removes unowned `.part` files.
- New tests: `discover_models_removes_a_stale_partial_download_without_an_active_owner` and `cancellation_during_finalization_keeps_the_existing_model_file`. `failed_download_keeps_a_pre_existing_installed_model` now also asserts the cached status is Available.
- Checks:
  - `cargo test -p meetily --lib -- whisper_engine`: 26 passed.
  - `cargo check -p meetily --all-targets`: ok.
  - clippy: no new warnings. The cast warnings at L230/L239 are pre-existing code moved into `disk_model_status` unchanged.
  - rustfmt: new lines are clean. The moved discovery block keeps upstream's original (unformatted) style.

## 3. Upstream issue

**ISSUE TITLE:** Interrupted Whisper model download leaves a truncated file that is shown as Available and fails to load

**ISSUE BODY:**

### Description
Whisper model downloads stream directly into the final `ggml-<name>.bin`. If the app quits or crashes during a download, the truncated file stays under the final name. On next start, `discover_models` marks a file as `Available` if it is at least 90% of the catalog size and has a valid GGML header. A download interrupted after 90% therefore shows as installed, but whisper.cpp cannot load it and recording cannot start. Cleanup only runs on in-app failure or cancel, not on quit or crash. (Parakeet already checks exact sizes.)

### Steps to reproduce
1. Start downloading `large-v3` (about 3 GB) in model settings.
2. Quit the app when progress is around 93%.
3. Relaunch the app and open model settings.
4. Select large-v3 and try to start a recording.

### Expected
The incomplete download does not appear as an installed model, and the model can be re-downloaded.

### Actual
large-v3 is listed as Available. Loading it fails, so recording cannot start. The user has to find and delete the file by hand, or use delete-model.

### Affected code (devtest 24f4533)
- `frontend/src-tauri/src/whisper_engine/whisper_engine.rs:1041`: `fs::File::create(file_path)` writes directly to the final model path.
- `frontend/src-tauri/src/whisper_engine/whisper_engine.rs:206-243`: discovery treats a file >= 90% of the expected size with a valid header as `Available`.
- `frontend/src-tauri/src/whisper_engine/whisper_engine.rs:880-915`: cleanup only runs from `finish_download` (in-app failure or cancel).
- `frontend/src-tauri/src/whisper_engine/commands.rs:91-104`: `discover_models_standalone` treats any final file >= 1 MB as `Available`.

## 4. Upstream PR

**PR TITLE:** fix(whisper): download models to .part and rename after validation

**PR BODY:**

## Description
Whisper model downloads now write to `ggml-<name>.bin.part`. Once the transfer finishes, `finish_download` runs the existing GGML header check and minimum-size check on the `.part` file. It renames the file to `ggml-<name>.bin` only when both checks pass. On failure or cancel, only the `.part` file is removed. A model that was already installed, or a corrupted file the user kept, is no longer deleted by a failed re-download. If the final rename fails, the existing file is kept and the error says the downloaded model could not be installed. `delete_model` and `discover_models` also remove a stale `.part` left by a crash, as long as no download owns it. When a re-download fails or is cancelled, the model status is re-derived from the kept file. Cancellation is checked again right before the rename.

As a result, an interrupted download (app quit, crash, power loss) can never leave a truncated file under the final name. Previously such a file was reported as `Available` at the next startup and then failed to load. Both discovery paths (`discover_models` and `discover_models_standalone`) look up exact catalog filenames, so leftover `.part` files are never listed.

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

Added `interrupted_download_writes_to_part_file_not_final_model_path`, `finish_download_replaces_a_pre_existing_corrupted_model`, `failed_download_keeps_a_pre_existing_installed_model`, `delete_model_removes_a_stale_partial_download`, `discover_models_removes_a_stale_partial_download_without_an_active_owner` and `cancellation_during_finalization_keeps_the_existing_model_file`. `terminal_download_error_releases_the_active_transfer` now asserts that the pre-existing final file is kept, because a failed attempt no longer touches it. The first new test uses a local HTTP server that stalls mid-body. It asserts the bytes go to `.part`, not the final path, and that cancel removes them. It fails on devtest and passes with this change. Existing `finish_download` tests now stage the completed transfer in the `.part` file and assert the rename and cleanup. `cargo test -p meetily --lib -- whisper_engine`: 26 passed. Linux unit tests only. I did no live download, recording or macOS/Windows testing.

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
- There is no resume: each attempt restarts the `.part` file from zero, as before.
- A `.part` file left by a crash is never listed as a model. It is removed at the next model scan (unless a download owns it) or by delete-model.
- Partial final-named files left by older versions are still judged by the existing 90%+header check. That is out of scope here.
- `fs::rename` replaces an existing destination on Windows, but that was not tested on Windows.
- Download URLs are unchanged.

## 5. Not done / risks
- No live download, recording, macOS or Windows test was run. Only Linux unit tests ran.
- The rename-failure path has no direct test, because it is hard to trigger portably. It reuses the same `.part`-only cleanup that the tests do cover.
- `discover_models_standalone` still treats any final-named file >= 1 MB as Available. After this fix an interrupted download no longer produces such a file, but a hand-copied truncated file still would. That is a separate, pre-existing weakness and was not changed.
- No fsync before the rename (flush only, same as before).
