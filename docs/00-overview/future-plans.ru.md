# Планы на будущее

Документ фиксирует функции **вне текущих Phase 0–5**, но согласованные с архитектурой Euterpe. Реализация — **строгий TDD**, как и весь проект.

## FP-1 — Получение Qobuz-токена из приложения

### Проблема сейчас

На Phase 1–2 пользователь вручную копирует `user_id` и `user_auth_token` из DevTools play.qobuz.com или задаёт env в Docker ([oauth-and-tokens.ru.md](../05-qobuz/oauth-and-tokens.ru.md)).

### Цель

Подключить Qobuz **из UI Euterpe** (или wizard при первом запуске), получить токен через **OAuth redirect**, сохранить в БД — без ручной вставки в env.

### Пользовательский сценарий

1. Settings → «Подключить Qobuz»
2. Редирект на Qobuz OAuth (или встроенное окно / новая вкладка)
3. После успеха — callback на `https://<euterpe>/api/v1/qobuz/oauth/callback`
4. Сервер сохраняет учётную запись, показывает имя / тип подписки (Studio и т.д.)
5. Дальнейшие sync/download используют токен из БД

### Backend (черновик)

| Endpoint | Назначение |
|----------|------------|
| `GET /api/v1/qobuz/oauth/start` | URL для редиректа + `state` (CSRF) |
| `GET /api/v1/qobuz/oauth/callback` | Обмен code → UAT, запись в `qobuz_accounts` |
| `POST /api/v1/qobuz/accounts/:id/refresh` | Обновление UAT (если API поддерживает) |
| `DELETE /api/v1/qobuz/accounts/:id` | Отвязать аккаунт |

Референс реализации OAuth: [qobuz-dl-go](https://github.com/Aeneaj/qobuz-dl-go), [qobuz-dl PR #331](https://github.com/vitiko98/qobuz-dl/pull/331).

### Хранение в БД

Таблица `qobuz_accounts` (см. [sqlite-schema.ru.md](../02-backend/sqlite-schema.ru.md#qobuz_accounts-future)):

- `user_auth_token` — **только в зашифрованном виде** (ключ из env `EUTERPE_SECRETS_KEY` или file)
- `user_id`, `display_name`, `membership_label`
- `uat_obtained_at`, `uat_expires_at` (если удаётся распарсить JWT `exp`)
- `oauth_refresh_token` — optional, если появится в flow

**Не** хранить в `settings` plaintext; env `EUTERPE_QOBUZ_*` остаётся fallback для headless Docker.

### UI

- Settings: кнопка «Подключить», статус «подключено до …»
- Toast при истечении токена + кнопка «Переподключить»
- Phase 4+; зависит от Phase 2 server routes

### Milestones (TDD)

| ID | Scope |
|----|--------|
| FP-1a | OAuth start/callback + insert `qobuz_accounts` (mock Qobuz) |
| FP-1b | UI connect flow (Vitest + MSW) |
| FP-1c | Live OAuth test `#[ignore]` |
| FP-1d | Auto-refresh / уведомление об истечении |

**Целевая фаза:** **Phase 2b** (после базового API Phase 2) или начало **Phase 4** вместе с Settings UI.

---

## FP-2 — Выбор активного пользователя Qobuz

### Проблема

В доме может быть **несколько подписок** Qobuz (разные члены семьи) или тестовый + основной аккаунт. Сейчас предполагается один глобальный UAT.

### Цель

- Хранить **несколько** привязанных аккаунтов Qobuz
- Явно выбирать **активный** — от него идут sync, favorites, download jobs
- В UI видно, «чьё» избранное и очередь

### Модель данных

```sql
-- активный аккаунт
settings.key = 'qobuz.active_account_id'  → FK qobuz_accounts.id

-- опционально: привязка job к аккаунту
download_jobs.qobuz_account_id NOT NULL
qobuz_sync_runs.qobuz_account_id NOT NULL
qobuz_favorites.qobuz_account_id NOT NULL  -- избранное per account
```

При смене активного аккаунта:

- UI перезагружает favorites / queue для выбранного
- Фоновые jobs **не переключаются** mid-flight — только новые задачи

### API (черновик)

| Endpoint | Назначение |
|----------|------------|
| `GET /api/v1/qobuz/accounts` | Список привязанных аккаунтов (без UAT) |
| `POST /api/v1/qobuz/accounts/active` | `{ "account_id": 2 }` |
| `GET /api/v1/qobuz/accounts/active` | Текущий активный |

Все существующие routes (`/qobuz/sync`, `/qobuz/favorites`, `/downloads`) используют **active account**, если не передан заголовок `X-Euterpe-Qobuz-Account: <id>` (опционально для API power users).

### UI

- Header или Settings: **dropdown** «Qobuz: Имя (Studio)»
- При одном аккаунте — скрыть dropdown, показать badge
- «Добавить аккаунт» → FP-1 OAuth
- Удаление аккаунта с подтверждением

### Server state

```rust
// AppState: не один QobuzClient, а
QobuzSessionPool {
    get_client(account_id) -> Arc<QobuzClient>,
    active_account_id: RwLock<i64>,
}
```

Клиенты кэшируются в памяти; при обновлении UAT в БД — invalidate.

### Milestones (TDD)

| ID | Scope |
|----|--------|
| FP-2a | Migration `qobuz_accounts`, `active_account_id` |
| FP-2b | API list + set active; tests |
| FP-2c | Scope favorites/sync by `qobuz_account_id` |
| FP-2d | UI account switcher |

**Целевая фаза:** **Phase 2c** (после FP-1) или **Phase 4** вместе с Settings.

---

## FP-3 — Очередь загрузок: очистка и удаление заданий

### Проблема сейчас

На Phase 3–4 в UI есть **отмена** активного job (`DELETE /downloads/{id}` → `cancelled`), но нет:

- массовой уборки «старых» записей в списке;
- удаления конкретной строки из истории очереди (не путать с cancel running).

### Цель

1. **Полная очистка очереди** — одной операцией убрать все **завершённые/устаревшие** jobs, не трогая:
   - **новые** (`queued`);
   - **активные** (`running`).
   - Типично удаляются: `completed`, `failed`, `cancelled` (точный набор — зафиксировать в OpenAPI).

2. **Персональное удаление** — кнопка у строки: убрать **один** job из списка/БД (для finished jobs; для `queued`/`running` — либо запрет, либо сначала cancel).

### Backend (черновик)

| Endpoint | Назначение |
|----------|------------|
| `POST /api/v1/downloads/purge` | Удалить все jobs со статусом ∉ `{queued, running}`; ответ `{ "deleted": N }` |
| `DELETE /api/v1/downloads/{id}?purge=1` | Удалить запись job из БД (не cancel); **409** если `running` без предварительной отмены |

Альтернатива: отдельный `DELETE` только для terminal status; cancel остаётся как сейчас.

### UI (`/queue`)

- Toolbar: **«Очистить историю»** + confirm dialog
- Row action: **«Удалить»** (иконка) для terminal jobs; для running — **Cancel** как сейчас

### Milestones (TDD)

| ID | Scope |
|----|--------|
| FP-3a | OpenAPI + `download_jobs::purge_finished` + contract tests |
| FP-3b | `DELETE` purge single job + state rules |
| FP-3c | Queue UI: purge + per-row delete (Vitest + MSW) |

**Целевая фаза:** **Phase 4b** (доработка UI) / **Phase 3b** (API).

---

## FP-4 — Favorites: сортировка таблицы

### Проблема сейчас

Список избранного (`GET /api/v1/qobuz/favorites`) отображается в порядке БД/sync без сортировки по колонкам.

### Цель

Клиентская (или server-side) сортировка по:

| Колонка | Поле |
|---------|------|
| Title | `title` |
| Artist | `artist_name` |
| In library | `in_library` |

- Клик по заголовку колонки → asc/desc toggle (TanStack Table `getSortedRowModel`)
- Сохранение выбора в `sessionStorage` optional

### API (опционально)

Если понадобится серверная сортировка на больших списках:

`GET /api/v1/qobuz/favorites?sort=title&order=asc`

На первом этапе достаточно **client-side** sort по текущей странице.

### Milestones (TDD)

| ID | Scope |
|----|--------|
| FP-4a | Vitest: click Title header → rows reordered |
| FP-4b | Artist + In library sort |
| FP-4c | (optional) query params + SQL `ORDER BY` |

**Целевая фаза:** **Phase 4b** (только frontend) или **Phase 2b** при server-side sort.

---

## Связь с дорожной картой

| Функция | Рекомендуемая фаза |
|---------|-------------------|
| FP-1 OAuth in-app + DB | Phase 2b / 4 |
| FP-2 Multi-account + switch | Phase 2c / 4 |
| FP-3 Queue purge + delete job | Phase 3b / 4b |
| FP-4 Favorites column sort | Phase 4b |
| Ручной token / env | Phase 1–2 (остаётся) |

См. [roadmap.ru.md](roadmap.ru.md) — секция «Phase 6+ / Future».

## Не в scope (пока)

- Несколько **локальных** пользователей Euterpe (RBAC) — отдельная тема; FP-2 только про **аккаунты Qobuz** на одном инстансе
- Синхронизация избранного **между** двумя Qobuz-аккаунтами
- Публичный multi-tenant SaaS
