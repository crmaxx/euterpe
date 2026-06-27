---
title: "refactor: Move migrations fully to Welds builders"
type: refactor
date: 2026-06-27
topic: welds-migrations-no-raw-sql
---

# refactor: Move migrations fully to Welds builders

## Summary

Move first-party database migrations fully off raw SQL and onto Welds migration builders. Runtime migration execution, migration tests, and first-party compatibility fixtures must no longer depend on `.sql` migration files, `sqlx::migrate!`, `Manual::up`, or embedded SQL strings.

---

## Problem Frame

The Welds data-layer cutover made `euterpe-data` the owner of connection setup, migrations, repositories, and typed fixtures. The remaining gap is migration implementation: `crates/euterpe-data/src/migrations/mod.rs` still wraps the old `migrations/*.sql` files with Welds `Manual` migrations, and the compatibility test still uses `sqlx::migrate!` as the legacy database source.

That leaves the migration path conceptually split. Runtime code calls `euterpe-data`, but the schema is still defined by first-party raw SQL artifacts. This follow-up closes that gap by making Welds migration builders the first-party schema evolution source.

---

## Requirements

**Migration ownership**

- R1. `euterpe-data` must express first-party schema creation and schema changes through Welds migration builders such as `create_table`, `change_table`, column definitions, indexes, and supported Welds migration types.
- R2. Runtime migration execution must not use `Manual::up`, `Manual::down`, `include_str!` of `.sql` migration files, or embedded raw SQL strings.
- R3. The first-party `migrations/*.sql` chain must no longer be required to build, test, or run the application.

**Compatibility**

- R4. Fresh SQLite databases must migrate to the current schema shape with the Welds builder path.
- R5. Existing user databases created by the previous SQLx migration chain must be adopted without destructive reset or data loss.
- R6. Migration compatibility tests must prove current table, column, index, seed, and lifecycle expectations through typed detection, repositories, fixtures, or controlled non-SQL test databases.

**Testing and fixtures**

- R7. First-party migration tests must not use `sqlx::migrate!`, raw SQL setup, raw SQL assertions, or SQL migration files as fixtures.
- R8. Tests must cover migration idempotency, legacy database adoption, settings seeds, queue-related schema behavior, and current catalog/job/integration/Qobuz tables.
- R9. If a Welds builder API cannot represent a required SQLite schema feature, planning must surface the gap as an explicit blocker or approved exception before implementation uses any manual SQL fallback.

**Scope discipline**

- R10. This work must not change storage, SMB, media-path, torrent-import, converter, CUE split, or library-watch behavior.
- R11. This work must not add a CI raw-SQL scanner or automated enforcement guard.
- R12. Repository/domain refactors are out of scope unless required to verify migration compatibility without raw SQL.

---

## Key Decisions

- **Full first-party cleanup.** The chosen scope is stricter than runtime-only cleanup: raw SQL must leave first-party migration runtime, tests, and fixture paths, not just the production entrypoint.
- **Welds builders are the source of truth.** `Manual` migrations are treated as out of scope for this follow-up. The implementation should prefer explicit Welds table and column operations and only raise exceptions when the builder API cannot express the required behavior.
- **Compatibility remains mandatory.** Removing raw SQL artifacts cannot become a schema reset. Existing SQLite files that were migrated before the Welds cutover must continue forward.

---

## Acceptance Examples

- AE1. **Covers R1, R2.** Given a fresh database, when `euterpe_data::migrations::migrate` runs, then it creates the current schema through Welds migration builders and no runtime migration step loads `.sql` text.
- AE2. **Covers R4, R6.** Given a fresh database after migration, when schema detection runs, then all current tables, expected columns, indexes, and seeded settings are present.
- AE3. **Covers R5, R7.** Given a database representing the old SQLx-migrated schema, when the Welds migration runner executes, then existing data is preserved and the proof does not call `sqlx::migrate!` or first-party raw SQL fixtures.
- AE4. **Covers R8.** Given download, convert, CUE, Qobuz, integrations, catalog, and scan data expectations, when repository tests run after migrations, then lifecycle and uniqueness behavior stays compatible with the current application.
- AE5. **Covers R9.** Given a required schema feature that Welds cannot represent with builders, when planning reaches that feature, then the blocker or exception is documented before code introduces any manual SQL fallback.

---

## Scope Boundaries

- The legacy data-layer migration plan remains complete; this is a focused follow-up for migration implementation only.
- No storage or media behavior changes are included.
- No CI raw-SQL scanner is included.
- No broad repository reshaping is included beyond what compatibility verification needs.
- `docs/references` and `docs/dumps` remain out of scope unless a future plan explicitly changes that boundary.

---

## Success Criteria

- `crates/euterpe-data/src/migrations/mod.rs` no longer contains manual raw SQL migration wrappers.
- First-party migration tests do not call `sqlx::migrate!` and do not depend on `migrations/*.sql`.
- Fresh and legacy-compatible database paths are covered by tests.
- The application still builds and server/data tests remain green.
- Backend docs describe Welds migration builders as the schema evolution path.

---

## Dependencies / Assumptions

- Welds migration support from `https://book.weldsorm.com/migration.html` provides the baseline builder APIs for table creation, table changes, nullable columns, indexes, unique indexes, and foreign keys.
- Existing SQLx-migrated database compatibility can be represented in tests without keeping first-party raw SQL migration files as fixtures.
- If old `.sql` files need to remain as historical reference, they should not be part of first-party build, runtime, or test execution.

---

## Sources / Research

- Current follow-up origin: `docs/brainstorms/2026-06-25-welds-data-layer-requirements.md`
- Current implementation gap: `crates/euterpe-data/src/migrations/mod.rs`
- Current migration tests: `crates/euterpe-data/tests/migrations.rs`
- Historical SQL migration chain: `migrations`
- Welds migration documentation: `https://book.weldsorm.com/migration.html`
