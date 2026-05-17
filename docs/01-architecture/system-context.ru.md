# Системный контекст (C4 Level 1–2)

## Level 1 — Context

```mermaid
flowchart LR
  User[User browser]
  Euterpe[Euterpe app]
  Qobuz[Qobuz API]
  FS[Local music files]

  User -->|HTTP LAN| Euterpe
  Euterpe -->|HTTPS| Qobuz
  Euterpe -->|read write| FS
```

- **User** — владелец библиотеки, домашняя сеть
- **Euterpe** — Docker на NAS/PC
- **Qobuz** — облачный каталог, избранное, signed URLs
- **FS** — FLAC/MP3 на volume `/music`

## Level 2 — Containers

```mermaid
flowchart TB
  subgraph container [Docker euterpe]
    SPA[React SPA static]
    API[Axum API]
    Worker[Download worker]
    DB[(SQLite)]
    Lib[euterpe-qobuz]
  end

  Browser --> SPA
  Browser --> API
  API --> Lib
  API --> DB
  Worker --> Lib
  Worker --> DB
  Worker --> Music[/music volume/]
  API --> Music
  Lib --> Qobuz[Qobuz HTTPS]
```

Один OS-процесс (рекомендуется): API + worker + SQLite writer.

## Внешние зависимости

| Система | Обязательна | Примечание |
|---------|-------------|------------|
| Qobuz | да (Phase 1) | Подписка |
| DNS / Internet | для sync | Offline — только локальная библиотека |
| Reverse proxy | нет | Опционально Caddy + auth |

## Качество

Разработка: **строгий TDD** на всех контейнерах.
