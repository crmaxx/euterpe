---
title: Welds first-party data layer boundary
date: 2026-06-27
category: architecture-patterns
module: Data Layer Migration
problem_type: architecture_pattern
component: database
severity: high
applies_when:
  - "Moving persistence out of server routes, services, workers, or tests"
  - "Replacing SQLx-owned application data access with Welds repositories"
  - "Preserving existing SQLite databases while changing the persistence boundary"
  - "Adding migrations or typed fixtures for first-party application data"
tags: [welds, data-layer, sqlite, migrations, repositories, raw-sql, datahandle]
---

# Welds first-party data layer boundary

## Context

The `migrate-to-welds` branch moved Euterpe's application database ownership out of `euterpe-server` and into a new first-party data crate, `euterpe-data`. Before this branch, server modules owned SQLx pools, raw SQL repositories, migrations, and many test fixtures directly. That made storage and SMB work harder to reason about because routes, workers, library helpers, and tests could all bypass the intended persistence boundary.

The branch-wide solution is not only "use Welds." The durable pattern is: application code talks to a typed first-party data layer; Welds is the implementation inside that layer; the server owns orchestration, HTTP contracts, workers, storage, and integration behavior.

Session history shows this was implemented incrementally with TDD: job repositories were ported first, then Qobuz/favorites/integrations, then runtime callers moved from `SqlitePool` to `DataHandle` while server-facing APIs stayed stable where possible (session history).

## Guidance

Use `euterpe-data` as the project-owned boundary for application persistence:

- `DataHandle` owns the Welds SQLite client and is the runtime dependency passed through server state and workers.
- `euterpe-data::repositories::*` exposes typed repository functions and DTOs.
- repository internals keep Welds models private.
- `euterpe-data::migrations::migrate(&DataHandle)` owns schema setup and data seeds.
- `euterpe-data::fixtures::*` owns typed fixture builders for tests.
- server `test_db` modules may remain as compatibility/test helpers, but runtime code should not grow new first-party raw SQL paths.

Startup now constructs the data boundary once and fans it out:

```rust
let data = connect_database(&config.database_url).await?;
data_migrations::migrate(&data).await?;

let state = AppState::new(
    (*config).clone(),
    data.clone(),
    channels,
    hawk.clone(),
)
.await?;
```

`AppState` stores `DataHandle`, not `SqlitePool`:

```rust
#[derive(Clone)]
pub struct AppState {
    pub data: DataHandle,
    pub config: Arc<AppConfig>,
    // ...
}
```

When server behavior still needs a server-shaped API, keep the conversion at the boundary. For example, the data layer can return typed repository rows while HTTP routes keep their cursor encoding, response structs, and public status semantics. Session history shows this was especially important for favorites pagination and job lifecycle APIs: the port kept server contracts stable while moving persistence underneath (session history).

## Why This Matters

This separation keeps raw database behavior from leaking into unrelated code. Routes and workers can be tested as business logic, while repository tests lock persistence semantics such as queue ordering, Qobuz account secret boundaries, OAuth state consumption, integration secret preservation, and scan-run lifecycle behavior.

It also makes migrations safer. The new migration runner uses Welds builders in numbered one-file-per-migration modules:

```rust
#[path = "001_create_settings.rs"]
mod m001_create_settings;

const MIGRATIONS: &[MigrationFn] = &[
    create_settings,
    create_qobuz_favorites,
    // ...
];
```

Legacy SQLx-created databases are adopted without destructive reset by detecting the existing current schema and preserving user settings through typed repository reads/writes. Compatibility is verified with a binary SQLite fixture rather than bootstrapping tests from the old root SQL files.

The `sqlx` dependency has one deliberate workspace exception: the branch adds a local SQLite-only `sqlx` facade crate because the upstream `sqlx = 0.9` meta-crate pulls optional dependency edges that conflict with the vendored SMB/SSPI stack. The facade exports the SQLx core/SQLite surface needed by Welds and local compatibility helpers, but it is not a general MySQL/Postgres/macros replacement.

## When to Apply

Apply this pattern when adding or changing first-party persisted application data:

- add repository behavior in `crates/euterpe-data/src/repositories/`;
- add behavior tests in `crates/euterpe-data/tests/` before changing callers;
- expose only typed data-layer DTOs from `euterpe-data`;
- keep server API/request/response types in `euterpe-server`;
- pass `DataHandle` through server runtime dependencies instead of passing `SqlitePool`;
- put schema changes in numbered Welds migration files under `crates/euterpe-data/src/migrations/`;
- use typed seeds and fixtures instead of raw SQL setup in new first-party tests.

Do not add new application SQL directly in routes, services, workers, or server test helpers unless it is an explicit compatibility bridge that cannot reasonably live in `euterpe-data`.

## Examples

Before the migration, the server owned persistence modules under `crates/euterpe-server/src/db/*`, and callers passed `SqlitePool` through runtime state and worker dependencies.

After the migration:

```rust
use euterpe_data::DataHandle;

pub struct WorkerDeps {
    pub data: DataHandle,
    // worker-specific dependencies remain here
}
```

Repository tests define the data-layer contract first:

```rust
let handle = connect_database("sqlite::memory:").await.unwrap();
migrations::migrate(&handle).await.unwrap();

let job = download_jobs::create(&handle, request).await.unwrap();
download_jobs::claim_running(&handle, job.id).await.unwrap();
```

Migrations stay builder-owned and split by migration:

```rust
pub(super) fn create_settings(_: &TableState) -> welds::errors::Result<MigrationStep> {
    let migration = create_table("settings")
        .id(|c| c("key", Type::String))
        .column(|c| c("value", Type::Text))
        .column(|c| c("updated_at", Type::String));
    Ok(MigrationStep::new("001_create_settings", migration))
}
```

The migration tests should assert observable compatibility rather than old implementation details:

```rust
migrations::migrate(&handle).await.unwrap();

assert_eq!(
    settings::get(&handle, "downloads.settings").await.unwrap(),
    Some(r#"{"concurrency":7}"#.to_string())
);
assert!(library_scan_runs::latest(&handle).await.unwrap().is_some());
```

Known Welds builder gaps remain documented in backend migration docs. Partial unique indexes, composite primary keys, and some multi-column unique constraints are not reintroduced with raw SQL just to match old DDL. Where the builder cannot express an invariant, the current branch keeps the behavior in typed repository logic and regression tests.

## Related

- [Library album sorting stays contract-first and backend-owned](../conventions/library-album-sorting-openapi-welds-keyset.md) documents a concrete keyset pagination case where sort semantics belong in the Welds-backed repository, not in frontend reordering or server raw SQL.
- [SMB storage review fixes across job state, handles, and API contracts](../integration-issues/smb-storage-review-fixes.md) covers the storage-side review fixes that motivated part of the broader branch, but it does not document the Welds data-layer boundary.
- `docs/brainstorms/2026-06-25-welds-data-layer-requirements.md`
- `docs/plans/2026-06-25-001-refactor-welds-data-layer-plan.md`
- `docs/brainstorms/2026-06-27-welds-migrations-no-raw-sql-requirements.md`
- `docs/plans/2026-06-27-001-refactor-welds-builder-migrations-plan.md`
