# Справочник endpoints (этап 1)

Base: `https://www.qobuz.com/api.json/0.2/{endpoint}`

Метод: **GET** unless noted.

## Auth

### user/login

> **2026:** предпочтительен **token flow**. Email/password — deprecated (частые 401).

| Param | Required | Status |
|-------|----------|--------|
| `user_id`, `user_auth_token`, `app_id` | token flow | **Recommended** |
| `email`, `password`, `app_id` | legacy | **Deprecated** |

Метод в старых клиентах: **GET**. Веб-клиент Qobuz может использовать **POST** + OAuth body (не специфицировано для `euterpe-qobuz` M1).

**Response fields:** `user_auth_token`, `user`, `credential`

### Session without login

Если UAT уже известен — достаточно headers `X-App-Id` + `X-User-Auth-Token` без вызова `user/login` (см. qobuz-sync `AUTH_TOKEN`).

## Catalog

### album/get

| Param | Required |
|-------|----------|
| `album_id` | yes |

Returns album metadata + `tracks.items[]`.

### track/get

| Param | Required |
|-------|----------|
| `track_id` | yes |

### artist/get

| Param | Required |
|-------|----------|
| `artist_id` | yes |
| `extra` | `albums` for discography |
| `limit`, `offset` | pagination |

### playlist/get

| Param | Required |
|-------|----------|
| `playlist_id` | yes |
| `extra` | `tracks` |
| `limit`, `offset` | pagination |

### label/get

| Param | Required |
|-------|----------|
| `label_id` | yes |
| `extra` | `albums` |

## Search (Phase 2)

### album/search, track/search, artist/search, playlist/search

| Param | Required |
|-------|----------|
| `query` | yes |
| `limit` | no |

## Favorites

### favorite/getUserFavorites

| Param | Required |
|-------|----------|
| `type` | `albums` \| `tracks` \| `artists` |
| `limit`, `offset` | no |

Signing: [request-signing.ru.md](request-signing.ru.md)

### favorite/create

| Param | Required |
|-------|----------|
| `album_ids` | one of |
| `track_ids` | one of |
| `artist_ids` | one of |

### favorite/delete

Same params as create.

## Streaming / download URL

### track/getFileUrl

| Param | Required |
|-------|----------|
| `track_id` | yes |
| `format_id` | 5, 6, 7, 27 |
| `intent` | `stream` |
| `request_ts` | yes |
| `request_sig` | MD5 hex |

**Response:** `url` (temporary), `format_id`, optional `restrictions`

## Playlists user

### playlist/getUserPlaylists

| Param | Required |
|-------|----------|
| `limit` | no |

## URL parsing (play.qobuz.com)

| Type | Pattern |
|------|---------|
| album | `https://play.qobuz.com/album/{slug}` or id |
| track | `.../track/{id}` |
| artist | `.../artist/{id}` |
| playlist | `.../playlist/{id}` |
| label | `.../label/{id}` |

Slug vs numeric id: API принимает **numeric id**; slug резолвить через search или redirect (Phase 2).

## Headers summary

| Header | Value |
|--------|-------|
| `X-App-Id` | app_id |
| `X-User-Auth-Token` | UAT after login |
| `User-Agent` | Browser-like |
