# Сравнение референсных реализаций

## Проекты

| Repo | Stars | Фокус |
|------|-------|-------|
| [qobuz-dl](https://github.com/vitiko98/qobuz-dl) | ~2.2k | CLI download, qopy client |
| [streamrip](https://github.com/nathom/streamrip) | ~4.6k | Async multi-service, QobuzSpoofer |
| [qobuz-sync](https://github.com/trevorstarick/qobuz-sync) | small | Go, favorites → filesystem |

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
| OAuth CLI | нет | нет | [qobuz-dl-go](https://github.com/Aeneaj/qobuz-dl-go) |
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
- [qobuz-dl-go](https://github.com/Aeneaj/qobuz-dl-go): OAuth CLI + manual token; **password login explicitly unsupported**

**Euterpe** следует qobuz-sync + streamrip token path; OAuth UI — Phase 2.

## Legal

Все три проекта содержат disclaimer: educational use, user responsibility, API ToS.
