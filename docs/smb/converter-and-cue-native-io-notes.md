# Converter and CUE Native I/O Notes

Этот файл фиксирует общий риск для `converter-worker.md` и `cue-split.md`.

## Принцип

Для SMB нельзя использовать локальный временный файл в `/data` как compatibility bridge. Допустимые варианты:

- bytes/reader based decoder;
- callback based decoder/encoder;
- in-memory bounded buffer;
- remote sibling temp file внутри SMB share для atomic replace.

## Самые рискованные места

- WavPack binding может оказаться path-only.
- FLAC encoder может оказаться path-output oriented.
- CUE split может предполагать source/output directories as `Path`.

## Решение по умолчанию

Если dependency path-only:

1. Сначала искать reader/callback API в binding.
2. Если API нет, заменить binding на зрелый вариант с memory/callback support.
3. Если mature replacement нет, вернуть explicit unsupported error для конкретного формата и оставить ignored integration test, но не добавлять `/data` temp bridge.

