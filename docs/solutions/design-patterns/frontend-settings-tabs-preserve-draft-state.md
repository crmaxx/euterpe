---
title: Settings tabs should keep form panels mounted when drafts matter
date: 2026-07-04
category: design-patterns
module: Frontend Settings
problem_type: design_pattern
component: testing_framework
severity: low
applies_when:
  - "Splitting a settings page into tabs while forms may contain unsaved edits"
  - "Adding conditional tabs whose availability comes from server capabilities"
  - "Testing React tab panels that should preserve local component state"
related_components:
  - "React"
  - "SettingsPage"
  - "Qobuz scheduled sync"
tags: [frontend, react, settings, tabs, accessibility, testing, draft-state]
---

# Settings tabs should keep form panels mounted when drafts matter

## Context

The Settings page grew from a Qobuz account page into a mixed settings workspace: appearance, language, Qobuz account, scheduled favorites sync, integrations, conversion, scan workers, storage, downloads, and torrent settings. A simple vertical stack made unrelated controls hard to scan, so the page was reorganized into tabs.

Unlike the Sources page, Settings contains several forms with local draft state. Copying an unmount-on-select tab pattern would make tab switches discard unsaved edits. Settings also has a conditional Torrent tab controlled by server capability, so tab metadata and active-state fallback need to account for unavailable tabs.

## Guidance

Use a local typed tab model for Settings-style pages and render every eligible panel, hiding inactive panels with the `hidden` attribute rather than conditionally unmounting them. This preserves local form state while keeping accessible `tablist`, `tab`, and `tabpanel` relationships.

```tsx
type SettingsTab =
  | "general"
  | "scheduled-sync"
  | "integrations"
  | "converter"
  | "library-scan"
  | "library-storage"
  | "downloads"
  | "torrent";

const tabs = SETTINGS_TABS.filter(
  (tab) => !tab.requiresTorrent || hasTorrentSettings,
);
const visibleActiveTab =
  activeTab === "torrent" && !hasTorrentSettings ? "general" : activeTab;
```

Avoid correcting unavailable tabs with a synchronous `setState` inside an effect. The React hooks lint rule flags that pattern, and a derived `visibleActiveTab` expresses the fallback without creating an extra render cycle.

```tsx
<button
  type="button"
  role="tab"
  id={`settings-tab-${tab}`}
  aria-selected={visibleActiveTab === tab}
  aria-controls={`settings-panel-${tab}`}
  onClick={() => setActiveTab(tab)}
>
  {label}
</button>

<div
  role="tabpanel"
  id="settings-panel-scheduled-sync"
  aria-labelledby="settings-tab-scheduled-sync"
  hidden={visibleActiveTab !== "scheduled-sync"}
>
  <QobuzScheduledSyncSection />
</div>
```

Keep grouping decisions explicit. The default `General` tab should contain appearance, language, and Qobuz account controls. Storage deserves its own tab when it has its own summary and form; downloads can keep download quality and concurrency controls.

Tests should prove the user-observable contract:

- tab order and the default selected tab;
- every tab has an `aria-controls` relationship to an existing `tabpanel`;
- conditional tabs disappear when the server capability is absent;
- existing workflows still work after selecting their tab;
- a non-default form retains unsaved input after switching away and back.

For mounted hidden panels, scope assertions to the active accessible panel when duplicate text can exist in hidden DOM:

```tsx
await user.click(screen.getByRole("tab", { name: /library storage/i }));
const storagePanel = await screen.findByRole("tabpanel", {
  name: /library storage/i,
});

expect(
  await within(storagePanel).findByRole("heading", {
    name: /library storage/i,
    level: 3,
  }),
).toBeInTheDocument();
```

## Why This Matters

Tabbed settings pages have a different risk profile from tabbed source pickers. In a source picker, changing tabs often means changing workflows and unmounting inactive panels is acceptable. In a settings workspace, each panel can contain partially edited forms, so unmounting can silently lose user input.

Mounted hidden panels preserve the previous all-sections-visible behavior: hooks still run, existing save/run wiring stays intact, and local form state survives tab switches. The tradeoff is that tests must account for hidden DOM by querying through roles and scoping to accessible panels rather than assuming each text string appears only once.

Conditional tabs also need a render-safe fallback. Deriving the visible active tab from server capability keeps the tablist and visible panel aligned without adding state-reset effects that lint correctly rejects.

## When to Apply

- Apply this when turning a long settings page into tabs and the panels contain controlled forms or local draft state.
- Apply it when a tab is conditional on server info or feature capability.
- Use the existing unmount-on-select pattern only for pages whose inactive panels do not need to preserve draft state.
- Keep the tab model local unless several pages share the exact same behavior and accessibility requirements.

## Examples

The Settings implementation used a local tab metadata list and conditional Torrent eligibility:

```tsx
const SETTINGS_TABS = [
  { id: "general", labelKey: "settings.tabs.general" },
  { id: "scheduled-sync", labelKey: "settings.qobuzScheduled.title" },
  { id: "integrations", labelKey: "integrations.title" },
  { id: "converter", labelKey: "settings.converter.title" },
  { id: "library-scan", labelKey: "settings.libraryScan.title" },
  { id: "library-storage", labelKey: "settings.storage.title" },
  { id: "downloads", labelKey: "settings.downloads.title" },
  { id: "torrent", labelKey: "settings.torrent.title", requiresTorrent: true },
];
```

Regression coverage then clicked the scheduled sync tab, edited the cron field, switched to another tab, and returned:

```tsx
await user.click(await screen.findByRole("tab", {
  name: /scheduled favorites sync/i,
}));
const cron = await screen.findByLabelText(/cron expression/i);
await user.clear(cron);
await user.type(cron, "15 4 * * *");

await user.click(screen.getByRole("tab", { name: /^integrations$/i }));
await user.click(screen.getByRole("tab", {
  name: /scheduled favorites sync/i,
}));

expect(await screen.findByLabelText(/cron expression/i)).toHaveValue(
  "15 4 * * *",
);
```

The verification command in the plan used `npm --prefix frontend exec -- tsc -b --noEmit`, which runs `tsc` from the repository root and looks for a root `tsconfig.json`. For root-safe frontend type checks, pass the project file explicitly:

```bash
mise exec -- npm --prefix frontend exec -- tsc -b frontend/tsconfig.json --noEmit
```

## Related

- `docs/solutions/best-practices/frontend-tab-order-default-selection.md`
- Plan: `docs/plans/2026-07-04-002-feat-settings-page-tabs-plan.md`
- Implementation area: `frontend/src/features/settings/SettingsPage.tsx`
- Regression tests: `frontend/src/features/settings/SettingsPage.test.tsx`
