---
title: Frontend tab order must stay aligned with default selection
date: 2026-07-03
category: best-practices
module: Frontend Sources
problem_type: best_practice
component: testing_framework
severity: low
applies_when:
  - "Reordering visible tabs or segmented controls in React components"
  - "Changing the primary workflow on a page with a default active tab"
  - "Updating tests for tabbed panels where non-default panels still need coverage"
tags: [frontend, react, tabs, accessibility, testing, sources]
---

# Frontend tab order must stay aligned with default selection

## Context

The Sources page was changed so the primary source flow appears first: `Qobuz Favorites`, then `Qobuz Url`, then `Torrent`. The original component rendered `Torrent` first and also selected it by default, so a pure DOM reorder would have left the first visible tab and the selected panel out of sync.

This is easy to miss because the panels still work after clicking each tab. The regression only appears on first render, in keyboard/tablist order, and in tests that assumed the old default panel.

## Guidance

When reordering tabs, update the default active tab in the same change as the rendered tab order. The first visible tab should be the initially selected tab unless there is an explicit product reason to do otherwise.

For React tab components, keep the assertions at the accessibility boundary:

```tsx
expect(screen.getAllByRole("tab").map((tab) => tab.textContent)).toEqual([
  "Qobuz Favorites",
  "Qobuz Url",
  "Torrent",
]);
expect(
  screen.getByRole("tab", { name: /qobuz favorites/i }),
).toHaveAttribute("aria-selected", "true");
```

Then keep coverage for non-default panels by clicking their tabs before checking panel content:

```tsx
await user.click(await screen.findByRole("tab", { name: /torrent/i }));
expect(
  await screen.findByRole("heading", { name: /magnet link/i, level: 3 }),
).toBeInTheDocument();

await user.click(await screen.findByRole("tab", { name: /qobuz url/i }));
expect(
  await screen.findByRole("textbox", { name: /qobuz album url/i }),
).toBeInTheDocument();
```

This pattern keeps tests tied to user-observable behavior rather than component internals. It also forces the implementation to preserve `aria-selected`, `aria-controls`, and `tabpanel` wiring when the tab order changes.

## Why This Matters

Tabbed UI has two contracts: visual priority and active state. If those diverge, the page can look like one workflow is primary while rendering another workflow by default. That is confusing for users and especially easy for tests to hide when old tests only assert that all tab labels exist.

Testing order through `getAllByRole("tab")` catches DOM and keyboard order, not just visual text presence. Testing `aria-selected` catches the default state. Clicking non-default tabs preserves regression coverage for panels that are no longer visible on first render.

## When to Apply

- Use this when changing order, labels, or default state for tabs, segmented controls, or page-level source selectors.
- Apply it when promoting a secondary workflow into the primary first-render workflow.
- Skip it for purely visual styling changes that do not affect DOM order or selected state.

## Examples

Before, the test only proved that each tab existed:

```tsx
expect(screen.getByRole("tab", { name: /torrent/i })).toBeInTheDocument();
expect(screen.getByRole("tab", { name: /qobuz url/i })).toBeInTheDocument();
expect(screen.getByRole("tab", { name: /qobuz favorites/i })).toBeInTheDocument();
```

That would pass even if the order was wrong or the default selected tab stayed on the old first tab.

After, the test proves both order and default state:

```tsx
expect(screen.getAllByRole("tab").map((tab) => tab.textContent)).toEqual([
  "Qobuz Favorites",
  "Qobuz Url",
  "Torrent",
]);
expect(
  screen.getByRole("tab", { name: /qobuz favorites/i }),
).toHaveAttribute("aria-selected", "true");
```

The implementation stays small: change the initial tab state and move the tab buttons into the requested order. Avoid introducing a tab registry unless several components already need shared tab metadata; for one component, direct JSX order is clearer.

## Related

- Plan: `docs/plans/2026-07-03-001-feat-sources-tab-order-plan.md`
- Implementation area: `frontend/src/features/sources/SourcesPage.tsx`
- Regression tests: `frontend/src/features/sources/SourcesPage.test.tsx`
