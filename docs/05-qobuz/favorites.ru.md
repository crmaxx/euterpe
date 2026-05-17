# Избранное (Favorites)

## Endpoints

| Endpoint | Назначение |
|----------|------------|
| `favorite/getUserFavorites` | Список избранного |
| `favorite/create` | Добавить |
| `favorite/delete` | Удалить |

Все — **GET** с query parameters (как остальной Qobuz JSON API).

## getUserFavorites

### Query

| Param | Обязательный | Значение |
|-------|--------------|----------|
| `type` | да | `albums`, `tracks`, `artists` (множественное число) |
| `limit` | нет | default 50–500 в референсах |
| `offset` | нет | 0, 500, … |
| `app_id` | зависит от режима | |
| `user_auth_token` | зависит | или header UAT |
| `request_ts`, `request_sig` | зависит | см. [request-signing.ru.md](request-signing.ru.md) |

### Ответ (структура)

Корень содержит ключ по типу (`albums`, `tracks`, `artists`):

```json
{
  "albums": {
    "total": 120,
    "limit": 100,
    "offset": 0,
    "items": [ { "id": 123, "title": "...", "artist": { ... } } ]
  }
}
```

Для полной синхронизации — обход пока `offset + limit < total`.

### Euterpe sync semantics

1. `GET` все страницы favorites albums (и опционально tracks/artists).
2. Upsert в `qobuz_favorites` (`entity_type`, `qobuz_id`, `synced_at`, `removed=false`).
3. Записи в БД, отсутствующие в ответе — пометить `removed=true` или удалить (политика: soft-delete).
4. Запись в `qobuz_sync_runs` (started_at, finished_at, counts, error).

**TDD (server, Phase 2):** mock Qobuz client trait; тест diff logic без HTTP.

## favorite/create

Источник: [clj-qobuz](https://cljdoc.org/d/audiogum/clj-qobuz/0.1.15/api/clj-qobuz.favorite), LMS `API.pm`.

### Query (ожидаемые)

| Param | Формат |
|-------|--------|
| `album_ids` | `id1,id2` |
| `track_ids` | `id1,id2` |
| `artist_ids` | `id1,id2` |

Требуется авторизованный пользователь (UAT).

### Поведение Euterpe

- UI / API: «добавить в избранное Qobuz»
- После успеха — обновить локальную строку `qobuz_favorites` или триггер mini-sync

**TDD M3:** mock → assert query contains `album_ids=42`.

## favorite/delete

Аналогично create с теми же id-параметрами.

## qobuz-sync CLI

Команда `favorites` скачивает favorite albums/tracks в filesystem — другой use case. Euterpe Phase 1 фокус на **метаданные + sync state**, download — Phase 3.

## Ошибки

| Ситуация | Действие |
|----------|----------|
| 401 | Re-login |
| 400 + sig | Сменить `FavoritesSignMode` |
| Пустой items | OK, пустое избранное |

## Связь с локальной библиотекой

`qobuz_favorites.qobuz_id` JOIN `albums.qobuz_album_id` → UI badge «есть локально» / «только в Qobuz».
