use axum::Router;
use tower_http::services::{ServeDir, ServeFile};

use crate::config::AppConfig;
use crate::state::AppState;

/// Attach SPA fallback when `static_dir/index.html` exists.
pub fn apply_fallback(router: Router<AppState>, config: &AppConfig) -> Router<AppState> {
    let index = config.static_dir.join("index.html");
    if !index.is_file() {
        return router;
    }
    let dir = config.static_dir.clone();
    router.fallback_service(ServeDir::new(&dir).fallback(ServeFile::new(index)))
}
