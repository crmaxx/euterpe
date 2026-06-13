---
title: SMB storage review fixes across job state, handles, and API contracts
date: 2026-06-13
category: integration-issues
module: SMB storage migration
problem_type: integration_issue
component: service_object
symptoms:
  - "Torrent import could finish successfully after the required library scan failed or was cancelled"
  - "Public server-info returned null library_storage while frontend and OpenAPI expected a structured storage summary"
  - "SMB read/list/write paths had resource cleanup and stale temp-file gaps on failure paths"
root_cause: logic_error
resolution_type: code_fix
severity: high
related_components:
  - background_job
  - testing_framework
tags:
  - smb
  - storage
  - torrent-import
  - api-contract
  - resource-cleanup
---

# SMB storage review fixes across job state, handles, and API contracts

## Problem

The SMB storage migration introduced a set of cross-boundary regressions where server jobs, SMB I/O lifecycle, and frontend/OpenAPI contracts no longer agreed on the same success and cleanup semantics. The practical result was that a torrent import could be reported as successful after its required scan failed, SMB operations could leave remote handles or sibling temp files behind on some failure paths, and bootstrap clients lost the structured `library_storage` contract.

## Symptoms

- Torrent import treated every non-running scan as success, including `failed`, `cancelled`, missing rows, and unknown terminal states.
- `GET /api/v1/server/info` returned `library_storage: null` even when library storage was configured, while the storage migration expected a structured public summary.
- The frontend client exposed only a POST draft browse wrapper under `browseStorage`, even though the server and OpenAPI retained GET browse for configured storage.
- SMB one-shot reads and fully consumed streams did not have test-proven async close behavior.
- SMB and local atomic writes could leave `.euterpe-part` sibling files after rename/publish failures.

## What Didn't Work

- Treating "scan is no longer running" as "scan succeeded" was too coarse. The scan table has explicit `success`, `failed`, and `cancelled` states, and torrent import depends on the scan as a required post-copy step.
- Returning `library_storage: null` avoided leaking credentials but broke the stated structured storage contract. The useful distinction is not "return nothing"; it is "return a deliberate non-secret summary".
- Collapsing configured-storage GET browse and draft-location POST browse into one frontend method hid a server/OpenAPI distinction that matters for callers.
- Directly trying to close the directory handle after `smb::Directory::query` construction failure did not compile cleanly: the upstream `QueryDirectoryStream<'_>` API borrows the `Arc<Directory>` through the query result. That leaves query-construction failure as a residual edge case unless the SMB wrapper changes shape.

## Solution

Make each boundary encode the real contract explicitly.

Torrent import now distinguishes scan terminal states:

```rust
match library_scan_runs::get_by_id(pool, scan_id).await {
    Ok(Some(run)) => match run.status.as_str() {
        "running" => tokio::time::sleep(Duration::from_secs(1)).await,
        "success" => return Ok(false),
        "failed" => {
            let error = run
                .error_message
                .unwrap_or_else(|| "library scan failed".into());
            return Err(ApiError::Message(format!(
                "torrent post-import scan failed: {error}"
            )));
        }
        "cancelled" => {
            return Err(ApiError::Message(
                "torrent post-import scan cancelled".into(),
            ));
        }
        other => {
            return Err(ApiError::Message(format!(
                "torrent post-import scan ended with unsupported status: {other}"
            )));
        }
    },
    Ok(None) => {
        return Err(ApiError::Message(format!("scan {scan_id} not found")));
    }
    Err(e) => return Err(e),
}
```

Server info now returns a structured storage view while scrubbing SMB credentials:

```rust
let library_storage = StorageSettingsView::from_with_watch_status(&storage, watch_status)
    .library
    .map(public_server_storage_view);
```

For SMB locations, `public_server_storage_view` keeps host/share/path/watch status but clears `username`, `workgroup`, and reports `password_configured: false` so public bootstrap does not disclose credential state.

Frontend storage browsing now has two wrappers that match the wire contract:

```ts
browseStorage: (path?: string) => {
  const params = new URLSearchParams({ target: "library" });
  if (path) {
    params.set("path", path);
  }
  return fetchJson<StorageBrowseResponse>(
    `/settings/storage/browse?${params.toString()}`,
  );
},

browseDraftStorage: (body: StorageBrowseRequest) =>
  fetchJson<StorageBrowseResponse>("/settings/storage/browse", {
    method: "POST",
    body: JSON.stringify(body),
  }),
```

SMB storage lifecycle paths now explicitly close on normal read/list completion and stream item errors. Atomic write publishes through a sibling temp object and deletes that temp if either the stream write or final rename fails:

```rust
if let Err(err) = self
    .write_stream_all(&tmp_location, credentials, stream)
    .await
{
    let _ = self.delete(&tmp_location, credentials).await;
    return Err(err);
}
match self
    .rename(&tmp_location, location, credentials, true)
    .await
{
    Ok(()) => Ok(()),
    Err(err) => {
        let _ = self.delete(&tmp_location, credentials).await;
        Err(err)
    }
}
```

The fix was verified with targeted tests:

- `cargo test -p euterpe-smb --lib`
- `cargo test -p euterpe-server --test api_server_info --test api_storage_settings`
- `cargo test -p euterpe-server services::download::torrent_job::tests::scan_wait --lib`
- `cargo test -p euterpe-server library::storage::tests::local_storage_streaming_atomic_write --lib`
- `npm run build`

## Why This Works

Each corrected path now treats a cross-boundary success signal as a typed contract instead of a loose absence of error.

For torrent import, the job only proceeds when the required scan reports `success`; failed indexing is no longer masked as a completed import. A stopped torrent job still returns the separate stopped path, so cancellation remains distinct from scan failure.

For server-info, public bootstrap data is restored without leaking SMB credentials. This preserves the client-visible `library_storage` shape while keeping secrets and credential presence out of the public endpoint.

For SMB I/O, async resources are closed on the paths that open them, and atomic writes clean temporary siblings when publish fails. This matters more on SMB than local disk because remote handles and partial siblings can affect later listing, rename, and delete behavior.

Session history also showed that the earlier torrent import work had already moved copy into `LibraryStorage::atomic_write_stream` and added cancellation checks (session history). The later review findings were therefore not about reintroducing local temp bridges; they were about terminal-state correctness and failure cleanup around the storage-native path.

## Prevention

- When a background job waits on another persisted job/run, assert every terminal status separately: success, failed, cancelled, missing row, and unknown status.
- For public bootstrap contracts, prefer a dedicated redacted view over returning `null` to avoid leaking fields. Tests should assert both the useful public fields and the absent secret fields.
- Keep generated frontend wrappers aligned with operation IDs. If OpenAPI has GET `browseStorage` and POST `browseDraftStorage`, the runtime client should expose both names.
- Add dry-run or fake backend counters for remote resources. Tests should prove open/close balance for one-shot reads, fully consumed streams, metadata, listing success, and listing item errors.
- For atomic remote writes, test both mid-stream failure and rename/publish failure. The expected postcondition is no final replacement and no lingering `.euterpe-part` sibling.
- Preserve a documented residual risk when an upstream API prevents a clean fix. In this case, `smb::Directory::query` construction failure still needs a different wrapper shape or upstream support to guarantee async close of the already opened directory handle.

## Related Issues

- Related docs: none found in `docs/solutions/`.
- GitHub issue search: skipped because `gh issue list` could not connect to `api.github.com` in this environment.
