---
title: Settings defaults should not normalize saved user input
date: 2026-07-05
category: conventions
module: Settings
problem_type: convention
component: service_object
severity: medium
applies_when:
  - "Adding persisted settings with defaults and editable user input"
  - "Validating cron-like schedule fields from the Settings UI"
  - "Splitting validation between frontend forms and backend settings services"
related_components:
  - "Frontend Settings"
  - "Qobuz scheduled sync"
  - "Developer tooling"
tags: [settings, validation, qobuz, cron, frontend, backend, tdd, mise]
---

# Settings defaults should not normalize saved user input

## Context

Qobuz scheduled sync originally treated an empty saved cron expression as if the user had not provided a value. The backend normalized empty or whitespace input back to the default cron expression during load, route patch, and save.

That made the Settings UI appear to save a cleared schedule successfully, while the backend silently rewrote the user's edit to the default. The same cleanup also removed stale Node setup guidance that still mentioned `nvm` even though the project runtime source is `mise.toml`.

## Guidance

Keep default settings separate from user input normalization. Defaults should seed first-run or missing settings, but once a field is editable, save paths should validate the submitted value rather than rewrite it to a default.

For scheduled settings, the durable backend shape is:

```rust
pub fn qobuz_scheduled_sync_defaults() -> QobuzScheduledSyncSettings {
    QobuzScheduledSyncSettings {
        cron_expression: DEFAULT_QOBUZ_SCHEDULED_SYNC_CRON.to_string(),
        ..QobuzScheduledSyncSettings::default()
    }
}

pub async fn save_qobuz_scheduled_sync(
    data: &DataHandle,
    value: &QobuzScheduledSyncSettings,
) -> Result<(), ApiError> {
    validate_qobuz_scheduled_sync(value)?;
    save_json(data, KEY_QOBUZ_SCHEDULED_SYNC_SETTINGS, value).await
}
```

Validation should reject empty enabled schedules before parsing the cron expression:

```rust
pub fn validate_qobuz_scheduled_sync(v: &QobuzScheduledSyncSettings) -> Result<(), ApiError> {
    if v.enabled {
        if v.cron_expression.trim().is_empty() {
            return Err(ApiError::bad_request("cron expression is required"));
        }
        CronSchedule::parse(&v.cron_expression)?;
    }
    Ok(())
}
```

The Settings UI should still block known-bad saves locally so the user gets immediate feedback and the API does not receive avoidable invalid input. Trim before deciding whether to PATCH, and send the trimmed value when it is valid:

```tsx
const trimmedCronExpression = cronExpression.trim();
if (!trimmedCronExpression) {
  toast({
    title: t("settings.qobuzScheduled.cronRequired"),
    variant: "destructive",
  });
  return;
}

await patch.mutateAsync({
  enabled,
  cron_expression: trimmedCronExpression,
  auto_download_new_favorites: autoDownload,
});
```

Use TDD for both sides of the boundary:

- backend API tests should first prove that `enabled: true` plus `cron_expression: ""` returns `400`, while a PATCH that omits `cron_expression` can still use the runtime/default value;
- frontend tests should first prove that empty and whitespace-only form values show the validation message and send no PATCH;
- valid cron values with leading or trailing whitespace should PATCH the trimmed expression.

Tooling guidance should follow the same source-of-truth rule. If `mise.toml` pins Node, developer-facing docs and version-check remediation should point to `mise install` / `mise exec` only, not a second runtime manager.

## Why This Matters

Silent normalization hides invalid edits. A user clearing a required schedule field is different from a new settings record that has no persisted value yet. Treating both as "use the default" makes tests pass for the happy path while preserving a confusing behavior: Save appears successful, but the system keeps a schedule the user tried to remove.

Separating defaults from validation keeps each boundary honest:

- defaults define what exists before user configuration;
- frontend validation gives fast feedback and avoids unnecessary API calls;
- backend validation remains authoritative for direct API callers and internal code paths;
- tests prove the old normalization behavior is gone rather than merely covered by UI logic.

The same principle applies to developer tooling. Two runtime setup paths create drift. One canonical runtime declaration makes warnings and remediation actionable.

## When to Apply

- Apply this when a settings object has both default values and user-editable persisted values.
- Apply this when a frontend form can locally detect invalid input before mutation.
- Apply this when backend state changes depend on a schedule expression, path, URL, token, or other required string.
- Do not apply it to genuinely optional fields where empty input is a meaningful saved value.
- Do not rely on UI validation alone; backend validation must still reject invalid direct requests.

## Examples

Before, the route and service normalized an empty cron into the default before saving:

```rust
settings = app_settings::normalize_qobuz_scheduled_sync(settings);
app_settings::save_qobuz_scheduled_sync(&state.data, &settings).await?;
```

The regression test also asserted the wrong contract:

```rust
assert_eq!(json["settings"]["cron_expression"], "0 3 * * *");
```

After the fix, there is no normalization helper or route call site. Empty enabled cron is a bad request:

```rust
assert_eq!(response.status(), StatusCode::BAD_REQUEST);
```

The frontend tests prove no PATCH occurs for empty input:

```tsx
await user.clear(await screen.findByLabelText(/cron expression/i));
await user.click(screen.getByRole("button", { name: /^save schedule$/i }));

expect(await screen.findByText(/cron expression is required/i)).toBeInTheDocument();
expect(qobuzScheduledSyncPatchCalls(fetchSpy)).toHaveLength(0);
```

The tooling cleanup removed `.nvmrc` and changed the Node check remediation to the single supported runtime path:

```js
console.error('Run `mise install && mise exec -- npm --prefix frontend run lint`.')
```

## Related

- [Settings tabs should keep form panels mounted when drafts matter](../design-patterns/frontend-settings-tabs-preserve-draft-state.md)
- [Internal OpenAPI contracts should change directly](internal-openapi-contracts-no-deprecated-shims.md)
- [Frontend tab order must stay aligned with default selection](../best-practices/frontend-tab-order-default-selection.md)
- Plan: `docs/plans/2026-07-05-001-fix-qobuz-cron-ui-and-mise-docs-plan.md`
- Implementation areas: `crates/euterpe-server/src/services/app_settings.rs`, `frontend/src/features/settings/QobuzScheduledSyncSection.tsx`
