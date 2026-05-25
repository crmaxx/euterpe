# SMB Torrent Import and Copy Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Torrent incoming остаётся local/env-managed, но copy/import destination должен быть configured library storage, включая SMB.

**Architecture:** Source side remains local `torrent_incoming_dir`; destination side uses `LibraryStorage`. Copy streams local files into remote storage with atomic writes. Directory naming/conflict handling uses storage metadata, not local `PathBuf`.

**Tech Stack:** Rust, `tokio::fs` for local torrent source, `LibraryStorage` for library destination.

---

## Task 1: Destination path allocation over storage

**Files:**
- Modify: `crates/euterpe-server/src/services/torrent_import.rs`
- Test: `crates/euterpe-server/src/services/torrent_import.rs`

- [x] Add `unique_library_dest_storage(storage, display_name) -> StoragePath`.
- [x] Preserve existing naming: sanitized display name; conflicts append ` (n)`.
- [x] Use `storage.metadata` to detect conflicts.
- [x] Unit test with `LocalStorage` backend: existing `Album`, new path is `Album (2)`.

## Task 2: Recursive copy local source to storage destination

**Files:**
- Modify: `crates/euterpe-server/src/services/torrent_import.rs`

- [x] Add `copy_local_tree_to_storage(source_dir, storage, dest_root)`.
- [x] For files: read local file in bounded chunks or full bytes for first iteration, then `storage.atomic_write`.
- [x] For directories: `storage.create_dir_all`.
- [x] Ignore partial hidden files from torrent engine only if existing local import already ignores them; otherwise copy exact tree.
- [x] Return destination relative root string.

## Task 3: Torrent job integration

**Files:**
- Modify: `crates/euterpe-server/src/services/download/torrent_job.rs`
- Test: `crates/euterpe-server/tests/api_torrent*.rs` or service test file if present

- [x] Replace `require_local_library_path` in copy-to-library branch with `state/deps` storage resolution.
- [x] Call `copy_to_library_storage`.
- [x] Start `start_scan_storage` for copied subtree.
- [x] Keep local incoming cleanup unchanged.

## Task 4: CUE/convert inside torrent job

**Files:**
- Modify: `crates/euterpe-server/src/services/download/torrent_job.rs`

- [x] Remove path-only post-import CUE split from torrent job.
- [x] After copy, trigger normal library scan; let CUE and converter routes/jobs operate on storage.
- [x] If torrent payload currently requests immediate split/convert, map it to queued storage-native jobs after Tasks `cue-split.md` and `converter-worker.md`.

## Acceptance Criteria

- Torrent incoming dir still uses env/docker local path.
- `copy_to_library=true` works when library storage is SMB.
- No copied data goes through `/data` except the torrent incoming source itself.
