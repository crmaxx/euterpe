# SMB Storage Migration Plans

Этот каталог содержит поэтапные TODO-планы для оставшихся local-only зон после ввода `LibraryStorage` и `euterpe-smb`.

## Порядок выполнения

1. [x] [Tag write для SMB](tag-write.md)
2. [x] [Cover upload/embed для SMB](cover-upload-embed.md)
3. [x] [Torrent import/copy в SMB library](torrent-import-copy.md)
4. [x] [Integrations apply через storage](integrations-apply.md)
5. [x] [CUE split через storage](cue-split.md)
6. [x] [Converter worker через storage](converter-worker.md)
7. [x] [SMB ChangeNotify watcher](change-notify-watcher.md)

## Общие правила

- Не использовать disk temp bridge в `/data` для library operations.
- Remote atomic temp files внутри целевого storage допустимы: `.<name>.euterpe-part`.
- DB paths всегда library-relative с `/`.
- Для SMB password использовать только encrypted-at-rest Settings + runtime decrypt через `EUTERPE_MASTER_KEY`.
- Local backend должен оставаться совместимым с текущими тестами.
- Каждый этап начинается с failing test, затем минимальная реализация, затем targeted tests.
