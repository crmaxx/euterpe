---
title: Welds data-layer review fixes should stay page-shaped and set-oriented
date: 2026-07-05
category: conventions
module: Data Layer
problem_type: convention
component: database
severity: medium
applies_when:
  - "Fixing review findings in Welds-backed repositories"
  - "Replacing full-catalog in-memory pagination with repository-shaped queries"
  - "Implementing bulk Download Queue actions that assign queue positions"
  - "Testing migration metadata without duplicating Welds metadata models"
tags: [welds, data-layer, keyset-pagination, download-queue, migrations, query-shape, review-fixes]
---

# Welds data-layer review fixes should stay page-shaped and set-oriented

## Context

A data-layer review found three related problems after the Library sorting and Download Queue controls work: `retry_all_failed` repeated queue-position scans per failed job, `list_albums_keyset` shaped pages by loading too much catalog data into memory, and migration tests duplicated the production `_welds_migrations` model.

The first implementation attempt for the Library fix used `sqlx::QueryBuilder` with a local fallback comment. After reading the Welds advanced, include, and custom-select documentation, the better path was to keep the query inside Welds using custom select, joins, manual expression ordering, and grouped aggregates (session history).

## Guidance

For repository review fixes, first ask whether the operation is incorrectly shaped, not whether raw SQL would be shorter. If Welds can express the query through `select_as`, relationships, `left_join`, `where_manual2`, `order_manual`, grouped selects, or typed model updates, keep it in the Welds Data Layer.

Library album pages should be selected as pages before track counts are attached. The durable shape is:

```rust
let page_records = query_album_list_page(handle, &params, query.as_deref()).await?;
let has_more = page_records.len() > params.limit;
let page_records = page_records.into_iter().take(params.limit).collect::<Vec<_>>();
let track_counts = album_track_counts(handle, &page_records).await?;
```

The page query can still use Welds for joined projection and manual ordering expressions:

```rust
Album::all()
    .select_as(|album| album.id, "id")
    .select_as(|album| album.title, "title")
    .left_join(
        |album| album.artist,
        Artist::select_as(|artist| artist.name, "artist_name"),
    )
    .order_manual(album_list_order_sql(params.sort, params.order))
    .order_by_asc(|album| album.id)
    .limit(params.limit as i64 + 1)
    .run(handle.client())
    .await?
    .collect_into()?
```

Track counts should be computed only for the selected page ids:

```rust
Track::all()
    .where_col(|track| track.album_id.in_list(&album_ids))
    .select_as(|track| track.album_id, "album_id")
    .select_count(|track| track.id, "track_count")
    .group_by(|track| track.album_id)
```

Bulk queue actions should compute shared state once, then mutate rows with the same reset helper used by single-row actions. `retry_all_failed` now builds a per-`DownloadJobType` max-position map from queued jobs, sorts failed jobs deterministically, increments the per-type position in memory, and calls a shared `requeue_job_at_position` helper. That preserves torrent runtime cleanup without calling `next_queue_position` for every failed row.

Migration metadata assertions should go through production-owned code. Keep the `MigrationLog` Welds model private in migrations and expose a small helper such as `applied_migration_count` for tests that need to assert repaired legacy databases recorded applied migrations.

## Why This Matters

The Welds Data Layer boundary does not automatically prevent expensive or duplicate data access. A repository can still be logically backend-owned while doing full-catalog work, per-row scans, or duplicated metadata mappings.

Page-shaped queries protect keyset pagination correctness and scale: search, sort, cursor filtering, tie-breaking, and limit must happen before page rows are selected. Bulk queue actions protect user-visible ordering and worker behavior only when queue positions are assigned against a stable per-type view of the queue. Production-owned migration helpers prevent tests from silently drifting away from the schema model that migration code actually uses.

The rejected `sqlx::QueryBuilder` attempt is the useful caution (session history): a raw SQL fallback may compile and pass behavior tests, but it should come after checking the Welds custom-select surface, not before.

## When to Apply

- Apply this when a review flags N+1 behavior, repeated full-table scans, raw SQL drift, or duplicated database models inside `crates/euterpe-data`.
- Apply this when a paginated repository function needs joined data, aggregate counts, cursor predicates, or expression ordering.
- Apply this when a bulk operation currently calls a single-row helper that recomputes shared queue state.
- Apply this when a test needs migration metadata; add a narrow production helper instead of defining a second model for Welds-owned tables.

## Examples

Before, bulk retry reused the single-row helper and each failed job recomputed the next queue position from the whole queue:

```rust
for mut job in jobs {
    if job.status == DownloadJobStatus::Failed.as_str() {
        requeue_failed_job(handle, &mut job).await?;
        retried += 1;
    }
}
```

After, the bulk path computes max queued positions once and assigns positions in stable order:

```rust
let mut next_positions = jobs
    .iter()
    .filter(|job| job.status == DownloadJobStatus::Queued.as_str())
    .map(|job| Ok((parse_job_type(&job.job_type)?, job.queue_position)))
    .collect::<Result<Vec<_>>>()?
    .into_iter()
    .fold(HashMap::<DownloadJobType, i64>::new(), |mut positions, (job_type, position)| {
        positions
            .entry(job_type)
            .and_modify(|max_position| *max_position = (*max_position).max(position))
            .or_insert(position);
        positions
    });
```

Before, migration tests defined their own `_welds_migrations` mapping. After, tests call the production helper:

```rust
pub async fn applied_migration_count(handle: &DataHandle) -> Result<u64> {
    Ok(MigrationLog::all().count(handle.client()).await?)
}
```

## Related

- [Welds first-party data layer boundary](../architecture-patterns/welds-first-party-data-layer.md)
- [Library album sorting stays contract-first and backend-owned](./library-album-sorting-openapi-welds-keyset.md)
- [Download queue controls stay contract-first and backend-owned](./download-queue-controls-openapi-contract.md)
- [SMB storage review fixes across job state, handles, and API contracts](../integration-issues/smb-storage-review-fixes.md)
- `docs/plans/2026-07-04-003-fix-data-layer-review-findings-plan.md`
- `crates/euterpe-data/src/repositories/catalog.rs`
- `crates/euterpe-data/src/repositories/download_jobs.rs`
- `crates/euterpe-data/src/migrations/mod.rs`
