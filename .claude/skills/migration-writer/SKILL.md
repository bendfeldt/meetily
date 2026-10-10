---
name: migration-writer
description: Write safe database migrations for meetily's sqlx + SQLite conventions. Use for any schema change.
when_to_use: add/alter a table, column, index, or constraint
---

# Migration Writer

meetily stores data in a local SQLite database owned by each user's install.
Migrations live in `frontend/src-tauri/migrations/` and are embedded with
`sqlx::migrate!("./migrations")` (`src/database/manager.rs`), so they run
automatically on app start against every user's existing data.

1. Read the current schema first: the existing migrations in order, and the
   repository code in `src/database/` that reads the tables you touch.
2. Name the file `YYYYMMDDHHMMSS_<verb>_<noun>.sql` (UTC timestamp, later than
   every existing file), matching e.g. `20251223000000_add_meeting_notes.sql`.
3. Migrations are forward-only (no down files). An app downgrade cannot undo
   one, so make every change additive and backward-compatible: new nullable
   columns or columns with a `DEFAULT`, new tables, new indexes. Never drop or
   rename a column a released version still reads.
4. Respect SQLite limits: `ALTER TABLE` can add columns but not change or drop
   constraints; that needs create-new-table → copy → drop → rename inside the
   migration, and must preserve all existing rows.
5. Never edit an already-merged migration; sqlx checksums applied ones and the
   app will refuse to start. Add a new one.
6. Update the Rust structs/queries in `src/database/` in the same change, and
   test against a copy of a populated database (`cargo test`, or a dev build
   on an existing profile), not only a fresh one.
   Output the migration, the code changes, and how existing data is upgraded.
