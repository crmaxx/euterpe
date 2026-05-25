# SMB Tag Write Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Сделать чтение и запись тегов storage-native, чтобы `PATCH /library/albums/{id}/tags` и `PATCH /library/tracks/{id}/tags` работали для SMB без локального temp bridge.

**Architecture:** Вынести path-only `lofty::save_to_path` за storage-aware API. Для чтения использовать bytes/reader path, уже начатый в `library/tags.rs`; для записи использовать in-memory rewrite через `Cursor<Vec<u8>>` и `storage.atomic_write()` как первый native вариант. Для больших файлов добавить explicit лимит и понятную ошибку до появления streaming writer.

**Tech Stack:** Rust, `lofty`, `LibraryStorage`, `StoragePath`, Axum route tests.

---

## Task 1: Storage-native tag API

**Files:**
- Modify: `crates/euterpe-server/src/library/tags.rs`
- Modify: `crates/euterpe-server/src/library/storage.rs`
- Test: `crates/euterpe-server/src/library/tags.rs`

- [x] Добавить тест `write_tags_to_bytes_round_trips_wav`: создать WAV bytes, прочитать tags через `read_tags_from_bytes_with_rel`, применить patch, записать через новый `write_tags_to_bytes`, снова прочитать bytes.
- [x] Реализовать `pub fn write_tags_to_bytes(bytes: Vec<u8>, display_path: &str, tags: &TrackTags) -> Result<Vec<u8>, ApiError>`.
- [x] Использовать `Probe::new(Cursor<Vec<u8>>)` + `AudioFile::save_to(&mut cursor, WriteOptions::default())`.
- [x] Сохранить существующий `write_tags(path, tags)` как local wrapper.
- [x] Прогнать `cargo test -p euterpe-server library::tags::tests --lib`.

## Task 2: Storage tag service

**Files:**
- Modify: `crates/euterpe-server/src/library/tags.rs`
- Modify: `crates/euterpe-server/src/routes/library.rs`

- [x] Добавить `pub async fn read_tags_storage(storage: &dyn LibraryStorage, path: &StoragePath)`.
- [x] Добавить `pub async fn write_tags_storage(storage: &dyn LibraryStorage, path: &StoragePath, tags: &TrackTags)`.
- [x] В `write_tags_storage` читать bytes из storage, проверять лимит `EUTERPE_STORAGE_TAG_REWRITE_MAX_BYTES` с default `536870912`, писать через `storage.atomic_write`.
- [x] Ошибка при превышении лимита: `STORAGE_TAG_REWRITE_TOO_LARGE`.
- [x] Для local backend оставить старый path route только как optimization; основная логика route должна работать через storage helpers.

## Task 3: Album/track patch routes

**Files:**
- Modify: `crates/euterpe-server/src/routes/library.rs`
- Test: `crates/euterpe-server/tests/api_library.rs`

- [x] Перевести `patch_library_track_tags` на `state.library_storage()` + `StoragePath::parse(track.path)`.
- [x] Перевести `patch_library_album_tags` на storage loop по `track_rows`.
- [x] После записи metadata получать через `storage.metadata`, `file_mtime` для SMB оставлять `None`, `file_size` обновлять отдельным follow-up если schema path есть.
- [x] Добавить API test с fake/local storage через Settings, чтобы route не вызывает `state.require_local_library_path()`.
- [x] Прогнать `cargo test -p euterpe-server --test api_library patch_library_track_tags`.

## Acceptance Criteria

- `rg "require_local_library_path" crates/euterpe-server/src/routes/library.rs` не показывает tag patch routes.
- SMB storage может обновить tags через bytes rewrite + remote atomic write.
- Local tag tests и API tag patch tests проходят.

