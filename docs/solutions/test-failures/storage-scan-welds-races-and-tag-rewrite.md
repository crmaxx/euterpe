---
title: Storage scan Welds races and tag rewrite regressions
date: 2026-06-27
last_updated: 2026-06-27
category: test-failures
module: Storage Scan and Library Tags
problem_type: test_failure
component: database
symptoms:
  - "library_patch_album_tags_updates_all_track_files saw only one indexed track from a two-file album"
  - "library_scan_cancel_sets_status_and_rejects_repeat could observe success instead of cancelled"
  - "Short WAV tag rewrites could look updated through storage reads while path reads still saw old tags"
  - "wavpack-sys failed under CMake 4 because vendored WavPack declares an old CMake policy floor"
  - "worker_skips_existing_track_files could time out waiting for a SQLite pool connection"
root_cause: async_timing
resolution_type: code_fix
severity: high
related_components:
  - testing_framework
  - background_job
  - tooling
tags: [welds, storage-scan, library-tags, scan-cancel, concurrency, lofty, ci, wavpack]
---

# Storage scan Welds races and tag rewrite regressions

## Problem

Two `api_library` tests exposed regressions introduced around the storage-native scan and Welds data-layer migration. Album-wide tag patching could update only one physical file from a two-track album, and scan cancellation could be overwritten by late scan completion.

Follow-up CI checks exposed the same theme in adjacent areas: test infrastructure needs to isolate the behavior under test, and toolchain compatibility belongs in explicit config rather than relying on a developer machine's older defaults.

## Symptoms

- `library_patch_album_tags_updates_all_track_files` failed because one file still had `artist = "Old Artist"` after the album PATCH returned `200 OK`.
- Runtime tracing showed storage scan discovery saw both files, but only one track row survived in the catalog before the PATCH route listed tracks.
- The skipped file hit `UNIQUE constraint failed: albums.path` during concurrent `upsert_album` from storage scan workers.
- `library_scan_cancel_sets_status_and_rejects_repeat` sometimes failed because a scan reached `success` after cancellation.
- Short WAV storage rewrites could be read back as updated through `read_tags_storage`, while a direct path-based Lofty read still returned the old tag.
- `wavpack-sys` failed its build script with CMake 4 because the vendored WavPack `CMakeLists.txt` requested compatibility with a policy version below CMake 3.5.
- `worker_skips_existing_track_files` could fail in CI with `pool timed out while waiting for an open connection` while unwrapping `run_job`.

## What Didn't Work

- Looking only at `patch_library_album_tags` was misleading. The route used the correct storage path and rewrote the file it was given; the real album PATCH failure was that the catalog contained only one of the two tracks.
- Fixing only the tag writer was insufficient. A targeted short-WAV regression was still worth adding, but the API test continued to fail until the storage scan catalog race was fixed.
- Treating scan progress as a proxy for persisted catalog rows was unsafe. `process_storage_audio_entry` increments progress after a per-file error, so `files_indexed = 2` did not prove that two track rows were inserted.
- Updating scan run rows through loaded `DbState` values allowed stale terminal state writes. A late success/failure/progress save could overwrite a row that another request had already cancelled.
- Switching the download-worker test to a file-backed SQLite database did not fix the pool timeout. That disproved the first hypothesis that the in-memory SQLite pool size was the whole issue and showed the test was also exercising album download concurrency it did not need to cover.
- Patching vendored WavPack sources would have made dependency-cache state part of the fix. The project-level CMake policy environment is smaller and preserves the crate source.

## Solution

Make the data-layer operations match the concurrency and schema reality that the storage scan depends on.

First, `scan_keep_paths` now models the actual migration schema. The table has a generated `id` primary key plus `scan_id` and `path`; modeling `scan_id` and `path` as composite primary keys did not match the Welds migration output and could collapse or mishandle multiple keep rows for one scan:

```rust
#[derive(Debug, WeldsModel)]
#[welds(table = "scan_keep_paths")]
struct ScanKeepPath {
    #[welds(primary_key)]
    id: i64,
    scan_id: i64,
    path: String,
}
```

Second, album upsert now retries the existing-row path after an insert conflict. Storage scan workers can process sibling tracks concurrently; if two workers race to create the same album path, one insert wins and the other should bind to the existing album rather than skipping the track:

```rust
pub async fn upsert_album(handle: &DataHandle, album: AlbumUpsert<'_>) -> Result<i64> {
    if let Some(id) = update_existing_album(handle, &album).await? {
        return Ok(id);
    }

    let mut row = Album::new();
    // assign fields...
    if let Err(error) = row.save(handle.client()).await {
        if let Some(id) = update_existing_album(handle, &album).await? {
            return Ok(id);
        }
        return Err(DataError::from(error));
    }
    Ok(row.id)
}
```

Third, scan run state transitions now use conditional Welds updates. Progress and terminal writes only apply while the row is still `running`, so a cancelled run stays cancelled:

```rust
LibraryScanRun::where_col(|run| run.id.equal(id))
    .where_col(|run| run.status.equal("running"))
    .set(|run| run.status, "success".to_string())
    .set(|run| run.finished_at, Some(sqlite_timestamp()))
    .run(handle.client())
    .await?;
```

Fourth, tag rewrite now writes one canonical tag and removes old alternate tag blocks. This keeps storage-backed rewrites and direct path reads consistent for short WAV files:

```rust
let mut tag = Tag::new(tagged.primary_tag_type());
apply_tags_to_lofty_tag(&mut tag, tags);
tagged.clear();
tagged.insert_tag(tag);

tagged.save_to(&mut cursor, WriteOptions::new().remove_others(true))?;
```

Fifth, the local build environment now opts CMake into the compatibility policy floor needed by the vendored WavPack CMake project:

```toml
[env]
CMAKE_POLICY_VERSION_MINIMUM = "3.5"
```

Sixth, `worker_skips_existing_track_files` now keeps the download worker's runtime concurrency at `1`. The test's contract is "an existing file is skipped but both tracks are still registered", not "album downloads run concurrently". The separate `album_download_uses_runtime_concurrency_for_tracks` test remains responsible for the concurrency contract:

```rust
let runtime = test_runtime(&config);
runtime.write().await.downloads.concurrency = 1;

let deps = WorkerDeps {
    runtime,
    // ...
};
```

Finally, the migration legacy-schema detector dropped a no-op `.as_ref()` call after clippy correctly flagged it as useless. That was a mechanical CI cleanup, not a behavior change:

```rust
.map(|table| table.ident().name())
```

The fix is covered by focused regressions:

- concurrent `upsert_album` callers for the same path return the same album id;
- multiple `scan_keep_paths` rows survive for one scan id;
- cancelled scan runs ignore late success, failure, and progress writes;
- storage tag rewrite updates are visible to direct path reads for short WAV files;
- the skip-existing download worker test no longer depends on parallel track execution;
- the original `api_library` tests now pass.

## Why This Works

The album PATCH route depends on the catalog containing every track in the album path scope. The missing row was caused earlier by concurrent scan workers racing through a non-atomic find-then-insert album upsert. Retrying the lookup after an insert conflict turns that race into normal idempotent upsert behavior: one worker creates the album, the other attaches its track to the same album.

The scan keep-path fix addresses the schema mismatch introduced by the Welds migration. The repository model now matches the table that migrations create, so one scan can record every discovered path before stale pruning runs.

The scan cancellation fix makes terminal status monotonic at the repository boundary. A worker may still finish after a cancellation request, but its terminal update no longer changes a non-running row.

The tag rewrite fix removes ambiguity between multiple tag formats in the same audio object. Updating every existing tag type sounded conservative, but for these WAV fixtures it left path-based reads able to select stale metadata. Replacing the tag set with the file type's primary tag keeps both storage-byte and path-based reads aligned.

The CMake config fix addresses an environment compatibility break at the Cargo build-script boundary. It does not change WavPack behavior; it tells modern CMake to accept the vendored project's historical policy floor.

The download-worker test fix narrows the test to the behavior it owns. With `sqlite::memory:` the test database intentionally has one connection, and a multi-track album job can introduce unrelated DB acquisition timing into a skip-existing assertion. Setting concurrency to `1` removes that incidental async pressure while preserving dedicated coverage for parallel album downloads elsewhere.

## Prevention

- Treat repository `upsert_*` functions used from parallel workers as concurrent APIs. Add tests with two tasks and a barrier when a unique path, id, or external identifier is part of the contract.
- Keep Welds models structurally aligned with migration tables. If the migration uses `.id(...)`, the model should expose that id as the primary key rather than inventing a composite key in the model.
- Do not rely on scan progress counters as proof that catalog persistence succeeded. When scan workers can skip files after per-file errors, API tests should assert catalog rows or end-user effects.
- Make terminal job/run updates conditional on the active status. Late progress, success, and failure updates should not mutate cancelled or otherwise terminal rows.
- For storage-native tag writes, test both the storage reader and the legacy path reader when local tests still use direct filesystem assertions.
- Keep tests scoped to the contract they name. If a test is about skip/re-register semantics, do not also depend on worker parallelism unless the assertion is explicitly about parallelism.
- Treat C toolchain policy differences as repository configuration. A local build that only passes with older CMake is a CI risk even when Rust code did not change.

## Related Issues

- [Welds first-party data layer boundary](../architecture-patterns/welds-first-party-data-layer.md) documents the broader repository boundary and why persistence behavior should be locked in `euterpe-data` tests.
- [SMB storage review fixes across job state, handles, and API contracts](../integration-issues/smb-storage-review-fixes.md) documents earlier storage scan reconciliation and terminal-state contract fixes.
