# delete-meeting-folder (Finding 19)

## 1. Branch + commit
- Branch `fix/delete-meeting-folder` (based on upstream/devtest 24f4533), pushed to origin (bendfeldt/meetily).
- Commit `322e4db` fix(meetings): remove the recording folder when a meeting is deleted
- Commit `8f60624` fix(meetings): harden recording folder removal on delete (review follow-up)
- Commit `c6081e2` fix(meetings): keep folders on case or name matches (final review)

## 2. What changed
- `frontend/src-tauri/src/api/api.rs`
  - New `folder_inside_root(folder, root) -> Option<PathBuf>`: canonicalizes both; accepts only an existing directory strictly inside the root (root itself, outside paths, `..` escapes, symlinks pointing out, missing paths are rejected).
  - New `delete_meeting_and_folder(pool, meeting_id, recordings_root)`: reads `folder_path` and counts other meetings using the same folder BEFORE deleting rows; calls the existing `MeetingsRepository::delete_meeting`; then `remove_dir_all` only if inside root and unshared. Removal failure / skip is logged with `log_warn!` (meeting id + `io::ErrorKind`, no paths) and does not fail the command.
  - `api_delete_meeting` now calls it with `audio::recording_preferences::get_default_recordings_folder()` (the root used by `recording_saver.rs:226` and `import.rs:343` to create meeting folders).
  - Tests (`#[cfg(test)] mod tests`, in-memory SQLite + `sqlx::migrate!`): `only_folders_strictly_inside_the_recordings_root_are_removable`, `delete_meeting_removes_its_folder_unless_shared_or_outside_root`.
- Fail-before: with stubs keeping the old rows-only behaviour both failed (`left: None, right: Some(".../recordings/Meeting 1")`; `own folder must be removed`). After the fix: `test result: ok. 2 passed; 0 failed` (`cargo test -p meetily --lib -- api::api::tests`).
- `cargo check -p meetily --all-targets`: Finished, no errors. `cargo clippy -p meetily --lib`: no warnings on changed lines (existing warnings in api.rs at other lines only). `rustfmt --check`: no diffs in changed lines (remaining diffs are pre-existing upstream formatting).
- UI not changed: the toast "All associated data has been removed" is now true for folders in the recordings root.

### Review follow-up (second commit)
- `folder_inside_root` now returns `Result<PathBuf, String>` with distinct reasons (empty/relative path rejected via `Path::is_absolute` before canonicalize; missing; not resolvable incl. permission error kind; outside the recordings folder; not a directory). Reasons contain no paths.
- Sharing check no longer compares raw strings: new `folder_shared` loads all other meetings' `folder_path`s before the delete, canonicalizes each absolute one (skips unresolvable/relative), and keeps the folder if another equals it, is inside it, or contains it. On macOS/Windows the comparison is also case-insensitive (lowercased).
- Logs: NULL folder_path -> debug; kept folder -> warn with reason (or "used by another meeting"); removal failure -> warn with error kind.
- New tests: `null_empty_or_relative_folder_path_removes_nothing` (NULL, "", whitespace, and a relative path that would canonicalize to a real folder inside the root) and `folder_used_by_another_meeting_under_a_different_spelling_is_kept` (trailing slash, nested folder of another meeting, folder inside another meeting's, symlink alias). Both failed on commit 322e4db ("relative folder_path must not be resolved", "same folder spelled with a trailing slash") and pass now: `4 passed; 0 failed`.

### Final review round (third commit)
- `folder_shared` compares case-insensitively on all platforms (a false match only keeps a folder).
- Another meeting's non-empty `folder_path` that is relative or cannot be resolved keeps the folder when its last component equals the folder's name (case-insensitive); logged as its own reason, no path.
- The relative-path test is `#[cfg(unix)]`.
- New test `case_variants_and_unresolvable_same_name_paths_keep_the_folder` (failed on 8f60624 with "case variant must keep the folder"); 5/5 pass.

## 3. Upstream issue

**ISSUE TITLE:** Deleting a meeting leaves its audio and transcript files on disk

**ISSUE BODY:**

### Description
Deleting a meeting from the sidebar removes only its database rows. The meeting's recording folder (audio file, `transcripts.json`, `metadata.json`) is left in the recordings folder, although the app shows "Meeting deleted successfully - All associated data has been removed". For a privacy-first app this is misleading: users believe the recording is gone.

### Steps to reproduce
1. Record a short meeting (or import an audio file).
2. Note its folder under the recordings folder (e.g. `~/Movies/meetily-recordings/<meeting>` on macOS).
3. Delete the meeting from the sidebar.
4. Look in the recordings folder.

### Expected
The meeting's folder with audio and transcripts is removed (when it belongs only to that meeting and lives in the app's recordings folder).

### Actual
The folder and all files stay on disk; only the DB rows are gone.

### Affected code (devtest)
- `frontend/src-tauri/src/api/api.rs:770-801` `api_delete_meeting` only calls `MeetingsRepository::delete_meeting` (L784).
- `frontend/src-tauri/src/database/repositories/meeting.rs:18-47` and `:233-275` delete rows only.
- `frontend/src/components/Sidebar/index.tsx:341` toast "All associated data has been removed".

## 4. Upstream PR

**PR TITLE:** fix(meetings): remove the recording folder when a meeting is deleted

**PR BODY:**

## Description
`api_delete_meeting` deleted only the database rows; the meeting folder (audio, `transcripts.json`, `metadata.json`) stayed on disk while the UI says all data was removed.

Now the command reads the meeting's `folder_path` (and all other meetings' folder paths) before deleting rows and, after the rows are deleted, removes the folder only if:
- the stored path is absolute (empty, whitespace and relative paths are never resolved);
- its canonical path is strictly inside the canonical recordings folder, so symlinks / `..` cannot escape it and the root itself is never removed;
- no other meeting's folder, after canonicalizing, is the same folder, is inside it, or contains it (catches trailing separators, symlink aliases, nested folders; compared case-insensitively on all platforms, since a false match only keeps a folder);
- no other meeting has a relative or unresolvable folder path ending in the same folder name (kept to be safe).

The root is `get_default_recordings_folder()` because devtest writes all recordings and imports there (`recording_saver.rs:226`, `import.rs:343`); the user's `save_folder` preference is displayed but not used for writing, so nothing outside the default root is ever removed.

A skipped or failed removal is logged with a distinct reason (missing, outside the recordings folder, not resolvable / permission error, used by another meeting; no paths) and does not fail the delete. A meeting without a folder logs at debug. No new dependencies, no UI change.

Out of scope, for a single-user desktop app: the check-then-remove window, i.e. a local process swapping a path component for a symlink between the check and `remove_dir_all` (it already has the user's file access), or a concurrent save into the same old folder at the exact moment its meeting is deleted.

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

Ran on Linux: `cargo test -p meetily --lib -- api::api::tests` (5 new tests, in-memory SQLite with the real migrations): root containment (outside, `..` escape, symlink out, empty, relative); delete removes its own folder and keeps shared / outside-root folders; NULL, empty and relative `folder_path` remove nothing; a folder used by another meeting under a different spelling (trailing slash, symlink alias, case variant, nested either way, or an unresolvable path with the same folder name) is kept. All failed before the fix and pass now. `cargo check -p meetily --all-targets` and `cargo clippy -p meetily --lib` clean for the changed lines. No live recording/deletion test in the app and no macOS/Windows test was done.

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
Behaviour change by design: deleting a meeting now deletes its files. Folders outside the default recordings folder (e.g. legacy paths) are intentionally kept.

## 5. Not done / risks
- No live app test, no macOS/Windows run (canonicalize on Windows yields `\\?\` paths for both sides, so `starts_with` still compares like with like, but untested).
- Check-then-remove race (symlink swap between check and removal) is out of scope, documented in the PR.
- Root is `get_default_recordings_folder()`, not the user's `save_folder` preference, because that is what recording/import actually use on devtest. If upstream later honours a custom save folder, meetings there would be kept (safe failure mode).
- Folder is deleted after the rows commit; if the app crashes in between, the folder is orphaned (same as before).
- `api.rs` was not rustfmt-clean upstream; only my lines are formatted.
