# Сравнение референсных реализаций

## Проекты

| Repo | Stars | Фокус |
|------|-------|-------|
| [qobuz-dl](https://github.com/vitiko98/qobuz-dl) | ~2.2k | CLI download, qopy client |
| [streamrip](https://github.com/nathom/streamrip) | ~4.6k | Async multi-service, QobuzSpoofer |
| [qobuz-sync](https://github.com/trevorstarick/qobuz-sync) | small | Go, favorites → filesystem |

## Локальные клоны (`docs/references/<имя>/`)

Каталог `docs/references/` в `.gitignore` — в репозитории его нет, но при разработке туда кладут зеркала исходников. Ниже — **куда смотреть** внутри клона (пути от корня клона).

| Клон | Auth / токены | Bundle / API base |
|------|----------------|-------------------|
| `qobuz-dl` | `qobuz_dl/qopy.py` (`user/login`, `X-User-Auth-Token`) | `qobuz_dl/bundle.py` |
| `streamrip` | `streamrip/client/qobuz.py` (`login`, `use_auth_token`, `user/login`) | `QobuzSpoofer` в том же файле |
| `qobuz-sync` | `client/client.go` (`Login`, `AUTH_TOKEN`, заголовки) | `getBundleURL` / `getSecrets` в `client.go` |
| `qobuz-qt` | `rust/src/api/client.rs` (`oauth2/login`), `rust/src/api/models.rs` (`OAuthLoginResponse`) | см. клиент: `BASE_URL`, bundle при необходимости в Qt-слое |
| `qobuz-dl-go` | **Добавить клон** — браузерный OAuth, redirect, обмен `code` | обычно рядом с OAuth — тот же клиент к `api.json` |

Подробнее по OAuth и роли каждого клона: [oauth-and-tokens.ru.md](oauth-and-tokens.ru.md).

## Файлы для портирования в Rust

| Concern | qobuz-dl | streamrip | qobuz-sync |
|---------|----------|-----------|------------|
| Bundle parse | `bundle.py` | `QobuzSpoofer` in `client/qobuz.py` | `client.go` getSecrets |
| HTTP client | `qopy.py` | `QobuzClient` | `Querier[T]` |
| Login | `qopy.auth` | `login()` | `Login()` |
| Favorites | `get_favorite_*` | `get_user_favorites` | `FavoriteGetUserFavorites` |
| File URL | `get_track_url` | `_request_file_url` | `TrackGetFileURL` |
| Download bytes | `downloader.py` | `downloadable.py` | album/track download |

Euterpe **не** копирует downloader целиком на Phase 1 — только URL API в `euterpe-qobuz`.

## Таблица расхождений

| Тема | qobuz-dl | streamrip | qobuz-sync |
|------|----------|-----------|------------|
| HTTP lib | requests | aiohttp | net/http |
| Async | sync | async | sync |
| Favorites sig | В `api_call`: ts+secret; методы list — без явного sec | Без sig | ts only, no secret |
| Probe track_id | 5966783 | 19512574 | 5966783 |
| Quality CLI | format_id напрямую | 1–4 → map | `TrackFormat` enum |
| Auth (2026) | email/password broken | `use_auth_token` | `AUTH_TOKEN` env primary |
| OAuth CLI | нет (в типичном клоне; см. PR #331) | нет | qobuz-dl-go (клон в `docs/references/qobuz-dl-go`) |
| Pagination | `multi_meta` generator | `_paginate` + gather | playlist loop |
| Create/delete fav | нет | нет | нет |

## bundle.js timezone order

streamrip документирует: **второй** timezone block должен быть первым после reorder. qobuz-dl `OrderedDict.move_to_end(keypairs[1][0], last=False)`. qobuz-sync итерирует seeds map без явного reorder — риск расхождения; TDD с live bundle fixture.

## Что брать за основу для euterpe-qobuz

| Модуль | Primary | Secondary |
|--------|---------|-----------|
| `bundle.rs` | streamrip | qobuz-dl |
| `signing.rs` | qopy + qobuz-sync | — |
| `pagination.rs` | streamrip | qobuz-sync playlist |
| `models/` | qobuz-sync (typed) | serde_json Value fallback |
| `FavoritesSignMode` | все три | auto-fallback |

## Лицензии и атрибуция

В README crate указать inspiration: Qo-DL Reborn, Sorrow446, DashLt, vitiko98, nathom, trevorstarick. Код писать заново, не копипаста Python/Go.

## Миграция auth (апрель 2026+)

По [qobuz-dl #329](https://github.com/vitiko98/qobuz-dl/issues/329):

- Qobuz убрал рабочий email/password API login → OAuth на сайте
- Обход: UAT в config; streamrip `use_auth_token = true`; qobuz-sync `AUTH_TOKEN`
- qobuz-dl-go (локально `docs/references/qobuz-dl-go`): OAuth CLI + manual token; password login в вебе часто непригоден для автоматизации

**Euterpe** следует qobuz-sync + streamrip token path; OAuth UI — FP-1 (сверка URL с исходниками qobuz-dl-go в `docs/references/`).

## Legal

Все три проекта содержат disclaimer: educational use, user responsibility, API ToS.
