<!-- Шаблон описания PR: структура как в типовых PR репозитория. Замени плейсхолдеры, удали неактуальные блоки. -->

## Summary

- **Backend:** …
- **Tags & files:** … *(если не трогалось — удалить строку)*
- **Frontend:** …
- **Docs:** … *(если не трогалось — удалить строку)*
- **Lint / CI:** … *(если не трогалось — удалить строку)*

## Test plan

- [ ] `cargo test -p euterpe-server`
- [ ] `npm run lint` and `npm run test` in `frontend/`
- [ ] Manual: … *(коротко: что проверить руками в UI/API)*

<details>
<summary>Пример заполнения (Phase 5 / PR #6) — можно скопировать и править</summary>

## Summary

- **Backend:** SQLite migration for library catalog (artists, albums, tracks, scan runs), repos and `library_scan` worker with SSE progress; OpenAPI + routes for scan, album/track listing, track tag `PATCH`, and album cover `GET`.
- **Tags & files:** `lofty`-based read/write in `library/tags.rs` with tests; cover download after Qobuz album job uses **`cover.<ext>`** from MIME, registers downloaded albums for favorites `in_library` join, and embeds art with correct `MimeType`.
- **Frontend:** `/library` UI (rescan, albums, track tag editor, authenticated cover preview), API client/hooks/schema updates, MSW + Vitest; layout nav link; small queue/quality tweaks.
- **Docs:** roadmap and future-plans (FP-4, FP-7–FP-9, references); README highlights.
- **Lint:** avoid synchronous `setState` inside effects in `LibraryAlbumCover` and `LibraryPage` (split fetch subcomponent + keyed tag form).

## Test plan

- [ ] `cargo test -p euterpe-server`
- [ ] `npm run lint` and `npm run test` in `frontend/`
- [ ] Manual: Library rescan, open album, edit tags and save; favorites `in_library` after download (if Qobuz env available)

</details>
