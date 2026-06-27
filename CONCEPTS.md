# Concepts

Shared domain vocabulary for this project — entities, named processes, and status concepts with project-specific meaning. Seeded with core domain vocabulary, then accretes as ce-compound and ce-compound-refresh process learnings; direct edits are fine. Glossary only, not a spec or catch-all.

## Library Storage

### Library Storage
The configured storage root that contains the user's music library, independent of whether the backing location is local disk or a network share.

Library operations should resolve media paths through Library Storage rather than through environment-managed process paths. This keeps indexed paths portable across backends and lets the same media flows run against local and remote storage.

### Storage Backend
A concrete implementation that performs Library Storage operations against one backing system, such as local filesystem storage or SMB storage.

Backends are peers behind the storage interface; application services should not special-case local disk except inside the local backend itself.

### Storage Path
A path inside Library Storage, expressed relative to the configured library root with portable separators.

Storage Paths are the form stored in the database for library media. They are distinct from absolute host paths, network UNC paths, and process-local temporary paths.

### Atomic Write
A storage write that first publishes bytes to a sibling temporary object and then replaces the destination only after the temporary object has been fully written.

Atomic Write protects existing media from partial replacement. On failure, the temporary sibling should be removed when the backend can do so safely.

### Storage Watch
The background observation of Library Storage changes that converts backend notifications into library refresh work and status visible to the server.

Storage Watch is allowed to degrade when a backend cannot provide reliable notifications; degraded watch status should be observable rather than silently treated as healthy.

## Library Ingestion

### Library Scan
The process that reconciles Library Storage with the catalog of known artists, albums, tracks, covers, and media metadata.

Library Scan may run against the whole library or a subtree. Its outcome depends on persisted catalog effects, not only on discovering files or advancing progress counters.

### Scan Run
A persisted execution record for a Library Scan, including its lifecycle state and progress counters.

Scan Run terminal states are monotonic from the caller's point of view: once a run is cancelled or otherwise terminal, later worker progress or completion attempts should not make it running or successful again.

### Torrent Import
The process that moves completed torrent payloads from the local incoming area into Library Storage and then schedules the library work required to index the imported media.

Torrent Import is not complete just because files were copied. When it requires a follow-up scan, that scan's terminal state is part of the import outcome.

## Data Layer

### First-Party Data Layer
The project-owned boundary for application database access, migrations, and typed test fixtures.

First-party code should call this layer for persisted application data instead of constructing database operations directly in routes, workers, services, or tests.

### Welds Data Layer
The First-Party Data Layer implementation backed by Welds ORM in crate `euterpe-data`.

The Welds Data Layer keeps existing SQLite data compatible while project-owned database behavior lives behind typed models, repositories, migrations, and fixtures.
