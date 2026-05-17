# Типы данных (serde models)

Модели — **подмножество** полей Qobuz JSON (`#[serde(deny_unknown_fields)]` только где стабильно; иначе `#[serde(default)]` + optional).

## UserProfile

```rust
pub struct UserProfile {
    pub id: u64,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub credential_label: Option<String>, // e.g. "Studio"
}
```

Из `user/login` → `user`, `credential.parameters.short_label`.

## AlbumSummary

```rust
pub struct AlbumSummary {
    pub id: u64,
    pub title: String,
    pub artists: Vec<ArtistRef>,
    pub image: Option<Image>,
    pub release_date_original: Option<String>,
    pub hires: bool,
}
```

## AlbumDetail

```rust
pub struct AlbumDetail {
    #[serde(flatten)]
    pub summary: AlbumSummary,
    pub tracks: Vec<TrackSummary>,
    pub description: Option<String>,
}
```

## TrackSummary

```rust
pub struct TrackSummary {
    pub id: u64,
    pub title: String,
    pub track_number: Option<u32>,
    pub duration: Option<u32>,
    pub performer: Option<ArtistRef>,
    pub hires_streamable: Option<bool>,
}
```

## ArtistRef

```rust
pub struct ArtistRef {
    pub id: u64,
    pub name: String,
}
```

## StreamUrl

```rust
pub struct StreamUrl {
    pub url: String,
    pub format_id: u8,
    pub sampling_rate: Option<u32>,
    pub bit_depth: Option<u32>,
}
```

## Page

```rust
pub struct Page<T> {
    pub items: Vec<T>,
    pub total: u32,
    pub limit: u32,
    pub offset: u32,
}

pub struct PageRequest {
    pub limit: u32,   // default 500
    pub offset: u32,  // default 0
}
```

## FavoriteType

```rust
pub enum FavoriteType {
    Albums,
    Tracks,
    Artists,
}

impl FavoriteType {
    pub fn as_str(self) -> &'static str { ... } // "albums", ...
}
```

## Raw fallback

Feature `raw_json`:

```rust
pub async fn album_raw(&self, id: u64) -> Result<serde_json::Value, QobuzError>;
```

Для `qobuz-sync debug` аналога.

## Fixtures

Хранить в `crates/euterpe-qobuz/tests/fixtures/`:

- `login_ok.json`
- `favorites_albums_page0.json`
- `get_file_url_flac.json`

**TDD:** тесты десериализации без HTTP.
