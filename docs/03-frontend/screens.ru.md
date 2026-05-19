# Экраны UI (Phase 4 / Iteration 1 scope)

Каждый экран: список **тестов** перед реализацией (Vitest).

## Settings (`/settings`)

- **Phase 2 (interim):** вставка `user_id` + token → сохранение в БД
- **FP-1 (будущее):** кнопка «Подключить Qobuz» → OAuth в приложении, токен в `qobuz_accounts` без DevTools
- **FP-10 (будущее):** dropdown «Аккаунт Qobuz» — выбор активного пользователя; «Добавить аккаунт»
- Инструкция fallback: [oauth-and-tokens.ru.md](../05-qobuz/oauth-and-tokens.ru.md)
- Test connection → `POST /api/v1/qobuz/test-login`
- Default quality: select 5/6/7/27
- Library path (read-only display from server)

**Tests:** form validation; test connection success/error toast.

## Favorites (`/favorites`)

- Table: title, artist, in library?, actions
- Toolbar: Sync now, Add by URL/search (Phase 2)
- Row: **Download** / **Re-download** (`in_library`); кнопка блокируется на строке до terminal job (`Downloading…` + spinner)
- Bulk select + bulk download queue
- **FP-3 (будущее):** сортировка по Title, Artist, In library (клик по заголовку)

**Tests:** table renders mock data; sync mutation invalidates query.

## Queue (`/queue`)

- Jobs: type, title, status, progress bar, cancel
- Live updates via SSE or polling
- **FP-2 ✅:** «Очистить историю» (все terminal jobs, не queued/running); удаление одной строки из очереди

**Tests:** progress bar at 50% when event received.

## Library (`/library`) — Phase 5

- **Rebuild index** (outline) → `POST /api/v1/library/scan`; **Repair folder** на альбоме → `?root=<album dir>`
- **Cancel scan** при `running` → `DELETE /api/v1/library/scan/{id}`
- Album list + track list; превью обложки по `GET /api/v1/library/albums/{id}/cover` (плейсхолдер **No cover**, если нет пути или файла)
- Редактирование **текстовых** тегов трека → `PATCH /api/v1/library/tracks/{id}`
- Header shows last scan status
- **Replace cover** — `PUT /api/v1/library/albums/{id}/cover` (JPEG/PNG/WebP/BMP, до 20 MiB); после Qobuz-download обложка также пишется на диск и встраивается в треки

**Tests:** album list; rebuild index; cancel scan; repair folder query.

**Layout:** после завершения album download — авто-invalidate Library/Favorites (`useInvalidateLibraryOnDownloadComplete`).

## Layout

- Sidebar: Favorites, Queue, Library, Settings
- Header: sync status last run

## shadcn components

- `Button`, `Table`, `Dialog`, `Progress`, `Toast`, `Input`, `Select`
