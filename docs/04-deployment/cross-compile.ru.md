# Кросс-сборка `euterpe-server`

Release-бинарники для:

| Платформа | Rust target | Артефакт |
|-----------|-------------|----------|
| Linux amd64 | `x86_64-unknown-linux-gnu` | `euterpe-server` |
| Windows 10+ amd64 | `x86_64-pc-windows-gnu` | `euterpe-server.exe` |
| DietPi / RPi 1 B+ (ARMv6) | `arm-unknown-linux-gnueabihf` + `arm1176jzf-s` | `euterpe-server` |

Конфигурация: [`.cargo/config.toml`](../../.cargo/config.toml), [`Cross.toml`](../../Cross.toml), цели `make release-*`.

## Быстрый старт

```bash
# overmind, cross, rustup targets, npm (см. Makefile prepare)
# Нужен rustup (не только brew install rust): https://rustup.rs
make prepare

make cross-release-all
# → dist/linux-amd64/euterpe-server
# → dist/windows-amd64/euterpe-server.exe
# → dist/arm-pi1/euterpe-server

# На Linux amd64 хосте можно собрать нативно без Docker:
make release-linux-amd64
make dist-linux-amd64
```

Фронтенд по-прежнему собирается на машине разработки: `cd frontend && npm run build`, на Pi копируется `frontend/dist` (см. [docker.ru.md](docker.ru.md)).

## Raspberry Pi 1 (ARM1176JZF-S)

- Triple: **`arm-unknown-linux-gnueabihf`** (DietPi armhf).
- Флаги: **`-C target-cpu=arm1176jzf-s`** — иначе бинарник с инструкциями ARMv7/NEON может падать с `Illegal instruction` на Pi 1.
- Задаются в [`.cargo/config.toml`](../../.cargo/config.toml) (`cargo build`) и через `CARGO_TARGET_ARM_UNKNOWN_LINUX_GNUEABIHF_RUSTFLAGS` в `make cross-release-arm-pi1` ([`Cross.toml`](../../Cross.toml) passthrough).

Проверка на устройстве:

```bash
file ./euterpe-server
# ELF 32-bit LSB executable, ARM, ...
./euterpe-server  # smoke: должен стартовать без Illegal instruction
```

## Windows

Используется **`x86_64-pc-windows-gnu`** (MinGW). На Windows 10 достаточно скопировать `euterpe-server.exe`; при необходимости установить [MinGW runtime](https://www.mingw-w64.org/) (часто уже есть в portable-сборках).

Сборка **MSVC** (`x86_64-pc-windows-msvc`) в репозитории не настроена — при необходимости добавьте linker в `.cargo/config.toml` отдельно.

## Сборка без Docker (Linux → ARM)

На Debian/Ubuntu хосте:

```bash
sudo apt install gcc-arm-linux-gnueabihf
rustup target add arm-unknown-linux-gnueabihf
make release-arm-pi1
```

Linker `arm-linux-gnueabihf-gcc` прописан в `.cargo/config.toml`.

## Переменные Makefile

| Переменная | По умолчанию | Назначение |
|------------|--------------|------------|
| `CARGO` | `cargo` | Нативная сборка |
| `CROSS` | `cross` | Сборка через cross-rs |
| `DIST` | `dist` | Каталог для `make dist-*` |

## См. также

- [docker.ru.md](docker.ru.md) — runtime в контейнере (не для Pi 1)
- [backup-restore.ru.md](backup-restore.ru.md)
