# SMB folder browse (settings) — design

**Date:** 2026-05-27  
**Status:** Approved (approach B)

## Problem

After Save with `smb://host/dietpi/Musik/Flac`, the Settings **Folder listing** panel is empty while Finder lists albums. List shares works (IPC$/srvsvc only).

Root cause: `browse` query `path` is relative to the **library root**, but the UI sets `browsePath` to the saved `library.path` (`Musik/Flac`). `SmbStorage` joins `root.path + rel` → `Musik/Flac/Musik/Flac`. Navigation by full SMB paths worsens the prefix.

`app7_smb.pcap` captured list-shares only; verify browse with a post-Save Refresh capture against `osx3`.

## Approach B (chosen)

- **Browse `path`**: always relative to library root; `""` = list contents of library root.
- **Response paths**: relative to library root for SMB (and unchanged semantics for local).
- **UI**: `browsePath = ""` after Save; navigate by segment; show browse errors; tooltips on icon buttons.

## API

No OpenAPI shape change. Clarify behavior in docs:

- `GET /api/v1/settings/storage/browse?target=library&path=` — list library root.
- `path` segments are relative (no `..`, no leading `/`).
- Entry `path` is relative to library root for navigation.

## Backend

### `SmbStorage::list_dir`

After `list_directory`, map each entry path to be **relative to `self.root.path`** (library root within share):

- Strip `root/` prefix from `entry.path` when present.
- If result is empty for a directory entry, use `name` only.

### `browse_storage`

Unchanged signature; relies on fixed `list_dir` paths.

### Tests

- Unit test: root `Musik/Flac`, browse rel `""` → SMB location path `Musik/Flac` only (mock or path join test on `location()`).
- Unit test: strip prefix `Musik/Flac/Aarni` → `Aarni`.

## Frontend

### State

- `browsePath`: relative to library root; initialize and reset to `""` on Save / preset activate / SMB kind switch when library is SMB.
- `useEffect` on settings: SMB → `setBrowsePath("")` not `library.path`.

### Navigation

- Click folder: `setBrowsePath(joinRelative(browsePath, entry.name))`.
- Arrow up: `parentPath(browsePath)`; disable at `""`.
- Refresh: `browse.refetch()`.
- Select folder (SMB): update network location with `formatSmbLocation` using current `browsePath` under saved host/share.

### Folder listing panel

- Show `browse.error` message (destructive/muted).
- Loading state unchanged.
- Tooltips (`title` + i18n): Refresh, Up, Select folder.

## Verification

1. Save `smb://192.168.0.124/dietpi/Musik/Flac`, user `dietpi`, password saved.
2. Folder listing shows album folders (match Finder).
3. Up from subfolder works; Refresh keeps listing.
4. Pcap: tree connect `dietpi`, query directory on `Musik\Flac`.

## Out of scope

- Browse before Save (draft location).
- SMB query info-level changes unless still empty after path fix.
