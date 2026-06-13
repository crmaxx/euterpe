# Storage backend switch (local ↔ SMB)

When an admin changes **library storage kind** in Settings (local disk ↔ SMB), the server
returns optional fields on `PATCH /api/v1/settings/storage`:

| Field | Type | When present |
|-------|------|----------------|
| `storage_migration_hint` | string | Kind changed on PATCH |
| `recommend_full_scan` | `true` | Kind changed on PATCH |

`GET` responses omit these fields. Patches that only update paths, host, or credentials
within the same kind also omit them.

## TDD Execution Policy

- Before each behavior change, add or update the smallest failing API/frontend test that proves the desired contract.
- Keep the first implementation minimal, then expand only when the test exposes a real integration gap.
- Run the task-specific targeted test before marking the behavior complete.

## Client behavior

1. Save storage settings as usual.
2. If `recommend_full_scan` is true, show a toast (or alert) using
   `storage_migration_hint` when provided.
3. Direct the user to **Library → Rebuild index** (full scan) so DB paths and the storage
   backend stay consistent.

## Rationale

Switching backends does not migrate files automatically. The library index still refers to
paths resolved through the previous backend until a full scan repopulates metadata from the
new storage location.

## Tests

- `crates/euterpe-server/tests/api_storage_settings.rs` — kind change vs same-kind PATCH
- `frontend/src/features/settings/StorageSettingsSection.test.tsx` — migration toast
