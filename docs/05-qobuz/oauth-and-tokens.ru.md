# OAuth и auth-токены (актуально с 2025–2026)

## Статус (проверено по референсам, май 2026)

| Метод | Статус | Комментарий |
|-------|--------|-------------|
| **Email + password** → `user/login` | **Не работает / deprecated** | Qobuz перевёл веб-логин на OAuth + reCAPTCHA; автоматический login по паролю в API даёт 401 ([qobuz-dl #329](https://github.com/vitiko98/qobuz-dl/issues/329), [streamrip #954](https://github.com/nathom/streamrip/issues/954)) |
| **`user_id` + `user_auth_token`** → `user/login` | **Работает** | Режим streamrip `use_auth_token = true` |
| **Только `X-User-Auth-Token`** (без login) | **Работает** | Как `AUTH_TOKEN` в [qobuz-sync](https://github.com/trevorstarick/qobuz-sync) |
| **OAuth** (браузерный redirect) | **Рекомендуется** для CLI | [qobuz-dl-go](https://github.com/Aeneaj/qobuz-dl-go), PR [qobuz-dl #331](https://github.com/vitiko98/qobuz-dl/pull/331) |

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

По модели [qobuz-dl-go](https://github.com/Aeneaj/qobuz-dl-go):

```bash
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
- [reference-implementation.ru.md](reference-implementation.ru.md)
