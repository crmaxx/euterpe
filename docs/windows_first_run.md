# Первый запуск на Windows 10 (Docker)

Руководство для запуска Euterpe на **Windows 10** через **Docker Desktop**, без установки Rust, Node.js и без локальной компиляции на Windows. Образа на Docker Hub нет — сборка только из исходников на GitHub.

См. также: [docker.ru.md](04-deployment/docker.ru.md), [compose.example.yml](04-deployment/compose.example.yml).

## Что понадобится

- **Docker Desktop for Windows** (backend WSL 2 рекомендуется)
- **Git** (или ZIP-архив репозитория с GitHub)
- Браузер

Сборка выполняется **внутри Linux-контейнеров** (multi-stage `docker/Dockerfile`: Node → Rust → runtime).

## 1. Установить Docker Desktop

1. Скачать [Docker Desktop for Windows](https://docs.docker.com/desktop/install/windows-install/).
2. Включить **WSL 2** (рекомендуется для Windows 10).
3. После установки убедиться, что в терминале работают:

```powershell
docker version
docker compose version
```

## 2. Клонировать репозиторий

В **PowerShell** или **cmd**:

```powershell
cd C:\Users\ВашПользователь
git clone https://github.com/crmaxx/euterpe.git
cd euterpe
```

Подставьте URL своего форка, если клонируете не с `crmaxx/euterpe`.

## 3. Подготовить `compose.yml` и `.env`

Скопировать пример compose в **корень репозитория**:

```powershell
copy docs\04-deployment\compose.example.yml compose.yml
copy .env.example .env
```

В `compose.yml` для корня репозитория поправить **build context** (в примере он рассчитан на каталог `docs/04-deployment/`):

```yaml
    build:
      context: .
      dockerfile: docker/Dockerfile
```

Сгенерировать ключ шифрования Qobuz-токенов в SQLite (**32 байта**, hex — 64 символа). Пример в PowerShell:

```powershell
-join ((1..32 | ForEach-Object { '{0:x2}' -f (Get-Random -Max 256) }))
```

В файле `.env`:

```env
EUTERPE_MASTER_KEY=<вставьте_64_символа_hex>
EUTERPE_PUBLIC_BASE_URL=http://127.0.0.1:8080
```

`compose.example.yml` уже подхватывает `${EUTERPE_MASTER_KEY}` из `.env`.

## 4. Тома: данные и музыка

По умолчанию в примере compose — **именованные тома** Docker (`euterpe-data`, `euterpe-music`).

Чтобы использовать папку на диске Windows, замените монтирование музыки в `compose.yml`:

```yaml
    volumes:
      - euterpe-data:/data
      - C:/Music:/music
```

- Путь лучше указывать со слэшами вперёд: `C:/Music`.
- В **Docker Desktop → Settings → Resources → File sharing** должна быть разрешена буква диска.

В контейнере:

| Переменная | Значение по умолчанию | Назначение |
|------------|----------------------|------------|
| `EUTERPE_LIBRARY_PATH` | `/music` | Корень библиотеки (скан, загрузки) |
| `EUTERPE_DATABASE_URL` | `sqlite:/data/library.db?mode=rwc` | SQLite в томе `/data` |

## 5. Собрать образ и запустить

Первый запуск **долгий** (внутри образа: `npm ci`, `cargo build --release`):

```powershell
docker compose build
docker compose up -d
```

Логи:

```powershell
docker compose logs -f euterpe
```

Проверка health:

```powershell
curl http://127.0.0.1:8080/health
```

В браузере: **http://127.0.0.1:8080**

## 6. Подключение Qobuz

Токены **не** задаются через `EUTERPE_QOBUZ_USER_ID` / `EUTERPE_QOBUZ_AUTH_TOKEN` в env (устарело).

После старта: **Settings → Connect Qobuz** в веб-интерфейсе.

`EUTERPE_PUBLIC_BASE_URL` должен совпадать с тем URL, по которому вы открываете UI (нужен для OAuth callback).

## 7. Доступ с другого ПК в LAN

В `compose.yml` изменить порты:

```yaml
    ports:
      - "0.0.0.0:8080:8080"
```

В `.env`:

```env
EUTERPE_PUBLIC_BASE_URL=http://192.168.x.x:8080
```

Добавить правило в брандмауэре Windows для TCP 8080.

## Схема

```text
Win10 + Docker Desktop
    → git clone
    → compose.yml (context: .) + .env (EUTERPE_MASTER_KEY)
    → docker compose build   ← Rust/Node только в Linux-слоях образа
    → docker compose up -d
    → http://127.0.0.1:8080
```

## Остановка и данные

```powershell
docker compose down
```

Тома с БД и музыкой сохраняются. Удалить тома вместе с контейнером (осторожно — сотрёт `library.db`):

```powershell
docker compose down -v
```

## Частые проблемы

| Проблема | Что сделать |
|----------|-------------|
| `build` падает на `npm ci` / `cargo` | Контекст сборки — **корень репо** (должны быть `frontend/`, `crates/`, `migrations/`) |
| Очень долгая первая сборка | Нормально; повторные сборки быстрее за счёт кэша слоёв Docker |
| Нет доступа к `C:\Music` | File sharing в Docker Desktop + путь `C:/Music:/music` |
| OAuth Qobuz не срабатывает | Совпадение `EUTERPE_PUBLIC_BASE_URL` и адреса в браузере |
| WSL2 не включён | Docker Desktop предложит включить; может потребоваться перезагрузка |

## См. также

- [docker.ru.md](04-deployment/docker.ru.md) — устройство образа и переменные окружения
- [backup-restore.ru.md](04-deployment/backup-restore.ru.md) — резервное копирование `library.db`
- [oauth-and-tokens.ru.md](05-qobuz/oauth-and-tokens.ru.md) — Qobuz и токены
