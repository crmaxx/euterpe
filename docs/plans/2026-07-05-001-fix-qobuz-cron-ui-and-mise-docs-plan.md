---
title: "fix: Move Qobuz cron empty-check to UI and remove nvm references"
date: 2026-07-05
type: fix
artifact_contract: ce-unified-plan/v1
artifact_readiness: implementation-ready
product_contract_source: ce-plan-bootstrap
execution: code
origin: user request
---

# fix: Move Qobuz cron empty-check to UI and remove nvm references

## Goal Capsule

| Field | Value |
|---|---|
| Objective | Remove backend cron auto-normalization for Qobuz scheduled sync, block empty cron saves in the Settings UI, and remove `nvm`/`nvm use` guidance in favor of `mise`. |
| Authority | User request; project convention that frontend work uses `mise` and TypeScript guidance, while backend persistence/settings behavior stays explicit and tested. |
| Execution profile | Lightweight fullstack fix, TDD/characterization-first. |
| Stop conditions | Stop if removing `.nvmrc` breaks an existing documented workflow that is still intentionally supported. |
| Tail ownership | No OpenAPI schema change is expected; this changes validation behavior and developer guidance only. |

---

## Product Contract

### Summary

Qobuz scheduled sync should no longer silently replace an empty saved cron expression with the default schedule.
The Settings page should prevent saving when the cron field is empty, and project-facing Node setup guidance should point only to `mise`.

### Problem Frame

The current backend normalizes an empty Qobuz scheduled-sync cron expression into `0 3 * * *` on load, route patch, and save.
That hides an invalid user edit and makes the UI look successful even when the user cleared the schedule field.
Separately, the frontend tooling docs and version check still mention `nvm` even though this project standardizes on `mise`.

### Requirements

- R1. Remove `normalize_qobuz_scheduled_sync` and its call sites so saved Qobuz scheduled-sync settings are not silently rewritten to the default cron expression.
- R2. Keep the existing default cron expression for new/default settings where no saved value exists.
- R3. When a user tries to save Qobuz scheduled-sync settings with an empty or whitespace-only cron expression, the Settings UI must show a validation error and must not send the PATCH request.
- R4. Valid cron expression saves must continue to work, including enabled sync, auto-download toggles, status refresh, and Run now behavior.
- R5. Remove `nvm`/`nvm use` mentions from frontend tooling guidance and align the visible instruction with `mise`.
- R6. Remove or stop advertising the repository `.nvmrc` path so `mise.toml` remains the canonical Node version source.

### Scope Boundaries

In scope:
- Qobuz scheduled-sync settings validation in backend services/routes and Settings UI.
- Focused backend and frontend tests for the changed empty-cron behavior.
- Frontend runtime/tooling documentation and script output that currently mentions `nvm`.

Out of scope:
- Changing the scheduled-sync OpenAPI schema.
- Changing the default cron value for first-time/default settings.
- Adding a cron editor, cron preset picker, or cron syntax helper UI.
- Reworking unrelated Settings tabs or Qobuz account flows.

### Acceptance Examples

- AE1. Given scheduled sync has default settings, when the settings endpoint is read, then `cron_expression` is still `0 3 * * *` and `next_run_at` is null while disabled.
- AE2. Given a user clears the cron field and clicks Save schedule, when the field is empty after trimming, then the UI shows a validation error and no PATCH request is sent.
- AE3. Given a client sends `enabled: true` with an empty `cron_expression`, when the backend validates the patch, then it rejects the request instead of replacing the cron expression with the default.
- AE4. Given a valid cron expression is saved from the UI, when Save schedule completes, then the request contains the trimmed expression and the success toast/status behavior remains unchanged.
- AE5. Given a developer runs the Node version check with the wrong Node major version, when the script prints remediation guidance, then the guidance mentions `mise` and not `nvm`.

---

## Planning Contract

### Key Technical Decisions

- KTD1. **Keep default settings separate from user input normalization.** `qobuz_scheduled_sync_defaults` should continue to seed default runtime settings, but saved/patch values should be validated as provided rather than normalized through a shared helper.
- KTD2. **Validate empty cron at the UI boundary before mutation.** The Settings form should prevent a known-bad save locally so users get immediate feedback and the backend does not receive avoidable invalid requests.
- KTD3. **Preserve backend rejection for direct/internal API callers.** The UI check is not a substitute for backend validation; direct PATCH requests with an empty enabled cron must still fail.
- KTD4. **Make `mise.toml` the only advertised Node runtime source.** The repo already pins `node = "24"` in `mise.toml`, so docs and script guidance should route developers through `mise` rather than mentioning `nvm` or `.nvmrc`.

### Assumptions

- The Qobuz scheduled-sync API is still consumed only by this repository's frontend, so no deprecated compatibility path is needed.
- The root `.nvmrc` exists only as stale setup guidance; `mise.toml` is the intended Node version source.

### Sources and Research

- `crates/euterpe-server/src/services/app_settings.rs`
- `crates/euterpe-server/src/routes/settings_ext.rs`
- `crates/euterpe-server/tests/api_qobuz.rs`
- `frontend/src/features/settings/QobuzScheduledSyncSection.tsx`
- `frontend/src/features/settings/SettingsPage.test.tsx`
- `frontend/src/i18n/locales/en.ts`
- `frontend/src/i18n/locales/ru.ts`
- `frontend/scripts/check-node-version.mjs`
- `docs/03-frontend/stack.ru.md`
- `mise.toml`
- `docs/solutions/design-patterns/frontend-settings-tabs-preserve-draft-state.md`

---

## Implementation Units

### U1. Remove backend scheduled-sync cron normalization

- **Goal:** Delete `normalize_qobuz_scheduled_sync` and make backend behavior explicit: defaults still provide a cron expression, but saved/patch values are validated rather than rewritten.
- **Requirements:** R1, R2, R3, AE1, AE3.
- **Dependencies:** None.
- **Files:**
  - `crates/euterpe-server/src/services/app_settings.rs`
  - `crates/euterpe-server/src/routes/settings_ext.rs`
  - `crates/euterpe-server/tests/api_qobuz.rs`
- **Approach:** Remove the normalize helper and call sites in load/save/patch flow. Keep `qobuz_scheduled_sync_defaults` unchanged for first-run settings. Tighten `validate_qobuz_scheduled_sync` so enabled sync with empty or whitespace-only cron fails through the same API error path as invalid cron.
- **Execution note:** Start by changing the existing empty-cron backend test from "normalizes to default" to "rejects empty cron".
- **Patterns to follow:** Existing invalid-cron API test in `crates/euterpe-server/tests/api_qobuz.rs`; existing settings patch pattern in `crates/euterpe-server/src/routes/settings_ext.rs`.
- **Test scenarios:**
  - Covers AE1. GET scheduled-sync settings on a fresh state still returns disabled settings with default cron and no next run.
  - Covers AE3. PATCH with `enabled: true` and `cron_expression: ""` returns bad request.
  - PATCH with `enabled: true` and whitespace-only `cron_expression` returns bad request.
  - PATCH with `enabled: true` and no `cron_expression` preserves the existing runtime/default cron and still returns a next run.
  - PATCH with a valid cron still succeeds and returns the saved expression.
- **Verification:** Backend tests prove empty values are no longer normalized while default settings and valid saves still behave as before.

### U2. Add Settings UI empty-cron save validation

- **Goal:** Prevent the Settings page from saving a blank Qobuz scheduled-sync cron expression and show a localized validation message.
- **Requirements:** R3, R4, AE2, AE4.
- **Dependencies:** U1 for backend behavior alignment.
- **Files:**
  - `frontend/src/features/settings/QobuzScheduledSyncSection.tsx`
  - `frontend/src/features/settings/SettingsPage.test.tsx`
  - `frontend/src/i18n/locales/en.ts`
  - `frontend/src/i18n/locales/ru.ts`
- **Approach:** Add a local validation branch in the save handler that trims the cron expression before deciding whether to call the patch mutation. When the trimmed value is empty, surface a destructive toast or inline field error using a new `settings.qobuzScheduled` translation key and return before calling `mutateAsync`. For valid values, send the trimmed cron expression so leading/trailing spaces are not persisted.
- **Execution note:** Implement the UI behavior test-first with Testing Library and MSW/fetch spy coverage.
- **Patterns to follow:** Existing scheduled-sync Settings tests in `frontend/src/features/settings/SettingsPage.test.tsx`; mounted-tab draft preservation documented in `docs/solutions/design-patterns/frontend-settings-tabs-preserve-draft-state.md`.
- **Test scenarios:**
  - Covers AE2. Clear the cron field, click Save schedule, assert the validation message appears and no PATCH call is made.
  - Covers AE2. Enter only spaces, click Save schedule, assert the same validation behavior.
  - Covers AE4. Enter a valid cron with leading/trailing whitespace, click Save schedule, assert PATCH is sent with the trimmed cron expression.
  - Existing Run now behavior remains covered and should not be affected by the save validation branch.
- **Verification:** Frontend tests prove blank cron is blocked locally and valid saves still hit the API with the expected body.

### U3. Remove nvm guidance from frontend tooling

- **Goal:** Remove stale `nvm`/`nvm use` references and make `mise` the visible Node setup path.
- **Requirements:** R5, R6, AE5.
- **Dependencies:** None.
- **Files:**
  - `.nvmrc`
  - `frontend/scripts/check-node-version.mjs`
  - `docs/03-frontend/stack.ru.md`
- **Approach:** Update the Node version check remediation text to mention `mise install` / `mise exec` only. Update frontend stack documentation to cite `mise.toml` as the source for Node 24. Remove `.nvmrc` if no remaining documented workflow depends on it.
- **Execution note:** Treat this as a tooling/docs cleanup; the proof is a literal search plus the script output path, not broad frontend behavior testing.
- **Patterns to follow:** Existing `mise` command examples in `docs/solutions/design-patterns/frontend-settings-tabs-preserve-draft-state.md`.
- **Test scenarios:**
  - Covers AE5. Node version check failure output no longer contains `nvm` or `nvm use`.
  - Literal repo search for `nvm` returns no tracked references after the cleanup.
  - Test expectation: no UI behavior tests for docs-only text changes.
- **Verification:** Tooling guidance and docs no longer advertise `nvm`; Node 24 remains declared in `mise.toml`.

---

## Verification Contract

| Gate | Applies to | Done signal |
|---|---|---|
| `cargo test -p euterpe-server --test api_qobuz` | U1 | Scheduled-sync settings API rejects empty enabled cron and preserves valid/default behavior. |
| `mise exec -- npm --prefix frontend run test -- SettingsPage.test.tsx` | U2 | Settings UI blocks blank cron saves and valid saves still PATCH. |
| `mise exec -- npm --prefix frontend run lint` | U2, U3 | TypeScript/React changes and tooling script stay lint-clean. |
| `mise exec -- npm --prefix frontend exec -- tsc -b frontend/tsconfig.json --noEmit` | U2 | Frontend typecheck passes after new validation/i18n changes. |
| `rg -n "nvm|nvm use" .` | U3 | No tracked project reference to `nvm` remains. |
| `cargo fmt --check` | U1 | Rust formatting stays clean. |
| `git diff --check` | All units | No whitespace errors. |

---

## Definition of Done

- U1 is done when `normalize_qobuz_scheduled_sync` no longer exists, no route/service call site references it, and backend tests cover empty-cron rejection.
- U2 is done when the Settings UI blocks empty/whitespace-only cron saves before PATCH and valid cron saves still succeed with the trimmed expression.
- U3 is done when project docs, scripts, and tracked config no longer mention `nvm`/`nvm use`, with `mise.toml` remaining the Node version source.
- The final diff contains no OpenAPI schema change unless implementation discovers the generated contract is inconsistent with the existing request/response shape.
- All Verification Contract gates pass, or any skipped gate is explained with the concrete blocker.
