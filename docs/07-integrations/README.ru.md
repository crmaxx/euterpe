# Интеграции (tag sources)

Реестр внешних каталогов для автозаполнения тегов и обложки альбома из UI библиотеки.

## Настройка

**Settings → Integrations** — добавить провайдер из каталога, указать поля конфигурации.

| Провайдер | Поля | Секреты |
|-----------|------|---------|
| MusicBrainz | `contact` (email для User-Agent) | — |
| Discogs | — | `token` (требует `EUTERPE_MASTER_KEY`) |
| GnuDB | `server_base` (опционально) | — |
| TrackType.org | `api_base` (опционально) | `api_key` (опционально) |

## Library

В модалке **Edit tags** (контекст альбома): кнопка **Autofill** с выбором провайдера (split button), затем список кандидатов → **Apply**.

Поиск в каталогах использует **имя артиста, альбома и трека из пути файла** на диске (`Artist/2020 - Album/01 - Title.flac`), а не только поля из SQLite.

API:

- `POST /api/v1/library/albums/{id}/metadata/lookup`
- `POST /api/v1/library/albums/{id}/metadata/apply`

## Ограничения

- GnuDB и TrackType.org зависят от доступности внешних HTTP API; при недоступности — `PROVIDER_UNAVAILABLE`.
- Discogs: обложки скачиваются на диск; соблюдайте [Discogs API Terms](https://www.discogs.com/developers).
