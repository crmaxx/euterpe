# Подпись запросов (request_sig)

Некоторые endpoints требуют параметры `request_ts` (Unix timestamp, секунды) и `request_sig` (MD5 hex).

**MD5 здесь — не криптостойкость**, а обфускация API Qobuz. В Rust использовать crate `md-5` только для этих строк.

## track/getFileUrl

### Строка для подписи

Конкатенация **без разделителей**:

```
trackgetFileUrlformat_id{format_id}intentstreamtrack_id{track_id}{request_ts}{secret}
```

Пример (псевдокод):

```
format_id = 6
track_id  = 12345678
request_ts = 1715900000
secret = "<decoded app secret>"

raw = "trackgetFileUrlformat_id6intentstreamtrack_id123456781715900000<secret>"
request_sig = md5_hex(raw)
```

### Query parameters

| Param | Описание |
|-------|----------|
| `track_id` | ID трека |
| `format_id` | 5, 6, 7 или 27 |
| `intent` | `stream` |
| `request_ts` | Unix time (float/int; референсы используют `time.time()`) |
| `request_sig` | MD5 hex lowercase |

### Заголовки

Требуются `X-App-Id` и `X-User-Auth-Token` (после login).

### Ошибки

- **400** + invalid secret message → `InvalidAppSecretError`
- Ответ без `url`, есть `restrictions[]` → регион/права (см. streamrip NonStreamableError)

## favorite/getUserFavorites

Референсы **расходятся**. `euterpe-qobuz` должен поддержать режимы и выбрать рабочий через TDD + live test.

### Вариант A — qobuz-dl (qopy.py, legacy branch в api_call)

```
raw = "favoritegetUserFavorites" + str(request_ts) + secret
request_sig = md5_hex(raw)
```

Дополнительно в query: `app_id`, `user_auth_token`, `type`, `offset`, `limit`.

**Замечание:** публичные методы `get_favorite_*` в qopy передают только `type/offset/limit` — возможно опора на UAT без sig в актуальной ветке. Проверить live.

### Вариант B — qobuz-sync (Go)

```
raw = "favoritegetUserFavorites" + timestamp   // без secret
request_sig = md5_hex(raw)
```

Query: `limit`, `offset`, `type` (`albums`|`tracks`|`artists`), `request_ts`, `request_sig`.

### Вариант C — streamrip

Без `request_ts` / `request_sig`; только:

```
type=albums|tracks|artists  // множественное число
limit, offset
```

Пагинация через `_paginate`.

### Рекомендация для euterpe-qobuz

```rust
pub enum FavoritesSignMode {
    None,           // streamrip style
    TimestampOnly,  // qobuz-sync
    TimestampSecret // qopy api_call branch
}
```

Порядок попытки при 400: `TimestampSecret` → `TimestampOnly` → `None` (только в integration/debug).

## favorite/create и favorite/delete

По [LMS Qobuz plugin](https://github.com/LMS-Community/plugin-Qobuz): endpoints `favorite/create`, `favorite/delete` с `_use_token`.

Query (предположительно):

| Param | Описание |
|-------|----------|
| `album_ids` | Comma-separated IDs |
| `track_ids` | Comma-separated |
| `artist_ids` | Comma-separated |
| `app_id` | |
| `user_auth_token` | или header |

**TDD M3:** mock 200; затем live round-trip.

Подпись для create/delete — **не документирована** в qobuz-dl/streamrip; уточнить при реализации.

## Golden tests (обязательно)

Файл `tests/signing_golden.rs`:

- Вектор из qobuz-sync: timestamp fixed → ожидаемый sig для favorites
- Вектор из qopy: track getFileUrl

Фиксированные `request_ts` в тестах, не `time::now()`.
