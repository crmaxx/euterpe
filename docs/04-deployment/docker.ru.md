# Docker

## Образ

Multi-stage Dockerfile (документированный; файл в `docker/Dockerfile` при Phase 2).

### Stage 1 — frontend

```dockerfile
FROM node:22-bookworm AS frontend
WORKDIR /app/frontend
COPY frontend/package*.json ./
RUN npm ci
COPY frontend/ ./
RUN npm run build
```

### Stage 2 — rust

```dockerfile
FROM rust:1-bookworm AS rust
WORKDIR /app
COPY . .
RUN cargo build --release -p euterpe-server
```

### Stage 3 — runtime

```dockerfile
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
RUN useradd -r -s /bin false euterpe
COPY --from=rust /app/target/release/euterpe-server /usr/local/bin/
COPY --from=frontend /app/frontend/dist /usr/share/euterpe/static
USER euterpe
VOLUME ["/data", "/music"]
ENV EUTERPE_DATABASE_URL=sqlite:/data/library.db
EXPOSE 8080
HEALTHCHECK CMD curl -f http://127.0.0.1:8080/health || exit 1
ENTRYPOINT ["euterpe-server"]
```

Phase 1: только `euterpe-qobuz` CLI binary optional.

## Volumes

| Mount | Путь в контейнере | Содержимое |
|-------|------------------|------------|
| `euterpe-data` | `/data` | `library.db`, config, qobuz cache |
| `euterpe-music` | `/music` | FLAC/MP3 |

## Environment

| Variable | Default | Описание |
|----------|---------|----------|
| `EUTERPE_BIND` | `127.0.0.1:8080` | Listen address |
| `EUTERPE_DATABASE_URL` | `sqlite:/data/library.db` | sqlx URL |
| `EUTERPE_LIBRARY_PATH` | `/music` | Scan/download root |
| `EUTERPE_QOBUZ_USER_ID` | — | **Recommended** — from browser `localuser.id` |
| `EUTERPE_QOBUZ_AUTH_TOKEN` | — | **Recommended** — `userAuthToken` |
| `EUTERPE_QOBUZ_EMAIL` | — | Deprecated (password login unreliable) |
| `EUTERPE_QOBUZ_PASSWORD` | — | Deprecated — use AUTH_TOKEN instead |
| `EUTERPE_ADMIN_PASSWORD` | — | UI auth Phase 1 |
| `RUST_LOG` | `euterpe=info` | tracing |

## Сеть

- Default: `127.0.0.1` only
- LAN: `0.0.0.0:8080` + firewall
- Remote: Tailscale / WireGuard, не port-forward

## См. также

[compose.example.yml](compose.example.yml), [backup-restore.ru.md](backup-restore.ru.md)
