# Library scan (FP-9 / FP-9e / FP-9b / FP-9d / FP-7e)

`POST /api/v1/library/scan` запускает фоновый обход хранилища библиотеки, настроенного в Settings.

Опциональный query **`root`**: относительный путь под корнем библиотеки (например `Artist/Album`) — сканируется только это поддерево (FP-7e). Валидация: без `..`, не absolute, каталог существует и лежит под `library_path` после `canonicalize`.

**Отмена:** `DELETE /api/v1/library/scan/{id}` — для run в статусе `running` переводит в `cancelled` (**204**). Повторная отмена — **409**. Неизвестный id — **404**. Частично проиндексированные файлы в БД остаются (MVP).

## Двухфазная модель (enumerate → process)

**Enumerate (FP-9b):** общая `DirWorkQueue` — `BinaryHeap` по `depth` (глубже = выше приоритет), `visited` по canonical path. Воркер делает `read_dir` **одного уровня**: файлы → `path_queue`, подкаталоги → re-enqueue. Seed: подкаталоги на `EUTERPE_LIBRARY_SCAN_SEED_DEPTH` или один `root` для subtree.

**Process:** забирает пути из `path_queue`, `stat` (mtime + size). Если в БД тот же `path`, `file_mtime`, `file_size` — **skip** без тегов/SHA256; счётчики `files_processed` и `files_indexed` растут (FP-9d). Иначе — теги + SHA256 → `index_queue` → DB writer.

Корень библиотеки и пути в очереди **canonicalize** (важно на macOS `/var` vs `/private/var`).

После join enumerate: `files_seen` → **`files_total`**.

```mermaid
flowchart LR
  dirQ[(dir_queue heap + visited)]
  enum[Enumerate read_dir 1 level]
  pathQ[(path_queue)]
  proc[Process stat / skip / index]
  indexQ[(index_queue)]
  dbw[DB writer]
  dirQ --> enum
  enum --> pathQ
  enum --> dirQ
  pathQ --> proc
  proc --> indexQ
  indexQ --> dbw
```

### Счётчики

| Поле | Когда растёт | Смысл |
|------|----------------|--------|
| `files_seen` | enumerate | Найдено аудиофайлов |
| `files_total` | после join enumerate | = `files_seen` на конец enumerate |
| `files_processed` | process (включая skip) | Обработка пути завершена |
| `files_indexed` | DB writer или skip | Учтено в прогрессе индекса |

## Env

| Переменная | Default | Назначение |
|------------|---------|------------|
| `EUTERPE_LIBRARY_SCAN_WORKER_TOTAL` | 10 | enum + process ≤ total |
| `EUTERPE_LIBRARY_SCAN_ENUM_WORKERS` | 5 | Пул enumerate |
| `EUTERPE_LIBRARY_SCAN_PROCESS_WORKERS` | 5 | Пул process |
| `EUTERPE_LIBRARY_SCAN_PATH_QUEUE` | 2048 | Очередь путей |
| `EUTERPE_LIBRARY_SCAN_SEED_DEPTH` | 1 | Seed от корня |
| `EUTERPE_LIBRARY_SCAN_INDEX_QUEUE` | 512 | Очередь index jobs |

`EUTERPE_DEBUG=true` — подробные логи воркеров на уровне `debug` (как у download worker).

## SQLite

Миграция `010_tracks_file_size.sql`: колонка `tracks.file_size` для skip-by-stat.
