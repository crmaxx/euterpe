---
title: Dev Vite UI should not use backend static dist
date: 2026-07-05
category: developer-experience
module: Dev Runtime
problem_type: developer_experience
component: tooling
severity: medium
applies_when:
  - "Running a dev stack with a separate Vite frontend service"
  - "The backend can also serve a built SPA from frontend/dist"
  - "The repo may contain an older frontend/dist build artifact"
symptoms:
  - "Fresh frontend changes do not appear in the dev UI"
  - "Opening the backend port can show an older production bundle from frontend/dist"
  - "Vite in Docker may miss host file changes without polling"
root_cause: config_error
resolution_type: config_change
related_components:
  - Frontend Vite
  - Backend static fallback
  - Docker Compose
  - Overmind
tags: [docker, vite, frontend, dev-server, static-assets, compose, overmind]
---

# Dev Vite UI should not use backend static dist

## Context

The dev stack runs backend and frontend as separate processes, either through Docker Compose or `make dev-local` with Overmind. The backend exposes the API on the configured bind address, while the frontend runs Vite on `127.0.0.1:5173` and proxies API calls to the backend.

The failure mode was subtle because the backend also knows how to serve a built SPA from `frontend/dist`. Any old host-side `frontend/dist/index.html` can therefore look like a valid dev UI if the browser is opened on the backend port instead of the Vite port.

## Guidance

In dev mode, treat Vite as the only UI server. Keep backend static fallback disabled or pointed at a path that does not contain a built SPA.

```yaml
backend:
  environment:
    # Dev UI is served by Vite on :5173; keep backend from serving stale host frontend/dist.
    EUTERPE_STATIC_DIR: /tmp/euterpe-static-disabled
```

For `make dev-local`, set the same override in the process manager:

```procfile
backend: EUTERPE_STATIC_DIR=/tmp/euterpe-static-disabled make backend
frontend: make frontend-dev
```

Vite should also derive its API proxy from the same root `.env` as the backend when `VITE_DEV_API_PROXY` is not explicitly set. That keeps local `EUTERPE_BIND=127.0.0.1:9080` and frontend proxying aligned.

For frontend containers running on Docker Desktop, make file watching explicit. Bind mounts can miss native file-system events, so Vite should be configured to honor polling environment variables:

```yaml
frontend:
  environment:
    CHOKIDAR_USEPOLLING: "true"
    CHOKIDAR_INTERVAL: "300"
```

```ts
const usePolling = ["1", "true", "yes"].includes(
  (process.env.CHOKIDAR_USEPOLLING ?? "").toLowerCase(),
);
const pollingInterval = Number(process.env.CHOKIDAR_INTERVAL ?? 300);

export default defineConfig({
  server: {
    watch: {
      usePolling,
      interval: Number.isFinite(pollingInterval) ? pollingInterval : 300,
    },
  },
});
```

## Why This Matters

Without disabling backend static fallback, two URLs can appear to be valid dev UIs:

- `127.0.0.1:5173` serves the Vite dev server and should reflect source changes.
- `127.0.0.1:9080` can serve the backend API and, if `frontend/dist` exists, an old built frontend.

That makes the symptom look like Docker did not mount new source code, while the actual problem is that the browser is using a stale static artifact from the backend service. Polling handles the other half of the problem: even on the correct Vite port, Docker bind mounts may not deliver reliable file-watch events.

## When to Apply

- A dev stack has separate backend and Vite services or processes.
- The backend process has SPA fallback behavior for production builds.
- The repo contains or can contain a host-side `frontend/dist`.
- UI changes are visible after rebuilding assets but not during normal dev refresh or HMR.

## Examples

Before:

```yaml
backend:
  environment:
    EUTERPE_STATIC_DIR: /app/frontend/dist

frontend:
  command: npm run dev -- --host 0.0.0.0
```

After:

```yaml
backend:
  environment:
    EUTERPE_STATIC_DIR: /tmp/euterpe-static-disabled

frontend:
  command: npm run dev -- --host 0.0.0.0
  environment:
    CHOKIDAR_USEPOLLING: "true"
    CHOKIDAR_INTERVAL: "300"
```

Verification should check both configuration and frontend tooling:

```bash
docker compose -f docker/compose.dev.yml --project-name euterpe-dev config
mise exec -- node -e "const vite = require('vite'); vite.resolveConfig({}, 'serve', 'development').then((config) => console.log(config.server.proxy))"
mise exec -- npm --prefix frontend run lint
mise exec -- npm --prefix frontend exec -- tsc -b frontend/tsconfig.json --noEmit
```

## Related

- `docker/compose.dev.yml` owns the dev-only split between API and Vite UI.
- `Procfile` owns local Overmind dev process settings.
- `frontend/vite.config.ts` owns Vite server proxy and watch behavior.
