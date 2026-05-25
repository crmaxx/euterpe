# SMB CUE Split Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Перевести CUE load/validate/split на storage-native I/O, чтобы split FLAC image работал на SMB без локального mount/temp bridge.

**Architecture:** Сначала сделать storage-aware CUE document loading in server. Затем расширить `euterpe-cue`: path-only `split_flac_image` оставить, но добавить reader/writer API, который принимает source bytes/reader и callback для atomic output writes.

**Tech Stack:** Rust, `euterpe-cue`, `LibraryStorage`, FLAC decoder/encoder used by current cue crate.

---

## Task 1: Storage-native CUE load

**Files:**
- Modify: `crates/euterpe-server/src/library/cue.rs`
- Modify: `crates/euterpe-server/src/routes/library.rs`
- Test: `crates/euterpe-server/tests/api_cue.rs`

- [x] Добавить `album_has_cue_files_storage(storage, album_rel)`.
- [x] Добавить `load_album_cue_storage(storage, album_rel, cue_path)`.
- [x] Читать `.cue` через `storage.read(StoragePath)`.
- [x] В route `get_library_album_cue` выбирать storage implementation вместо `require_local_library_path`.
- [x] API test: configured local storage through Settings, CUE load route works without `config.library_path`.

## Task 2: euterpe-cue storage split API

**Files:**
- Modify: `crates/euterpe-cue/src/lib.rs`
- Test: `crates/euterpe-cue/src/lib.rs`

- [x] Добавить `SplitIo` trait: `read_source(audio_path) -> Vec<u8>`, `write_output(rel_path, bytes)`, `delete_source(rel_path)`.
- [x] Добавить `split_flac_image_io(document, io, output_dir_rel, options)`.
- [x] Существующий `split_flac_image(&Path, &Path)` переписать как adapter над local `SplitIo`.
- [x] Unit test: in-memory `SplitIo` receives expected output paths for tiny fixture.

## Task 3: Server CUE split job over storage

**Files:**
- Modify: `crates/euterpe-server/src/routes/library.rs`
- Test: `crates/euterpe-server/tests/api_cue.rs`

- [x] В `split_library_album_cue` сохранять payload только с relative paths, не `cue_abs`.
- [x] `run_cue_split_job` получает `state.library_storage()`.
- [x] Реализовать `StorageSplitIo` over `LibraryStorage`.
- [x] Output writes use `storage.atomic_write`.
- [x] `delete_after_success` удаляет source audio and cue через `storage.delete`.
- [x] После split запускать `start_scan_storage` для album subtree.

## Acceptance Criteria

- CUE load/split не вызывает `require_local_library_path`.
- SMB split пишет tracks прямо в SMB share.
- Source delete policy работает через storage.
