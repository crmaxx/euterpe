# Пагинация

## Общий паттерн

List endpoints возвращают обёртку:

```json
{
  "<resource>s": {
    "total": 1234,
    "limit": 500,
    "offset": 0,
    "items": [ ... ]
  }
}
```

Имя ключа = первый сегмент endpoint + `s` (`album/get` → не применимо; `artist/get` extra=albums → `albums`).

## Алгоритм fetch_all

```
offset = 0
limit = 500  // или из первого ответа
all_items = []
loop:
  page = api(epoint, offset, limit, ...)
  items = page[key].items
  all_items.extend(items)
  if offset + limit >= page[key].total:
    break
  offset += limit
```

## Endpoints с пагинацией (этап 1+)

| Endpoint | Ключ items | Примечание |
|----------|------------|------------|
| `favorite/getUserFavorites` | `albums` / `tracks` / `artists` | |
| `artist/get` + `extra=albums` | `albums` | `albums_count` |
| `playlist/get` + `extra=tracks` | `tracks` | qobuz-sync мержит в один список |
| `label/get` + `extra=albums` | `albums` | |
| `*/search` | `albums`, `tracks`, … | Phase 2 UI |

## Параллельные запросы

streamrip после первой страницы шлёт `asyncio.gather` для остальных offset. `euterpe-qobuz` может:

- Последовательно (проще, rate limit)
- Параллельно с `FuturesUnordered` + semaphore (Phase 5 / config)

## rate limit

streamrip: `requests_per_minute` в config. Euterpe server: default 30 req/min к Qobuz, tunable.

## TDD

Fixture: 3 страницы по 2 items, total=5 → `fetch_all` length 5.
