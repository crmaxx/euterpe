# SMB Cover Upload and Embed Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Сделать upload/read/embed обложек storage-native, включая SMB album directories.

**Architecture:** Разделить cover file operations и cover image processing. Файловые операции идут через `LibraryStorage`; image validation/resize остаётся в `covers.rs`. Embedded cover в audio tags использует storage-native tag writer из `tag-write.md`.

**Tech Stack:** Rust, `image`, `LibraryStorage`, `lofty`, Axum multipart/body routes.

---

## Task 1: Cover file primitives over storage

**Files:**
- Modify: `crates/euterpe-server/src/library/covers.rs`
- Test: `crates/euterpe-server/src/library/covers.rs`

- [x] Добавить `CoverStorageWriteInput { album_rel, bytes, content_type }`.
- [x] Добавить `write_album_cover_file_storage(storage, input) -> CoverWriteResult`.
- [x] Имя файла выбирать так же, как сейчас: normalized `cover.<ext>`, где ext из detected image type.
- [x] Писать через `storage.atomic_write(StoragePath::parse(format!("{album_rel}/cover.{ext}")))`.
- [x] Добавить unit test на local backend: upload PNG bytes -> появляется `cover.png`, path relative.

## Task 2: Cover discovery without local read_dir

**Files:**
- Modify: `crates/euterpe-server/src/library/covers.rs`
- Modify: `crates/euterpe-server/src/routes/library.rs`

- [x] Вынести текущий route helper `discover_album_cover_rel_storage` из `routes/library.rs` в `covers.rs`.
- [x] Поддержать priority: `cover.jpg/jpeg/png/webp/bmp`, затем album-title-like image, затем первый supported image.
- [x] Для SMB не делать rename during discovery; rename album-title image to cover оставить только для local или отдельной explicit write operation.

## Task 3: Upload route for SMB

**Files:**
- Modify: `crates/euterpe-server/src/routes/library.rs`
- Test: `crates/euterpe-server/tests/api_library.rs`

- [x] Перевести `put_library_album_cover` на `state.library_storage()`.
- [x] Для local оставить existing embed path через storage-compatible wrapper.
- [x] Для SMB: write cover file, update `albums.cover_path`, then embed into each track using storage-native tag writer.
- [x] Если embed в конкретный track failed, не откатывать cover file; вернуть `tracks_embedded` с количеством успешных embeds и warning log.
- [x] Добавить API test: upload cover with configured local storage via Settings, assert DB cover_path and GET cover works.

## Task 4: Download cover after Qobuz download

**Files:**
- Modify: `crates/euterpe-server/src/library/covers.rs`
- Modify: `crates/euterpe-server/src/services/download/worker.rs`

- [x] Добавить `apply_album_cover_after_download_storage(http, pool, storage, album, quality, qobuz_album_id)`.
- [x] Скачать cover bytes как сейчас, записать через storage primitive.
- [x] Embed через storage-native tag writer.
- [x] В download worker убрать SMB deferred log.

## Acceptance Criteria

- `GET /library/albums/{id}/cover` и `PUT /library/albums/{id}/cover` работают при SMB library.
- Qobuz download в SMB сохраняет cover file и обновляет DB.
- No `/data` temp files.
