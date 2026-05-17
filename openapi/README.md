# OpenAPI contract

Канонический REST-контракт: [`openapi.yaml`](openapi.yaml).

Этот каталог — **отдельный npm-проект**: просмотр и сборка документации не требуют запуска `euterpe-server`.

## Установка

```bash
cd openapi
npm ci
```

## Lint

```bash
npm run lint
```

Из корня репозитория (без `cd`):

```bash
npx --yes @redocly/cli lint openapi/openapi.yaml
```

## Bundle (один файл)

```bash
npm run bundle
# → bundled.yaml (в .gitignore)
```

## Просмотр документации (dev-сервер)

Интерактивный Redoc (Redocly CLI 2.x: `preview`, не `preview-docs`):

```bash
npm run preview
# → http://127.0.0.1:8088
```

При первом запуске CLI подтянет пакет `redoc` через npx — может занять несколько секунд.

Статический просмотр после сборки (без live-reload):

```bash
npm run serve
# build → dist/index.html, затем serve на :8088
```

## Статическая HTML-сборка

Офлайн-артефакт для хостинга или открытия без Node:

```bash
npm run build
# → dist/index.html (+ assets)
```

Или любой статический сервер:

```bash
npx --yes serve dist -l 8088
```

## Runtime (euterpe-server)

Сервер по-прежнему отдаёт spec как JSON: `GET /api/openapi.json`.

Документация в `openapi/dist/` с ним не связана — это только контракт и UI для разработчиков.

## CI

В GitHub Actions: `npm ci` и `npm run lint` / `npm run build` в `openapi/`.

См. [docs/02-backend/openapi-first.ru.md](../docs/02-backend/openapi-first.ru.md).
