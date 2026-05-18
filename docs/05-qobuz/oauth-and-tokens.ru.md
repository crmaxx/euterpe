# OAuth и auth-токены (актуально с 2025–2026)

## Локальные референсы (`docs/references/`)

Каталог `docs/references/` указан в `.gitignore`: в git не попадает, но на машине разработчика там лежат **клоны** сторонних репозиториев. По ним (а не по «памяти модели» или случайным веб-страницам) сверяются URL, параметры и форматы ответов.

| Каталог | Исходный проект | Что смотреть для auth / токенов |
|---------|-----------------|----------------------------------|
| `docs/references/qobuz-dl/` | vitiko98/qobuz-dl | `qobuz_dl/qopy.py`: база API `https://www.qobuz.com/api.json/0.2/` (около строки 39), `user/login` по email/password в `api_call` / `auth` (около 44–130). **В этой копии нет браузерного OAuth** (модуль вроде `oauth.py` — из PR #331, его может не быть в вашем клоне). |
| `docs/references/streamrip/` | nathom/streamrip | `streamrip/client/qobuz.py`: константа `QOBUZ_BASE_URL` (стр. ~26–27), загрузка `bundle.js` со страницы `https://play.qobuz.com/login` (~69–79), логин `user/login` с `user_id` + `user_auth_token` при `use_auth_token` (~183–211), сборка URL `…/0.2/{endpoint}` (~442–450). |
| `docs/references/qobuz-sync/` | trevorstarick/qobuz-sync | `client/client.go`: `baseApp`, `baseAPI`, заголовок `X-User-Auth-Token` (~37–42); обход пароля через `AUTH_TOKEN` (~99–104). |
| `docs/references/qobuz-qt/` | qobuz-qt (см. `docs/references/qobuz-qt.ru.md`) | `rust/src/api/client.rs`: `BASE_URL` (~12), **не браузерный OAuth**, а подписанный `GET …/oauth2/login` с `username`/`password` (~188–221); ответ `OAuthLoginResponse` — `rust/src/api/models.rs` (~6–19): поля `oauth2.access_token` и/или `user_auth_token`. |
| `docs/references/qobuz-dl-go/` | Aeneaj/qobuz-dl-go | **Рекомендуется добавить клон сюда вручную.** Именно там искать **authorize URL**, **token URL**, `client_id`, `redirect_uri`, обмен `code` на токен для FP-1; после клонирования: поиск по дереву `rg -i 'authorize|token|oauth'` в этом каталоге. |

Итог: **таблица эндпоинтов браузерного OAuth** для Euterpe — ниже (эталон: ветка `bug/newauth` / PR #331 в `docs/references/qobuz-dl`, файлы `qobuz_dl/core.py`, `qobuz_dl/qopy.py`, `qobuz_dl/bundle.py`). Клон **qobuz-dl-go** в `docs/references/` не содержит отдельного OAuth-модуля — только token login.

### Браузерный OAuth (FP-1, зафиксировано по PR #331)

| Шаг | URL / метод | Параметры | Ответ / результат |
|-----|-------------|-----------|-------------------|
| 1. Authorize (браузер) | `GET https://www.qobuz.com/signin/oauth` | `ext_app_id` = `app_id` из bundle.js; `redirect_url` = callback Euterpe (`EUTERPE_PUBLIC_BASE_URL` + `/api/v1/qobuz/oauth/callback`) | Редирект на `redirect_url` с `code` или `code_autorisation` в query |
| 2. Exchange code | `GET https://www.qobuz.com/api.json/0.2/oauth/callback` | `code`, `private_key` (из bundle.js, `Bundle.get_private_key()`), `app_id` | JSON `{ "token": "<uat>" }` |
| 3. Partner login | `POST https://www.qobuz.com/api.json/0.2/user/login?app_id=…` | Заголовки `X-App-Id`, `X-User-Auth-Token`; тело `extra=partner` | JSON с `user`, `user_auth_token`, `user.credential` |
| 4. Euterpe | SQLite `qobuz_accounts` + `settings.qobuz.active_account_id` | UAT шифруется `EUTERPE_MASTER_KEY` | Пересборка `QobuzClient` без рестарта |

Реализация в Rust: `euterpe-qobuz::oauth`, сервер `GET /api/v1/qobuz/oauth/start|callback`.

## Статус (проверено по референсам, май 2026)

| Метод | Статус | Комментарий |
|-------|--------|-------------|
| **Email + password** → `user/login` | **Не работает / deprecated** | Qobuz перевёл веб-логин на OAuth + reCAPTCHA; автоматический login по паролю в API даёт 401 ([qobuz-dl #329](https://github.com/vitiko98/qobuz-dl/issues/329), [streamrip #954](https://github.com/nathom/streamrip/issues/954)) |
| **`user_id` + `user_auth_token`** → `user/login` | **Работает** | Режим streamrip `use_auth_token = true` |
| **Только `X-User-Auth-Token`** (без login) | **Работает** | Как `AUTH_TOKEN` в [qobuz-sync](https://github.com/trevorstarick/qobuz-sync) |
| **OAuth** (браузерный redirect) | **Рекомендуется** для CLI | Исходники: `docs/references/qobuz-dl-go` (после клонирования); контекст PR #331 — ветка/патч в `docs/references/qobuz-dl`, если вы тянете её отдельно |

**Вывод для Euterpe:** в Docker и UI по умолчанию — **токен**, не пароль. Пароль в документации и env оставить только как legacy с предупреждением.

## Что такое `user_auth_token` (UAT)

- Сессионный токен пользователя Qobuz.
- Возвращается в ответе `user/login` (при успешной аутентификации).
- Хранится в браузере: `localStorage` → ключ `localuser` → поля `id` (user_id) и `userAuthToken`.
- Часто описывается как **JWT-подобный** токен с **ограниченным сроком жизни** (часы–дни) — требуется обновление из браузера или OAuth refresh.

После получения UAT все API-вызовы используют заголовок:

```
X-User-Auth-Token: <uat>
X-App-Id: <app_id>
```

## Способ 1 — Ручной токен (homelab, Phase 1)

Подходит для Euterpe MVP без OAuth-сервера.

### Из Local Storage (предпочтительно)

1. Войти на [https://play.qobuz.com](https://play.qobuz.com)
2. DevTools → **Application** → **Local Storage** → `https://play.qobuz.com`
3. Ключ `localuser` → JSON:
   - `id` → `EUTERPE_QOBUZ_USER_ID`
   - `userAuthToken` → `EUTERPE_QOBUZ_AUTH_TOKEN`

### Из Network

1. DevTools → **Network**
2. Обновить страницу / воспроизвести трек
3. Найти запрос к `user/login` (может быть **POST** в новом веб-клиенте)
4. Response → скопировать `user_auth_token` и `user.id`

### Конфиг Euterpe (Docker)

```bash
EUTERPE_QOBUZ_USER_ID=12345678
EUTERPE_QOBUZ_AUTH_TOKEN=<paste token>
# НЕ задавать EUTERPE_QOBUZ_PASSWORD для новых установок
```

### Поведение `euterpe-qobuz`

Режим **`SessionToken`** (см. [authentication.ru.md](authentication.ru.md)):

1. Bootstrap `app_id` + secrets (bundle.js) — по-прежнему нужен для `track/getFileUrl`
2. Установить headers `X-App-Id`, `X-User-Auth-Token`
3. Опционально вызвать `user/login?user_id=&user_auth_token=&app_id=` для проверки/обновления UAT
4. Не отправлять email/password

## Способ 2 — Login по токену (streamrip)

```
GET https://www.qobuz.com/api.json/0.2/user/login
  ?user_id=<id>
  &user_auth_token=<token>
  &app_id=<app_id>
```

Успех → JSON с (возможно обновлённым) `user_auth_token`, данные `user`, `credential`.

streamrip config:

```toml
use_auth_token = true
email_or_userid = "<user_id>"
password_or_token = "<user_auth_token>"
```

**TDD:** mock этот GET; не использовать реальный пароль в CI.

## Способ 3 — OAuth из приложения Euterpe (план FP-1)

Эталон поведения CLI — репозиторий **qobuz-dl-go**; локально: `docs/references/qobuz-dl-go` (см. таблицу в начале файла). Там же искать точные **authorize** / **token** URL и параметры обмена `code`.

```bash
# в upstream CLI (не в каждой копии qobuz-dl на Python)
qobuz-dl oauth
# локальный redirect → сохранение токена в config
```

**Цель Euterpe:** тот же flow, но **внутри веб-UI** с сохранением в SQLite (`qobuz_accounts`), а не в env.

| Шаг | Компонент |
|-----|-----------|
| 1 | UI: «Подключить Qobuz» |
| 2 | `GET /api/v1/qobuz/oauth/start` → redirect |
| 3 | Callback → encrypt UAT → `INSERT qobuz_accounts` |
| 4 | Установить `qobuz.active_account_id` если первый аккаунт |

Подробно: [future-plans.ru.md](../00-overview/future-plans.ru.md#fp-1--получение-qobuz-токена-из-приложения).

**Не входит в M1** crate `euterpe-qobuz`; Phase 2b / 4 server + UI.

### Ручная вставка токена (interim)

До FP-1: Settings могут принимать paste `user_id` + UAT → `POST /api/v1/qobuz/accounts` (plaintext over HTTPS, сразу encrypt at rest) — опционально Phase 2a.

## Несколько аккаунтов Qobuz (план FP-2)

Один инстанс Euterpe может хранить **несколько** Qobuz-профилей; пользователь выбирает **активный** для sync и загрузок.

- Таблица `qobuz_accounts`
- Setting `qobuz.active_account_id`
- UI: dropdown в header

См. [future-plans.ru.md](../00-overview/future-plans.ru.md#fp-2--выбор-активного-пользователя-qobuz).

---

## Обновление и истечение токена

| Симптом | Действие |
|---------|----------|
| 401 на API | Получить новый UAT из браузера или пройти OAuth |
| UI «Qobuz disconnected» | Показать инструкцию + ссылку на play.qobuz.com |
| Фоновый sync failed | Запись в `qobuz_sync_runs.error_message`, не падать весь сервер |

Server Phase 2: хранить `qobuz.uat_expires_at` если удастся декодировать JWT `exp` (опционально).

## Partner refresh (community workaround)

В обсуждении [qobuz-dl #329](https://github.com/vitiko98/qobuz-dl/issues/329) упоминается патч `qopy.py`:

- `user/login` как **POST**
- refresh через endpoint с `extra=partner` вместо email/password
- авто-запись нового токена в config

**Euterpe:** зафиксировать как **исследование** в M6 (не блокирует Phase 1). При реализации — сначала тесты с mock POST response.

## Безопасность

- UAT = полный доступ к аккаунту Qobuz → шифровать at rest, не логировать
- Не коммитить в git; не показывать в UI целиком (маска `****last4`)
- Ротация при подозрении на утечку

## Ссылки

- [authentication.ru.md](authentication.ru.md)
- [api-reference.ru.md](api-reference.ru.md)
- [reference-implementation.ru.md](reference-implementation.ru.md) — сводная таблица репозиториев и путей в `docs/references/`
