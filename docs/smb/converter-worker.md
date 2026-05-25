# SMB Converter Worker Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Перевести converter worker с path-only `convert_file(&Path, ...)` на storage-native conversion for SMB.

**Architecture:** Разделить converter crate на pure conversion pipeline и filesystem adapter. Server worker получает bytes/readers из `LibraryStorage`, пишет output через `storage.atomic_write`, source policy выполняет `storage.delete`.

**Tech Stack:** Rust, `euterpe-converter`, FLAC encoder, WavPack/ALAC bindings, `LibraryStorage`.

---

## Task 1: Converter crate I/O boundary

**Files:**
- Modify: `crates/euterpe-converter/src/convert.rs`
- Modify: `crates/euterpe-converter/src/lib.rs`
- Test: `crates/euterpe-converter/src/lib.rs`

- [x] Добавить `ConvertInput { rel_path, bytes }`.
- [x] Добавить `ConvertOutput { rel_path, bytes, source_delete_rel }`.
- [x] Добавить `convert_bytes(input, options) -> Result<ConvertOutput>`.
- [x] `convert_file` оставить как filesystem adapter: read file -> `convert_bytes` -> write file/delete source.
- [x] Tests for WAV->FLAC bytes conversion.

## Task 2: Format-specific adapters

**Files:**
- Modify: `crates/euterpe-converter/src/source/wav.rs`
- Modify: `crates/euterpe-converter/src/source/alac.rs`
- Modify: `crates/euterpe-converter/src/source/wavpack.rs`
- Modify: `crates/euterpe-converter/src/source/ape.rs`

- [x] WAV: use reader/cursor API.
- [x] ALAC/M4A: use `Cursor<Vec<u8>>` if decoder supports `Read + Seek`; otherwise add bytes-backed adapter.
- [x] APE: same reader/cursor check.
- [x] WavPack: if binding is path-only, add callback/memory adapter or switch binding; do not write disk temp.
- [x] For unsupported native format return `CONVERTER_NATIVE_IO_UNSUPPORTED:<format>` and cover the explicit error until adapter lands.

Note: WAV is fully native. ALAC, APE, and WavPack currently return explicit `CONVERTER_NATIVE_IO_UNSUPPORTED:*` from `convert_bytes`; no disk temp bridge is used.

## Task 3: Server worker over storage

**Files:**
- Modify: `crates/euterpe-server/src/services/convert/worker.rs`
- Test: `crates/euterpe-server/src/services/convert/worker.rs`

- [x] Replace `require_local_library_path` with `state/deps` storage resolution.
- [x] For each convertible DB track: `storage.read(StoragePath::parse(track.path))`.
- [x] Call `euterpe_converter::convert_bytes`.
- [x] Write output with `storage.atomic_write`.
- [x] Delete source via `storage.delete` only after output write succeeds.
- [x] Update DB path/file_size for converted track.

## Task 4: Auto-convert after SMB scan

**Files:**
- Modify: `crates/euterpe-server/src/services/library_scan.rs`
- Test: `crates/euterpe-server/src/services/library_scan.rs`

- [x] Ensure storage scan enqueues convert jobs using DB relative paths only.
- [x] Add test where storage scan indexes `.wav`, auto converter setting on, convert job queued.

## Acceptance Criteria

- Converter worker has no `require_local_library_path`.
- No converter code writes to `/data` or local temp for SMB.
- WAV native conversion passes; ALAC/APE/WavPack either pass or have explicit unsupported error covered by tests.
