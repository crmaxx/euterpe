---
title: Sources Tab Order - Plan
type: feat
date: 2026-07-03
artifact_contract: ce-unified-plan/v1
artifact_readiness: implementation-ready
product_contract_source: ce-plan-bootstrap
execution: code
---

# Sources Tab Order - Plan

## Goal Capsule

- **Objective:** Reorder the Sources page tabs to `Qobuz Favorites`, `Qobuz Url`, `Torrent`.
- **Authority:** User request sets the tab order; existing Sources page component and test patterns set implementation boundaries.
- **Execution profile:** Lightweight frontend UI change with focused regression coverage.
- **Stop conditions:** Stop if implementation requires changing source routing, download behavior, Qobuz sync behavior, or torrent behavior beyond tab ordering.

## Product Contract

### Summary

The Sources page should present Qobuz Favorites first, Qobuz Url second, and Torrent third. The initial selected tab should match the first visible tab so page load and keyboard/tablist behavior remain coherent.

### Problem Frame

The current Sources page puts Torrent first and selects it by default. The requested ordering makes Qobuz Favorites the primary source flow on this page, followed by manual Qobuz URL download and then Torrent.

### Requirements

- R1. The Sources tablist renders tabs in this visual and DOM order: `Qobuz Favorites`, `Qobuz Url`, `Torrent`.
- R2. The Sources page initially selects `Qobuz Favorites`, matching the first tab in the requested order.
- R3. Selecting `Qobuz Url` and `Torrent` continues to show their existing panels without changing their form behavior.
- R4. Existing localization labels remain the source of tab text; no copy change is required.

### Scope Boundaries

- No new source types are added.
- No route, API, download queue, Qobuz sync, or torrent backend behavior changes are in scope.
- No visual redesign beyond the ordering and default selected tab is in scope.

## Planning Contract

### Key Technical Decisions

- KTD1. Keep the change local to `SourcesPage`: the tab order is owned by the component that renders `TabButton` instances and panel conditionals.
- KTD2. Make `qobuz-favorites` the initial `activeTab`: the first visible tab and selected tab should not diverge.
- KTD3. Update existing component tests instead of adding a separate test surface: `SourcesPage.test.tsx` already exercises tab rendering and panel selection.

### Sources & Research

- `frontend/src/features/sources/SourcesPage.tsx` defines `SourceTab`, the initial `activeTab`, the tablist order, and the tabpanel conditional rendering.
- `frontend/src/features/sources/SourcesPage.test.tsx` already covers Sources tabs, Torrent default content, and Qobuz Favorites panel activation; those assertions need to be adjusted to the new default.
- `frontend/package.json` uses Vitest through `npm run test`, with frontend lint available through `npm run lint`.

## Implementation Units

### U1. Reorder Sources Tabs

- **Goal:** Make `Qobuz Favorites` the first Sources tab, followed by `Qobuz Url` and `Torrent`.
- **Requirements:** R1, R2, R3, R4.
- **Dependencies:** None.
- **Files:** `frontend/src/features/sources/SourcesPage.tsx`.
- **Approach:** Change the initial `activeTab` to `qobuz-favorites` and render `TabButton` entries in the requested order. Leave tab IDs, `aria-controls`, translation keys, and panel conditionals intact so accessibility wiring and existing panel behavior stay stable.
- **Patterns to follow:** Preserve the existing `TabButton` API, `SourceTab` union, `useState<SourceTab>` state, and conditional panel rendering style already in `SourcesPage`.
- **Test scenarios:**
  - Initial render selects `Qobuz Favorites` and shows favorites content without clicking a tab.
  - The tablist exposes tabs in order: `Qobuz Favorites`, `Qobuz Url`, `Torrent`.
  - Selecting `Qobuz Url` shows the existing Qobuz URL input panel.
  - Selecting `Torrent` shows the existing Magnet link and `.torrent file` sections.
- **Verification:** On initial render, the selected tab is `Qobuz Favorites`, and switching to `Qobuz Url` or `Torrent` still displays the existing panel content.

### U2. Update Sources Page Regression Tests

- **Goal:** Lock the requested tab order and default selected panel in frontend tests.
- **Requirements:** R1, R2, R3.
- **Dependencies:** U1.
- **Files:** `frontend/src/features/sources/SourcesPage.test.tsx`.
- **Approach:** Update the tab-rendering test to assert DOM order from `getAllByRole("tab")`. Adjust tests that currently assume Torrent is the default by clicking the Torrent tab before asserting Magnet and `.torrent file` sections. Add or update an assertion that Qobuz Favorites content is visible by default.
- **Patterns to follow:** Use React Testing Library role queries and `userEvent` as the existing file does.
- **Test scenarios:**
  - Render Sources and assert the tab role names are exactly `Qobuz Favorites`, `Qobuz Url`, `Torrent` in order.
  - Render Sources and assert the `Qobuz Favorites` tab is selected by default and favorites content appears without clicking.
  - Click `Torrent` and assert the Magnet link and `.torrent file` sections still appear.
  - Click `Qobuz Url` and assert the Qobuz URL input panel still appears.
- **Verification:** The focused Sources page test fails before U1 and passes after U1.

## Verification Contract

| Gate | Scope | Done Signal |
|---|---|---|
| `mise exec -- npm --prefix frontend test -- SourcesPage.test.tsx` | Focused regression coverage for Sources tab order and panels | Passes with the new order and default tab expectations |
| `mise exec -- npm --prefix frontend run lint` | Frontend static checks | Passes without new lint errors |

## Definition of Done

- The Sources page tab order is `Qobuz Favorites`, `Qobuz Url`, `Torrent`.
- `Qobuz Favorites` is selected on first render.
- Existing Qobuz URL and Torrent panels still open from their tabs.
- Focused Sources tests cover tab order, default selection, and panel switching.
- No unrelated frontend, API, backend, or localization changes are included.
- Dead experimental code from implementation attempts is removed before completion.
