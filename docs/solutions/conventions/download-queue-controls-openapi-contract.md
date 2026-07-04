---
title: Download queue controls stay contract-first and backend-owned
date: 2026-07-04
category: conventions
module: Download Queue
problem_type: convention
component: development_workflow
severity: medium
applies_when:
  - "Changing download queue actions or status transitions"
  - "Adding queue-wide controls that span API, repository, generated client, hooks, MSW, and page UI"
  - "Tightening internal-only API behavior without external compatibility requirements"
tags: [download-queue, openapi-contract, internal-api-contract, welds-data-layer, retry-failed, purge-completed]
---

# Download queue controls stay contract-first and backend-owned

## Context

The Download queue page needed status filtering, completed-history cleanup, a Retry all action, and an explicit status sort order. The important semantic correction was that `POST /api/v1/downloads/purge` kept its URL but changed from "delete terminal jobs" to "delete completed jobs". Failed and cancelled rows are intentionally preserved so users can retry, inspect, or manually delete them.

This is a fullstack queue-control convention rather than a one-off UI change. The work touched OpenAPI, generated frontend types, backend route DTOs, Welds repository behavior, React API hooks, MSW handlers, user-facing copy, and tests.

## Guidance

Keep download queue state transitions owned by the backend. For bulk retry, add one backend endpoint such as `POST /api/v1/downloads/retry` and one repository function such as `retry_all_failed`, rather than making the frontend enumerate failed rows and fire many single-job retry calls.

Use the Welds Data Layer for queue mutations. Even bulk actions should stay in typed repository functions unless Welds cannot reasonably express the operation. Routes should orchestrate HTTP behavior, scheduler wakeups, and cleanup side effects, not rebuild database rules with raw SQL.

When an existing path keeps its URL but changes meaning, update the whole internal contract slice together:

- change `openapi/openapi.yaml` first;
- rename the operation, summary, and description to match the new semantics;
- regenerate `frontend/src/api/schema.d.ts`;
- update backend request/response types and route names;
- update frontend client methods, hooks, MSW handlers, and page UI;
- update backend repository/API tests and frontend client/UI tests.

For queue filtering, pass the selected status into the existing list query boundary, such as `useDownloads(status)`. Treat toolbar actions as global queue actions, not current-filter actions. For example, Retry all retries all failed jobs on the backend even when the UI is filtered to another status.

Keep queue display order explicit with a status rank:

```text
running -> queued -> paused -> failed -> cancelled -> completed
```

Within a status bucket, preserve the local rule that already belongs there: active work keeps queue-position ordering, while failed, cancelled, and completed history uses newest-first ordering.

## Why This Matters

Bulk queue controls affect persisted state, worker scheduling, user recovery paths, and frontend cache invalidation. If the frontend owns Retry all by issuing one request per visible failed row, behavior can accidentally depend on the current filter, partially succeed, or drift from the single-job retry reset rules.

Completed-only history cleanup protects user recovery. Failed jobs remain actionable, cancelled jobs remain inspectable, and diagnostic context is not lost just because a user clears successful history.

OpenAPI-first updates prevent type drift. The Rust API types, generated TypeScript schema, client methods, React hooks, MSW mocks, and tests all describe one current Internal API Contract.

## When to Apply

- Apply this when adding or changing Download Queue controls that mutate multiple jobs.
- Apply it when changing status semantics, queue ordering, generated API types, or user-facing queue actions.
- Apply it when an endpoint URL remains stable but its business meaning changes.
- Do not add deprecated aliases or duplicate endpoints unless there is a known external consumer or explicit compatibility requirement.

## Examples

Completed-only purge should preserve failed and cancelled rows:

```rust
if job.status == DownloadJobStatus::Completed.as_str() {
    job.delete(handle.client()).await?;
}
```

Bulk retry belongs behind one backend operation:

```rust
pub async fn retry_all_failed(handle: &DataHandle) -> Result<u64> {
    let mut retried = 0;
    for mut job in DownloadJob::all().run(handle.client()).await? {
        if job.status == DownloadJobStatus::Failed.as_str() {
            requeue_failed_job(handle, &mut job).await?;
            retried += 1;
        }
    }
    Ok(retried)
}
```

Frontend filtering should pass status into the list query while leaving bulk action semantics backend-global:

```ts
const selectedStatus = statusFilter === "all" ? undefined : statusFilter;
const { data } = useDownloads(selectedStatus);
const retryAll = useRetryFailedDownloads();
```

Useful verification for this kind of change:

- repository tests for each status boundary in purge and retry behavior;
- API tests that validate OpenAPI response schemas;
- generated frontend client tests for new endpoint paths;
- MSW-backed page tests for filter and toolbar behavior;
- pure sort-helper tests for the status rank.

## Related

- [Internal OpenAPI contracts should change directly](./internal-openapi-contracts-no-deprecated-shims.md)
- [Welds first-party data layer boundary](../architecture-patterns/welds-first-party-data-layer.md)
- [SMB storage review fixes across job state, handles, and API contracts](../integration-issues/smb-storage-review-fixes.md)
- [Storage scan Welds races and tag rewrite regressions](../test-failures/storage-scan-welds-races-and-tag-rewrite.md)
