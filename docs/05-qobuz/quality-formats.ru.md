# Качество аудио (format_id)

## Таблица format_id

Используется в `track/getFileUrl` и при выборе качества загрузки.

| format_id | Описание | qobuz-dl `-q` | streamrip `--quality` |
|-----------|----------|---------------|------------------------|
| **5** | MP3 ~320 kbps | 5 | 1 |
| **6** | FLAC 16-bit / 44.1 kHz (CD) | 6 | 2 |
| **7** | FLAC 24-bit ≤ 96 kHz | 7 | 3 |
| **27** | FLAC 24-bit > 96 kHz | 27 | 4 |

### Маппинг в euterpe-qobuz

```rust
pub enum Quality {
    Mp3_320,      // 5
    FlacCd,       // 6
    FlacHiRes,    // 7
    FlacHiResPlus // 27
}

impl Quality {
    pub fn format_id(self) -> u8 { ... }
    pub fn from_streamrip_level(level: u8) -> Option<Self> { ... }
}
```

## Проверка доступности

Объект `track` в metadata содержит поля (имена по API):

- `maximum_bit_depth`
- `maximum_sampling_rate`
- `hires_streamable` (bool)

При недоступном качестве `track/getFileUrl` может вернуть ответ без `url` и с `restrictions`.

### quality_fallback (qobuz-dl)

Если выбранное качество недоступно — опционально понизить (6 → 5). В Euterpe Phase 3 — настройка в job payload.

## Расширение файла

| format_id | Типичное расширение |
|-----------|---------------------|
| 5 | `.mp3` |
| 6, 7, 27 | `.flac` |

Определять по `format_id` из ответа `getFileUrl`, не только по URL.

## TDD

- Unit: `Quality::format_id()` round-trip
- Mock `getFileUrl` JSON с `format_id` 6 и 27
