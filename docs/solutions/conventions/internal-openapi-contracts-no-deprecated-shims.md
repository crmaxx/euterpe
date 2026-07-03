---
title: Internal OpenAPI contracts should change directly
date: 2026-07-03
category: conventions
module: API Contract
problem_type: convention
component: tooling
severity: medium
applies_when:
  - "Changing an OpenAPI endpoint consumed only by this repository's frontend"
  - "Replacing a query parameter, response field, or generated client type"
  - "Deciding whether to add deprecated aliases or compatibility shims"
tags: [openapi, api-contract, frontend, compatibility, tdd, generated-types]
---

# Internal OpenAPI contracts should change directly

## Context

The Qobuz Favorites page needed to default to the `In library` filter while still allowing `All` and `Not in library`. The first implementation added a new `library_filter` query parameter but kept the old `in_library` query parameter as a deprecated alias, with conflict handling when both appeared.

That was unnecessary for this project. The API consumer is the frontend in this repository, and the frontend types are generated from `openapi/openapi.yaml`. Keeping both parameters made the handler, tests, OpenAPI schema, and client surface larger without protecting a real external client.

Session history shows the path clearly: the implementation followed OpenAPI-first/TDD, added `library_filter`, then carried a legacy `in_library` alias forward. The user review corrected the assumption: because this is an internal-only contract, there should be one wire shape and no deprecated duplicate parameter (session history).

## Guidance

For API endpoints consumed only by this repository's frontend, change the OpenAPI contract directly and update all contract consumers in the same slice:

- update `openapi/openapi.yaml` first;
- regenerate the frontend schema;
- update the backend request parsing and response behavior;
- update the frontend API client, hooks, MSW handlers, and UI state;
- update backend and frontend tests to assert the new single contract.

Do not add deprecated compatibility parameters, duplicate fields, or migration shims unless there is a known external consumer or an explicit compatibility requirement.

The Qobuz Favorites filter is the model case. The final contract has one query parameter:

```yaml
- name: library_filter
  in: query
  description: Library membership filter. Defaults to `in_library`; use `all` for unfiltered favorites.
  schema:
    type: string
    enum: [in_library, all, not_in_library]
    default: in_library
```

The backend should parse only that parameter:

```rust
fn effective_favorites_in_library_filter(query: &FavoritesQuery) -> Result<Option<bool>, ApiError> {
    match query.library_filter.as_deref().unwrap_or("in_library") {
        "in_library" => Ok(Some(true)),
        "all" => Ok(None),
        "not_in_library" => Ok(Some(false)),
        _ => Err(ApiError::bad_request(
            "library_filter must be in_library, all, or not_in_library",
        )),
    }
}
```

The frontend should send the same enum through the generated type:

```ts
export type QobuzFavoritesLibraryFilter = NonNullable<
  operations["listQobuzFavorites"]["parameters"]["query"]["library_filter"]
>;
```

## Why This Matters

Duplicate internal contract shapes create fake compatibility work. Every alias needs parser branches, OpenAPI descriptions, generated type behavior, MSW support, and tests. That is useful only when callers outside the repository need time to migrate.

For internal-only endpoints, aliases make regressions easier. A missing query parameter can accidentally mean "default view" in one caller and "all data" in another. In the Qobuz Favorites change, the safe shape was a tri-state enum: `in_library`, `all`, and `not_in_library`. Callers that need full data, such as queue title lookup, must explicitly request `library_filter=all`.

This keeps the contract small and makes tests sharper: there is one accepted parameter and the default behavior is observable.

## When to Apply

- Apply this when the API is documented in this repository and consumed only by this repository's generated frontend client.
- Apply this when changing filter semantics, defaults, or field names where backend and frontend can ship together.
- Do not apply this when there is a real external client, a public API promise, or a deployment sequence that requires old and new callers to coexist.

## Examples

Before, the handler accepted both old and new shapes:

```rust
struct FavoritesQuery {
    library_filter: Option<String>,
    in_library: Option<bool>,
}
```

That forced conflict behavior:

```rust
if query.library_filter.is_some() && query.in_library.is_some() {
    return Err(ApiError::bad_request(
        "library_filter and in_library cannot be combined",
    ));
}
```

After the convention was applied, the query shape returned to one source of truth:

```rust
struct FavoritesQuery {
    library_filter: Option<String>,
}
```

Tests should then cover the single contract:

```rust
Request::builder()
    .uri("/api/v1/qobuz/favorites?type=album&library_filter=not_in_library")
```

and frontend helpers that need the old broad behavior should request it explicitly:

```ts
useFavoritesFlat({ limit: 100, library_filter: "all" });
```

## Related

- [SMB storage review fixes across job state, handles, and API contracts](../integration-issues/smb-storage-review-fixes.md) documents a bug-fix case where frontend and OpenAPI had to agree on the same public shape.
- [Frontend tab order must stay aligned with default selection](../best-practices/frontend-tab-order-default-selection.md) documents a related UI pattern: when a default changes, tests must assert the observable default and still cover non-default paths.
