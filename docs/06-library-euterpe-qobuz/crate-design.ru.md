# Дизайн crate `euterpe-qobuz`

## Cargo manifest (черновик)

```toml
[package]
name = "euterpe-qobuz"
version = "0.1.0"
edition.workspace = true

[dependencies]
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls", "cookies", "stream"] }
tokio = { version = "1", features = ["rt-multi-thread", "macros", "time", "sync"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
tracing = "0.1"
md-5 = "0.10"
chrono = { version = "0.4", features = ["serde"] }

[dev-dependencies]
mockito = "1"
tokio-test = "0.4"
pretty_assertions = "1"

[features]
default = []
download = []      # optional: stream bytes to file
raw_json = []      # expose serde_json::Value in debug API
```

## Структура модулей

```
crates/euterpe-qobuz/
├── Cargo.toml
├── src/
│   ├── lib.rs              # pub use prelude
│   ├── error.rs
│   ├── config.rs
│   ├── bundle.rs
│   ├── signing.rs
│   ├── client.rs
│   ├── pagination.rs
│   ├── api/
│   │   ├── mod.rs
│   │   ├── auth.rs
│   │   ├── catalog.rs
│   │   ├── favorites.rs
│   │   └── streaming.rs
│   └── models/
│       ├── mod.rs
│       ├── user.rs
│       ├── album.rs
│       ├── track.rs
│       ├── artist.rs
│       └── favorites.rs
└── tests/
    ├── signing_golden.rs
    ├── bundle_fixtures.rs
    └── integration_live.rs  # #[ignore]
```

## QobuzClient

```rust
pub struct QobuzClient {
    http: reqwest::Client,
    config: QobuzConfig,
    state: ClientState,
}

struct ClientState {
    app_id: String,
    secrets: Vec<String>,
    active_secret: Option<String>,
    user_auth_token: Option<String>,
    favorites_sign_mode: FavoritesSignMode,
}
```

### Construction

```rust
impl QobuzClient {
    /// Bootstrap secrets + apply SessionToken headers (no password).
    pub async fn connect(config: QobuzConfig) -> Result<Self, QobuzError>;

    pub async fn bootstrap(config: QobuzConfig) -> Result<Self, QobuzError>;

    /// TokenLogin or deprecated EmailPassword only.
    pub async fn login(&mut self) -> Result<UserProfile, QobuzError>;

    /// True if X-User-Auth-Token is set.
    pub fn is_authenticated(&self) -> bool;

    /// Lightweight API ping (e.g. favorites limit=1).
    pub async fn verify_session(&self) -> Result<(), QobuzError>;
}
```

## QobuzConfig

```rust
pub struct QobuzConfig {
    pub auth: AuthConfig,
    pub app_id: Option<String>,
    pub secrets: Option<Vec<String>>,
    pub api_base: String,
    pub user_agent: String,
    pub favorites_sign_mode: FavoritesSignMode,
    pub request_timeout: Duration,
    /// If true, call user/login after setting session token (streamrip style).
    pub refresh_session_via_login: bool,
}

pub enum AuthConfig {
    /// Default 2026: UAT from browser/OAuth; password login not used.
    SessionToken {
        user_id: u64,
        user_auth_token: String,
    },
    /// Optional: validate/refresh via user/login?user_id=&user_auth_token=
    TokenLogin {
        user_id: u64,
        user_auth_token: String,
    },
    /// Deprecated: expect Authentication error from API.
    #[deprecated(note = "Qobuz no longer supports automated email/password login")]
    EmailPassword { email: String, password: String },
}
```

`QobuzClient::connect` default: `SessionToken` + bootstrap secrets + set headers; `login()` — no-op или optional `TokenLogin` если `refresh_session_via_login`.

## Public API surface (Phase 1)

```rust
// favorites
pub async fn favorites_albums(&self, page: PageRequest) -> Result<Page<AlbumSummary>, QobuzError>;
pub async fn favorites_all_albums(&self) -> Result<Vec<AlbumSummary>, QobuzError>;
pub async fn favorite_add_albums(&self, ids: &[u64]) -> Result<(), QobuzError>;
pub async fn favorite_remove_albums(&self, ids: &[u64]) -> Result<(), QobuzError>;
// аналогично tracks, artists — M2/M3

// streaming
pub async fn track_stream_url(&self, track_id: u64, quality: Quality) -> Result<StreamUrl, QobuzError>;

// catalog
pub async fn album(&self, album_id: u64) -> Result<AlbumDetail, QobuzError>;
pub async fn artist_albums(&self, artist_id: u64) -> Result<Vec<AlbumSummary>, QobuzError>;
```

## Traits (testability)

```rust
#[async_trait::async_trait]
pub trait QobuzApi: Send + Sync {
    async fn favorites_albums(&self, page: PageRequest) -> Result<Page<AlbumSummary>, QobuzError>;
    // ...
}
```

`euterpe-server` в тестах использует mock impl.

## Logging

`tracing` spans: `qobuz.login`, `qobuz.favorites.list` — без PII.

## TDD rule

Новый public method → сначала тест в `src/...` `#[cfg(test)]` или `tests/`, затем реализация.
