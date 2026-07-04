---
title: Library album sorting stays contract-first and backend-owned
date: 2026-07-04
category: conventions
module: Library Albums
problem_type: convention
component: development_workflow
severity: medium
applies_when:
  - "Adding or changing Library album list sorting, filtering, or pagination behavior"
  - "Changing an internal-only Library API enum consumed by the generated frontend client"
  - "Extending keyset pagination where sorting must happen before page boundaries are selected"
  - "Choosing whether to add deprecated aliases or compatibility shims for repository-local API consumers"
tags: [library-albums, album-sorting, keyset-pagination, openapi-contract, internal-api-contract, generated-types, welds-data-layer, msw]
---

# Library album sorting stays contract-first and backend-owned

## Context

The Library page needed user-selectable album sorting by Album title, Artist, Album date, and Date added while preserving backend-owned ordering, keyset pagination, search, and the OpenAPI-first workflow.

The important constraint was that this is an internal API contract consumed by this repository's frontend, so the implementation could replace the old `year` API sort value directly instead of carrying a deprecated alias or compatibility shim.

The solved slice crossed `openapi/openapi.yaml`, generated frontend schema, the Rust route parser, the Welds-backed catalog repository, keyset cursor construction, Library UI controls, MSW fixtures, and backend/frontend tests. Sorting only the already-loaded frontend pages would be wrong because keyset pagination must operate on the same global ordering used by the backend cursor.

## Guidance

Start with the OpenAPI enum and let every layer follow that contract. Replace old API-facing sort values rather than adding aliases unless there is a known external consumer.

```yaml
sort:
  type: string
  enum: [title, artist, album_date, date_added]
  default: title
```

Regenerate the frontend schema and derive application types from the generated operation. In this codebase, the operation query object is optional, so the generated type alias needs nested `NonNullable`; eslint and focused vitest runs will not catch this type-level mismatch.

```ts
export type LibraryAlbumSort = NonNullable<
  NonNullable<operations["listLibraryAlbums"]["parameters"]["query"]>["sort"]
>;
```

Keep sorting backend-owned and repository-owned before keyset pagination. Extend the repository sort enum and row shape with the fields needed for the selected ordering, including `created_at` for Date added.

```rust
pub enum AlbumListSort {
    Title,
    Artist,
    AlbumDate,
    DateAdded,
}

pub struct AlbumListRow {
    pub title: String,
    pub artist_name: String,
    pub year: Option<i32>,
    pub created_at: String,
    pub id: i64,
}
```

Use an order-aware sentinel for Album date so unknown dates sort after known dates in both ascending and descending user-facing order. If the sentinel is not order-aware, reversing the comparison pulls unknown dates to the front for descending sorts.

```rust
pub fn album_date_sort_value(year: Option<i32>, order: AlbumListOrder) -> i64 {
    match (year, order) {
        (Some(year), _) => i64::from(year),
        (None, AlbumListOrder::Asc) => i64::MAX,
        (None, AlbumListOrder::Desc) => i64::MIN,
    }
}
```

Normalize `q` once at the API boundary before computing the cursor fingerprint, then pass the same effective query into the repository. Cursor validation must compare the sort field, sort order, stable tie-breaker, and effective search fingerprint that produced the page.

```rust
pub fn normalize_album_search_query(q: Option<String>) -> Option<String> {
    q.as_deref()
        .map(str::trim)
        .filter(|q| !q.is_empty())
        .map(|q| q.to_lowercase())
}
```

On the frontend, store `sort` and `order` in Library page state, feed them into the list query params, and rely on the query key changing to reset loaded pages. Do not reorder a partially loaded result set in React.

## Why This Matters

Keyset pagination makes sort semantics part of the data contract. A cursor is meaningful only for the ordering that produced it. If the frontend reorders loaded albums, or if the backend changes sorting without cursor validation, users can see duplicated rows, skipped rows, or inconsistent Load more results.

OpenAPI-first also prevents drift between the Rust route parser and TypeScript client. The generated schema is the single source of truth for allowed values, and deriving `LibraryAlbumSort` from it avoids hand-maintained unions like `"title" | "artist" | "year"` surviving after the contract changes.

The no-shim decision matters because this API is internal. Carrying both `year` and `album_date` would create unnecessary compatibility surface, duplicate behavior to test, and future ambiguity.

## When to Apply

Apply this pattern when adding, renaming, or removing list sort/filter parameters on an internal OpenAPI-backed endpoint that uses keyset pagination.

Apply it when frontend controls affect a paginated list and the result order must be globally correct, not just visually reordered for the current page.

Apply it when the API parameter name is a product-facing concept but the storage field has a different name, such as `album_date` being backed by `albums.year`.

Do not add raw SQL if the Welds repository can express the data access clearly. Do not add deprecated aliases for internal-only OpenAPI changes by default. Do not assume eslint or vitest covers generated API type correctness; run the TypeScript typecheck or the project gate that includes it.

## Test Coverage

Cover each boundary rather than only the UI:

- Repository tests: Album date asc/desc with unknown dates last, Date added asc/desc with pagination, deterministic tie-breaking.
- API tests: `sort=album_date`, `sort=date_added`, removed `sort=year` returns bad request, stale cursor fails when sort/order/search changes, normalized search can continue with a matching cursor.
- Frontend client tests: URL serialization for `album_date` and `date_added`.
- Library page tests: default title ascending, selecting Artist, Album date desc, Date added desc, sort change after Load more resets cursor to null, search preserves current sort/order.
- MSW handlers: mirror the same sort/order/search behavior enough for page-level tests to observe ordering changes.
- Gates: regenerate schema, OpenAPI lint/build, focused Rust repository/API tests, focused frontend tests, frontend lint, TypeScript typecheck, `cargo fmt --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, and `git diff --check`.

## Related

- [Internal OpenAPI contracts should change directly](internal-openapi-contracts-no-deprecated-shims.md)
- [Welds first-party data layer boundary](../architecture-patterns/welds-first-party-data-layer.md)
- [Download queue controls stay contract-first and backend-owned](download-queue-controls-openapi-contract.md)
- `docs/plans/2026-07-04-001-feat-library-album-sorting-plan.md`
- `openapi/openapi.yaml`
- `crates/euterpe-server/src/routes/library.rs`
- `crates/euterpe-data/src/repositories/catalog.rs`
- `frontend/src/features/library/LibraryPage.tsx`
