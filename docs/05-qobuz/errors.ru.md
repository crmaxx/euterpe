# Ошибки Qobuz API

## HTTP статусы

| Code | Типичная причина | QobuzError variant |
|------|------------------|-------------------|
| 200 | OK | — |
| 400 | Bad app_id, bad sig, bad params | `InvalidAppId`, `InvalidSignature`, `BadRequest` |
| 401 | Expired or invalid UAT / bad credentials | `Authentication` — show token refresh guide |
| 403 | Rare | `Forbidden` |
| 404 | Unknown id | `NotFound` |
| 429 | Rate limit | `RateLimit` |
| 5xx | Qobuz outage | `Upstream` |

## Доменные ошибки (из референсов)

| Ошибка | Условие |
|--------|---------|
| `Ineligible` | Free account, empty `credential.parameters` |
| `InvalidAppSecret` | Ни один secret не прошёл probe `getFileUrl` |
| `InvalidQuality` | format_id не в {5,6,7,27} |
| `NonStreamable` | Нет `url`, есть `restrictions` |
| `Authentication` | login 401 |

## JSON error body

Структура не стабильна. Парсить опционально поле `message` / `code` для логов.

```rust
#[derive(Debug, Deserialize)]
struct ApiErrorBody {
    message: Option<String>,
    code: Option<i32>,
}
```

## restrictions (track/getFileUrl)

Пример:

```json
{
  "restrictions": [{ "code": "GeoRestricted" }]
}
```

streamrip разбивает CamelCase `code` в человекочитаемое сообщение.

## Retry policy

| Ошибка | Retry |
|--------|-------|
| 429, 5xx | Exponential backoff, max 3 |
| 401 | Refresh login once |
| 400 sig | Switch sign mode once (favorites) |
| Ineligible | No retry |

## TDD

- mockito: status 401 → `QobuzError::Authentication`
- deserialize fixture `restrictions_only.json`
