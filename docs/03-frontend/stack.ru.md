# Frontend stack

Phase 4. Разработка — **TDD** (Vitest + Testing Library).

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

Generated or hand-written types from [api-client.ru.md](api-client.ru.md).

Base URL: relative `/api/v1` (same origin in Docker).

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
