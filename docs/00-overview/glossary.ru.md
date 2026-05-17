# Глоссарий

| Термин | Определение |
|--------|-------------|
| **UAT** | `user_auth_token` — основной credential; из браузера (`localuser.userAuthToken`) или OAuth |
| **Auth token login** | `user/login` с `user_id` + `user_auth_token` (streamrip) |
| **App ID** | Идентификатор приложения Qobuz (9 цифр), из `bundle.js` |
| **App secret** | Секрет для подписи `track/getFileUrl` (не путать с appSecret в bundle regex) |
| **Bundle** | JS-бандл `play.qobuz.com/resources/.../bundle.js` с credentials |
| **format_id** | Код качества потока: 5, 6, 7, 27 |
| **Source of truth** | Файлы в `/music`; БД — индекс |
| **Job** | Задача скачивания в `download_jobs` |
| **Sync run** | Запись в `qobuz_sync_runs` о проходе синхронизации избранного |
| **euterpe-qobuz** | Rust crate — клиент Qobuz API без Axum/SQLite |
| **TDD** | Test-Driven Development; обязательный процесс разработки |
| **OpenAPI-first** | Сначала `openapi/openapi.yaml`, затем contract test и handler |
| **Contract test** | Проверка JSON ответа по схеме из OpenAPI (`jsonschema`) |
| **operationId** | Имя операции в OpenAPI (например `qobuzSync`) |
