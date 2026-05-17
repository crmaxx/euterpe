use crate::client::QobuzClient;
use crate::error::QobuzError;
use crate::models::deser::{parse_album_ref_value, parse_id_value};
use crate::models::{
    AlbumSummary, FavoriteType, FavoritesAlbumsResponse, FavoritesTracksResponse, TrackSummary,
};
use crate::pagination::{fetch_all_pages, Page, PageRequest};
use crate::signing::{sign_favorites, FavoritesSignMode};

fn favorite_item_matches_catalog(item: &serde_json::Value, catalog_id: u64) -> bool {
    if let Some(v) = item.get("qobuz_id") {
        return parse_id_value(v).ok() == Some(catalog_id);
    }
    item.get("id")
        .and_then(|v| parse_id_value(v).ok())
        == Some(catalog_id)
}

fn favorite_item_album_api_id(item: &serde_json::Value) -> Option<String> {
    if let Some(v) = item.get("id") {
        if let Some(short) = parse_album_ref_value(v) {
            return Some(short);
        }
    }
    item.get("slug")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

impl QobuzClient {
    pub async fn favorites_albums(
        &self,
        page: PageRequest,
    ) -> Result<Page<AlbumSummary>, QobuzError> {
        let body = self
            .favorites_get_user_favorites_json(FavoriteType::Albums, page)
            .await?;
        let response: FavoritesAlbumsResponse = serde_json::from_value(body)?;
        Ok(Page {
            items: response.albums.items,
            total: response.albums.total,
            limit: response.albums.limit,
            offset: response.albums.offset,
        })
    }

    pub async fn favorites_all_albums(&self) -> Result<Vec<AlbumSummary>, QobuzError> {
        fetch_all_pages(|page| self.favorites_albums(page)).await
    }

    /// Scan favorites JSON for `qobuz_id` (no per-album `album/get` calls).
    pub async fn favorites_album_api_id_for_catalog(
        &self,
        catalog_id: u64,
    ) -> Result<Option<String>, QobuzError> {
        let limit = 500u32;
        let mut offset = 0u32;
        loop {
            let body = self
                .favorites_get_user_favorites_json(
                    FavoriteType::Albums,
                    PageRequest { limit, offset },
                )
                .await?;
            let block = body.get("albums");
            let total = block
                .and_then(|b| b.get("total"))
                .and_then(|t| t.as_u64())
                .unwrap_or(0) as u32;
            if let Some(items) = block
                .and_then(|b| b.get("items"))
                .and_then(|i| i.as_array())
            {
                for item in items {
                    if !favorite_item_matches_catalog(item, catalog_id) {
                        continue;
                    }
                    if let Some(api_id) = favorite_item_album_api_id(item) {
                        tracing::debug!(
                            qobuz_id = catalog_id,
                            album_api_id = %api_id,
                            "favorites JSON match for catalog id"
                        );
                        return Ok(Some(api_id));
                    }
                    tracing::warn!(
                        qobuz_id = catalog_id,
                        "favorite matched catalog id but has no slug or short id in JSON"
                    );
                }
            }
            offset += limit;
            if offset >= total {
                break;
            }
        }
        Ok(None)
    }

    pub async fn favorites_tracks(
        &self,
        page: PageRequest,
    ) -> Result<Page<TrackSummary>, QobuzError> {
        let body = self
            .favorites_get_user_favorites_json(FavoriteType::Tracks, page)
            .await?;
        let response: FavoritesTracksResponse = serde_json::from_value(body)?;
        Ok(Page {
            items: response.tracks.items,
            total: response.tracks.total,
            limit: response.tracks.limit,
            offset: response.tracks.offset,
        })
    }

    pub async fn favorites_all_tracks(&self) -> Result<Vec<TrackSummary>, QobuzError> {
        fetch_all_pages(|page| self.favorites_tracks(page)).await
    }

    pub async fn favorite_add_albums(&self, ids: &[u64]) -> Result<(), QobuzError> {
        if ids.is_empty() {
            return Ok(());
        }
        let album_ids = ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let params = vec![("album_ids", album_ids)];
        let (status, body) = self.get_json("favorite/create", &params).await?;
        if status != 200 {
            return Err(QobuzError::from_status("favorite/create", status, &body));
        }
        Ok(())
    }

    pub async fn favorite_remove_albums(&self, ids: &[u64]) -> Result<(), QobuzError> {
        if ids.is_empty() {
            return Ok(());
        }
        let album_ids = ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let params = vec![("album_ids", album_ids)];
        let (status, body) = self.get_json("favorite/delete", &params).await?;
        if status != 200 {
            return Err(QobuzError::from_status("favorite/delete", status, &body));
        }
        Ok(())
    }

    async fn favorites_get_user_favorites_json(
        &self,
        favorite_type: FavoriteType,
        page: PageRequest,
    ) -> Result<serde_json::Value, QobuzError> {
        let modes = if self.config.favorites_sign_mode == FavoritesSignMode::None {
            FavoritesSignMode::fallback_order().to_vec()
        } else {
            vec![
                self.config.favorites_sign_mode,
                FavoritesSignMode::TimestampSecret,
                FavoritesSignMode::TimestampOnly,
                FavoritesSignMode::None,
            ]
        };

        let mut seen = std::collections::HashSet::new();
        let mut last_err = QobuzError::InvalidSignature;

        for mode in modes {
            if !seen.insert(mode) {
                continue;
            }
            match self
                .favorites_get_user_favorites_with_mode(favorite_type, page, mode)
                .await
            {
                Ok(resp) => return Ok(resp),
                Err(QobuzError::InvalidSignature | QobuzError::BadRequest(_)) => {
                    last_err = QobuzError::InvalidSignature;
                    continue;
                }
                Err(e) => return Err(e),
            }
        }

        Err(last_err)
    }

    async fn favorites_get_user_favorites_with_mode(
        &self,
        favorite_type: FavoriteType,
        page: PageRequest,
        mode: FavoritesSignMode,
    ) -> Result<serde_json::Value, QobuzError> {
        let request_ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let mut params = vec![
            ("type", favorite_type.as_str().to_string()),
            ("limit", page.limit.to_string()),
            ("offset", page.offset.to_string()),
        ];

        match mode {
            FavoritesSignMode::None => {}
            FavoritesSignMode::TimestampOnly => {
                let sig = sign_favorites(request_ts, None);
                params.push(("request_ts", request_ts.to_string()));
                params.push(("request_sig", sig));
            }
            FavoritesSignMode::TimestampSecret => {
                let secret = self
                    .state
                    .active_secret
                    .as_deref()
                    .ok_or(QobuzError::InvalidSignature)?;
                let sig = sign_favorites(request_ts, Some(secret));
                params.push(("request_ts", request_ts.to_string()));
                params.push(("request_sig", sig));
                params.push(("app_id", self.state.app_id.clone()));
                if let Some(uat) = &self.state.user_auth_token {
                    params.push(("user_auth_token", uat.clone()));
                }
            }
        }

        let (status, body) = self
            .get_json("favorite/getUserFavorites", &params)
            .await?;
        if status == 400 {
            return Err(QobuzError::InvalidSignature);
        }
        if status != 200 {
            return Err(QobuzError::from_status(
                "favorite/getUserFavorites",
                status,
                &body,
            ));
        }

        Ok(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn favorite_item_lookup_by_qobuz_id() {
        let item: serde_json::Value = serde_json::from_str(
            r#"{
                "id": "zg7pv28g4mldg",
                "qobuz_id": 393908828,
                "slug": "lutosawski-concertos",
                "title": "Test"
            }"#,
        )
        .unwrap();
        assert!(favorite_item_matches_catalog(&item, 393908828));
        assert_eq!(
            favorite_item_album_api_id(&item).as_deref(),
            Some("zg7pv28g4mldg")
        );
    }

    #[test]
    fn favorite_item_does_not_match_wrong_catalog_id() {
        let item: serde_json::Value = serde_json::from_str(
            r#"{"id": "abc", "qobuz_id": 1, "title": "X"}"#,
        )
        .unwrap();
        assert!(!favorite_item_matches_catalog(&item, 393908828));
    }

    #[test]
    fn favorite_item_api_id_from_slug_when_id_is_upc_numeric() {
        let item: serde_json::Value = serde_json::from_str(
            r#"{
                "id": 3149020953969,
                "qobuz_id": 393908828,
                "slug": "lutosawski-concertos",
                "title": "Test"
            }"#,
        )
        .unwrap();
        assert!(favorite_item_matches_catalog(&item, 393908828));
        assert_eq!(
            favorite_item_album_api_id(&item).as_deref(),
            Some("lutosawski-concertos")
        );
    }

    #[test]
    fn favorite_item_matches_legacy_id_when_qobuz_id_absent() {
        let item: serde_json::Value = serde_json::from_str(
            r#"{"id": 42, "slug": "my-album", "title": "Test"}"#,
        )
        .unwrap();
        assert!(favorite_item_matches_catalog(&item, 42));
    }
}
