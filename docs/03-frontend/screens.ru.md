# Экраны UI (Phase 4 / Iteration 1 scope)

Каждый экран: список **тестов** перед реализацией (Vitest).

## Settings (`/settings`)

- **Phase 2 (interim):** вставка `user_id` + token → сохранение в БД
- **FP-1 (будущее):** кнопка «Подключить Qobuz» → OAuth в приложении, токен в `qobuz_accounts` без DevTools
- **FP-2 (будущее):** dropdown «Аккаунт Qobuz» — выбор активного пользователя; «Добавить аккаунт»
- Инструкция fallback: [oauth-and-tokens.ru.md](../05-qobuz/oauth-and-tokens.ru.md)
- Test connection → `POST /api/v1/qobuz/test-login`
- Default quality: select 5/6/7/27
- Library path (read-only display from server)

**Tests:** form validation; test connection success/error toast.

## Favorites (`/favorites`)

- Table: title, artist, in library?, actions
- Toolbar: Sync now, Add by URL/search (Phase 2)
- Row: Download, Remove from Qobuz favorites, Add to favorites
- Bulk select + bulk download queue
- **FP-4 (будущее):** сортировка по Title, Artist, In library (клик по заголовку)

**Tests:** table renders mock data; sync mutation invalidates query.

## Queue (`/queue`)

- Jobs: type, title, status, progress bar, cancel
- Live updates via SSE or polling
- **FP-3 (будущее):** «Очистить историю» (все terminal jobs, не queued/running); удаление одной строки из очереди

**Tests:** progress bar at 50% when event received.

## Library (`/library`) — placeholder Phase 1 docs

- Message: «Full library browser — Phase 5»
- Link to favorites

## Layout

- Sidebar: Favorites, Queue, Library, Settings
- Header: sync status last run

## shadcn components

- `Button`, `Table`, `Dialog`, `Progress`, `Toast`, `Input`, `Select`
