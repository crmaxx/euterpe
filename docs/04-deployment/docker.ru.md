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

Для отчётов Hawk в SPA передайте build-args (токен попадает в бандл — используйте отдельный frontend-проект в Hawk):

```bash
docker build \
  --build-arg VITE_HAWK_TOKEN='...' \
  --build-arg VITE_HAWK_RELEASE='1.0.0' \
  -f docker/Dockerfile .
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
| `euterpe-torrent` (опционально) | `/data/torrent-incoming` | staging и загрузки BitTorrent (librqbit) |

## Environment

| Variable | Default | Описание |
|----------|---------|----------|
| `EUTERPE_BIND` | `127.0.0.1:8080` | Listen address |
| `EUTERPE_DATABASE_URL` | `sqlite:/data/library.db` | sqlx URL |
| `EUTERPE_LIBRARY_PATH` | `/music` | Scan/download root |
| `EUTERPE_TORRENT_INCOMING_DIR` | — | Каталог для inspect/download торрентов; без него API торрентов отключён |
| `EUTERPE_TORRENT_MAX_ACTIVE` | `2` | Параллельных torrent-задач |
| `EUTERPE_TORRENT_DISABLE_UPLOAD` | `true` | librqbit: не отдавать пирам (только загрузка) |
| `EUTERPE_TORRENT_DEFAULT_MAX_UPLOAD_KIB` | `0` | Лимит отдачи (КиБ/с), если отдача включена |
| `EUTERPE_QOBUZ_USER_ID` | — | **Recommended** — from browser `localuser.id` |
| `EUTERPE_QOBUZ_AUTH_TOKEN` | — | **Recommended** — `userAuthToken` |
| `EUTERPE_QOBUZ_EMAIL` | — | Deprecated (password login unreliable) |
| `EUTERPE_QOBUZ_PASSWORD` | — | Deprecated — use AUTH_TOKEN instead |
| `EUTERPE_ADMIN_PASSWORD` | — | UI auth Phase 1 |
| `HAWK_TOKEN` | — | Токен интеграции [Hawk.so](https://hawk.so) (base64 JSON); при пустом значении отчёты отключены |
| `HAWK_RELEASE` | версия `euterpe-server` | Release в событиях Hawk |
| `HAWK_COLLECTOR_ENDPOINT` | `https://{integrationId}.k1.hawk.so` | Override URL коллектора |
| `HAWK_ENVIRONMENT` | — | Окружение (`production`, `staging`, …) |
| `HAWK_BACKTRACE_TRIM` | `true` | Скрывать кадры `std::` / `tokio::` / … в backtrace |
| `HAWK_BATCH_MAX` | `1` | Размер batch перед POST |
| `HAWK_BATCH_INTERVAL_MS` | `1000` | Интервал flush batch (мс) |
| `HAWK_SAMPLE_RATE` | `1.0` | Доля событий для отправки (0.0–1.0) |
| `HAWK_FLUSH_TIMEOUT_SECS` | `2` | Таймаут flush при shutdown |
| `HAWK_DEDUP_WINDOW_SECS` | `5` | Окно дедупликации одинаковых событий |

### Frontend (Vite, build-time)

| Переменная | Описание |
|------------|----------|
| `VITE_HAWK_TOKEN` | Токен интеграции для `@hawk.so/browser`; пусто → отключено |
| `VITE_HAWK_RELEASE` | Release для source maps (по умолчанию версия из `frontend/package.json`) |

См. [Hawk React / Browser](https://docs.hawk-tracker.ru/react).
| `RUST_LOG` | `euterpe=info` | tracing |

## Сеть

- Default: `127.0.0.1` only
- LAN: `0.0.0.0:8080` + firewall
- Remote: Tailscale / WireGuard, не port-forward

## См. также

[compose.example.yml](compose.example.yml), [backup-restore.ru.md](backup-restore.ru.md)
