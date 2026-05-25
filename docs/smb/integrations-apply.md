# SMB Integrations Apply Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `integrations/apply` должен обновлять tags и covers через storage backend, чтобы Discogs/MusicBrainz-style apply работал для SMB.

**Architecture:** Сервис apply перестаёт принимать/использовать `config.library_path`. Он получает `LibraryStorage`, читает и пишет tags/covers через storage-native helpers из `tag-write.md` и `cover-upload-embed.md`.

**Tech Stack:** Rust, integration registry/apply services, `LibraryStorage`, storage-native tags/covers.

---

## Task 1: Apply service dependencies

**Files:**
- Modify: `crates/euterpe-server/src/integrations/apply.rs`
- Modify: callers in routes/services that invoke apply

- [x] Introduce `ApplyStorageDeps { storage: Arc<dyn LibraryStorage> }`.
- [x] Remove direct usage of `config.library_path.join`.
- [x] Keep path hint parsing based on DB relative paths.

## Task 2: Track tag updates

**Files:**
- Modify: `crates/euterpe-server/src/integrations/apply.rs`
- Test: `crates/euterpe-server/src/integrations/apply.rs`

- [x] Replace `tags::write_tags(&file_path, &updated)` with `tags::write_tags_storage(storage, StoragePath::parse(db_track.path), &updated)`.
- [x] Add unit/service test using `LocalStorage` root and DB rows.
- [x] Error per missing file: fail apply with explicit `INTEGRATION_TRACK_FILE_NOT_FOUND:<path>`.

## Task 3: Cover apply

**Files:**
- Modify: `crates/euterpe-server/src/integrations/apply.rs`

- [x] Replace `covers::write_album_cover_from_bytes` path API with storage API.
- [x] Update DB `albums.cover_path`.
- [x] Embed into tracks through storage tag writer.
- [x] If cover write succeeds but embed partially fails, persist cover and return/apply warning.

## Task 4: API behavior

**Files:**
- Modify: integration route file that calls apply
- Test: integration API tests if present

- [x] Resolve `state.library_storage()` at route boundary.
- [x] Return `LIBRARY_STORAGE_NOT_CONFIGURED` if missing.
- [x] Add test for local Settings storage to prove route no longer relies on `config.library_path`.

## Acceptance Criteria

- `rg "library_path" crates/euterpe-server/src/integrations/apply.rs` returns no library file writes.
- Integration apply works with SMB for tags and covers.
