# SMB ChangeNotify Watcher Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Реализовать real-time-ish SMB library watching через SMB `ChangeNotify`, с visible degraded status при unsupported/disconnected.

**Architecture:** Watcher is a background service tied to configured library storage. For SMB it opens directory watch stream and debounces events into subtree scans. For local storage current behavior can remain polling/manual until local watcher is explicitly needed.

**Tech Stack:** Rust, `smb::Directory::watch_stream_cancellable`, Tokio task, broadcast/status API.

---

## Task 1: euterpe-smb watch API

**Files:**
- Modify: `crates/euterpe-smb/src/lib.rs`
- Test: `crates/euterpe-smb/src/lib.rs`

- [x] Add `SmbWatchEvent { path, action }`.
- [x] Add `SmbWatchStatus { connected, degraded_reason }`.
- [x] Add `watch_directory(location, credentials, recursive) -> Stream<Item = Result<SmbWatchEvent>>`.
- [x] Map `FileNotifyInformation` actions to stable enum: created, removed, modified, renamed_old, renamed_new.
- [x] Unit test action mapping with constructed notify structs.
- [x] Add ignored integration test gated by `EUTERPE_TEST_SMB_*`.

## Task 2: Server watcher service

**Files:**
- Create: `crates/euterpe-server/src/services/storage_watch.rs`
- Modify: `crates/euterpe-server/src/app.rs`
- Modify: `crates/euterpe-server/src/state.rs`

- [x] Add `StorageWatchHandle` with status in `RwLock`.
- [x] On app startup, if Settings library is SMB, start watcher task.
- [x] On Settings storage patch, restart watcher task.
- [x] Reconnect with exponential backoff: 1s, 2s, 5s, 10s, max 60s.
- [x] Status states: `disabled`, `connected`, `degraded`, `reconnecting`.

## Task 3: Debounce and scan scheduling

**Files:**
- Modify: `crates/euterpe-server/src/services/storage_watch.rs`
- Modify: `crates/euterpe-server/src/services/library_scan.rs`

- [x] Debounce window default `1500ms`.
- [x] Coalesce events by top-level album directory when possible.
- [x] If path cannot map safely, schedule full storage scan.
- [x] Do not start scan if `library_scan_runs::has_running`; record pending rescan and run once current scan finishes.

## Task 4: Status API/UI

**Files:**
- Modify: `crates/euterpe-server/src/api/server.rs` or settings API schema
- Modify: `openapi/openapi.yaml`
- Modify: `frontend/src/features/settings/StorageSettingsSection.tsx`

- [x] Expose `library_storage.watch_status`.
- [x] UI shows connected/degraded/reconnecting state near selected folder.
- [x] Degraded text must include reason but not credentials.

## Acceptance Criteria

- SMB server file changes enqueue library rescan without manual scan.
- Unsupported notify does not silently pretend to work; status is degraded.
- Watcher restarts after Settings SMB location changes.
