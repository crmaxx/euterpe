# Frontend stack

Phase 4. Разработка — **TDD** (Vitest + Testing Library).

## Node.js

Рекомендуется **Node 24** (`frontend/.nvmrc`: `nvm use`). Транзитивный `@hawk.so/types` объявляет `engines.node: 24.x`; на Node 22/26 `npm ci` выдаёт `EBADENGINE` — это предупреждение, установка не падает. На Node 26 предупреждение останется, пока CodeX не расширит `engines` в пакете типов.

## Core

| Tool | Версия (ориентир) | Назначение |
|------|-------------------|------------|
| Vite | 6.x | Dev server, build |
| React | 19.x | UI |
| TypeScript | 5.x | Types |
| Tailwind CSS | 4.x | Styling |
| shadcn/ui | latest | Components (Radix) |

## Data

| Tool | Назначение |
|------|------------|
| TanStack Query v5 | Server state, cache, mutations |
| TanStack Table v8 | Favorites / library grids |

## Routing

`react-router-dom` v7 — routes: `/`, `/favorites`, `/queue`, `/settings`.

## API client

**Phase 4:** типы из [`openapi/openapi.yaml`](../../openapi/openapi.yaml) (`openapi-typescript` или MSW handlers из spec).

Обзор: [api-client.ru.md](api-client.ru.md). Spec JSON: `GET /api/openapi.json`.

Base URL: relative `/api/v1` (same origin in Docker).

## Hawk (ошибки в браузере)

[`@hawk.so/browser`](https://docs.hawk-tracker.ru/react) — инициализация в `src/lib/hawk.ts`, `initHawk()` в `main.tsx` до рендера React.

| Переменная | Когда |
|------------|--------|
| `VITE_HAWK_TOKEN` | build / dev — в корневом `.env` или `frontend/.env` (см. `vite.config` `envDir`) |
| `VITE_HAWK_RELEASE` | опционально, для source maps |

`HawkErrorBoundary` отправляет ошибки рендера; глобальные ошибки и unhandled rejection — SDK по умолчанию.

## Dev proxy

`vite.config.ts`:

```ts
server: {
  proxy: {
    '/api': 'http://127.0.0.1:8080',
  },
},
```

## Testing (TDD)

| Tool | Use |
|------|-----|
| Vitest | Unit + component |
| @testing-library/react | UI behavior |
| MSW | Mock API |

Пример: тест кнопки Sync вызывает `POST /api/v1/qobuz/sync` before implementing page.

## Structure

```
frontend/src/
├── components/ui/     # shadcn
├── features/
│   ├── favorites/
│   ├── queue/
│   └── settings/
├── api/
│   ├── client.ts
│   └── hooks.ts
├── App.tsx
└── main.tsx
```

## Theme

Dark default — homelab / listening room; контраст для таблиц.
