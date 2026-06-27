# Data test fixtures

## legacy-sqlx-v18.sqlite

`legacy-sqlx-v18.sqlite` is a binary compatibility fixture for databases created by the historical SQLx migration chain through migration `018_scan_keep_paths`.

The fixture intentionally does not carry Welds migration metadata. It represents the cutover case where the application opens an existing SQLx-era database after the runtime migration owner has moved to Welds builders.

Representative contents:

- one custom `downloads.settings` value with concurrency `7`;
- one artist, album, and track row;
- one queued download job;
- one queued convert job;
- one queued CUE job;
- one tag-source integration row;
- one Qobuz favorite, sync run, account, and OAuth state;
- one completed library scan run;
- one scan keep path row.

Checksum:

```text
sha256 5aebba2157c9c5e0f21896a9b29f8ab6ef4402a70dfbee2732a5b42806b91959
```
