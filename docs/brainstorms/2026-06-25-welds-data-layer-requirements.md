---
title: Welds Data Layer Requirements
date: 2026-06-25
topic: welds-data-layer
type: brainstorm
---

# Welds Data Layer Requirements

## Summary

Euterpe will move first-party database access into a new `euterpe-data` crate backed by Welds ORM. Server code will stop owning database queries directly, and database behavior will be exposed through typed models, repositories, migrations, and fixtures. Existing SQLite databases must keep working without destructive reset.

---

## Problem Frame

The current server data layer is built around direct SQLx access spread across `crates/euterpe-server/src/db`, selected services, tests, and SQL migration files. That makes storage and domain changes harder to review because callers can bypass shared invariants by adding another raw query.

The migration target is stricter than a repository cleanup. The desired end state is a first-party codebase where database reads, writes, migrations, tests, and seed helpers do not use raw SQL strings. Vendored reference material and captured dumps are not part of this source-of-truth boundary.

---

## Key Decisions

- **New data crate.** Create `euterpe-data` as the only first-party crate that owns database models, repository APIs, migration steps, and test fixtures.
- **Welds as ORM boundary.** Use Welds for typed model access and Welds migration APIs for schema evolution wherever the project owns the database operation.
- **Compatibility over reset.** Preserve the current SQLite schema shape and data compatibility so existing installations can migrate forward.
- **No automated raw-SQL guard in this scope.** The no-raw-SQL rule is a review and planning requirement, not a CI-enforced scanner in this migration.

---

## Requirements

**Data ownership**

- R1. `euterpe-data` owns all first-party database models, repository-style operations, migrations, and typed fixtures.
- R2. `euterpe-server` uses `euterpe-data` APIs for application data access instead of constructing database queries directly.
- R3. Service code may keep business orchestration, but persistence details belong behind the new data crate boundary.

**Welds usage**

- R4. Runtime CRUD and lookup flows use Welds-backed models or repository functions.
- R5. Schema migration code uses Welds migration APIs for project-owned migrations.
- R6. Tests and seed helpers use typed fixtures or repository APIs instead of raw database strings.
- R7. The migration does not add a CI guard or source scanner that blocks raw-SQL-like text.

**Compatibility**

- R8. Existing SQLite database files migrate forward without destructive reset.
- R9. Current table semantics, identifiers, uniqueness expectations, and nullable-field behavior remain compatible unless a later plan explicitly calls out a breaking change.
- R10. Existing server API behavior remains stable while callers move from SQLx-oriented modules to `euterpe-data`.

**Scope boundary**

- R11. First-party source, tests, migrations, and fixtures are in scope for the no-raw-SQL migration.
- R12. Vendored/reference snapshots under `docs/references` and captured dumps under `docs/dumps` are out of scope.
- R13. Storage, SMB behavior, media-path semantics, and library-watch behavior do not change as part of this ORM migration.

---

## Key Flows

- F1. Data access from server code
  - **Trigger:** A route, worker, or service needs persisted application data.
  - **Steps:** The caller invokes a typed `euterpe-data` API; `euterpe-data` maps the operation through Welds; the caller receives domain-shaped data or a stable error.
  - **Outcome:** Persistence behavior is centralized and direct query construction does not leak into server services.

- F2. Existing database startup
  - **Trigger:** Euterpe starts against a database created by the current schema history.
  - **Steps:** The new migration runner applies Welds-backed migration steps that preserve existing data; model verification confirms the expected shape where feasible.
  - **Outcome:** Existing installations keep their library, Qobuz, download, conversion, CUE, integration, and settings data.

- F3. Test data setup
  - **Trigger:** A server or data-layer test needs database state.
  - **Steps:** The test creates state through typed fixtures or repository calls; assertions inspect behavior through typed APIs.
  - **Outcome:** Tests exercise the same data boundary as production code.

---

## Acceptance Examples

- AE1. **Covers R1-R6.** Given a developer adds a new persisted domain concept, when they write models, migrations, fixtures, and service calls, then the work lands in `euterpe-data` and does not introduce first-party raw database strings.
- AE2. **Covers R8-R10.** Given an existing SQLite database from the current application, when the Welds data layer is used on startup, then the database migrates forward without deleting or reinitializing user data.
- AE3. **Covers R7.** Given a raw-SQL-like string appears in a first-party file, when CI runs, then this migration does not rely on an automated scanner to fail the build; reviewers and follow-up plans own enforcement.
- AE4. **Covers R12.** Given SQL-like text exists in vendored references or captured dumps, when the migration is complete, then those files may remain unchanged.

---

## Scope Boundaries

- CI guard or automated source scanner for raw SQL is out of scope.
- Rewriting vendored reference repositories, third-party examples, and captured diagnostic dumps is out of scope.
- Resetting existing SQLite databases is out of scope.
- Redesigning storage, SMB media I/O, torrent import semantics, converter behavior, or library watch behavior is out of scope.
- Switching away from SQLite as the default application database is out of scope.

---

## Dependencies / Assumptions

- Welds supports the needed SQLite model mapping, CRUD, transactions, and migration-building behavior for the current schema.
- Any current migration that cannot be represented with Welds APIs becomes an explicit planning blocker or exception request, not a hidden fallback to raw SQL.
- The current SQLx dependency may remain temporarily while planning defines the cutover, but the target state removes direct SQLx query usage from first-party data access.
- Existing schema documentation and migration files remain authoritative inputs for matching compatibility until the Welds migration chain replaces them.

---

## Success Criteria

- First-party runtime data access goes through `euterpe-data`.
- First-party migrations and test fixtures no longer require raw database strings.
- Existing SQLite databases continue to open and migrate forward.
- Server API tests that cover library, downloads, Qobuz, conversion, CUE, integrations, settings, and storage status pass through the new data boundary.
- Code review can reason about persistence behavior from typed data APIs rather than scattered query strings.

---

## Sources / Research

- `crates/euterpe-server/src/db` currently contains the main SQLx data modules.
- `migrations` currently contains the SQLite schema history that must be preserved in behavior.
- `crates/euterpe-server/src/services/library_scan.rs`, `crates/euterpe-server/src/services/download/worker.rs`, and related services contain first-party persistence touchpoints outside the current `db` module.
- Welds README describes async ORM support for SQLite, model derives, CRUD examples, transactions, and schema verification.
- Welds migration documentation describes Rust migration builders for table and column changes.
