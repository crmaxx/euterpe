use crate::client::QobuzClient;
use crate::error::QobuzError;
use crate::models::{AlbumSummary, FavoriteType, FavoritesAlbumsResponse};
use crate::pagination::{fetch_all_pages, Page, PageRequest};
use crate::signing::{sign_favorites, FavoritesSignMode};

impl QobuzClient {
    pub async fn favorites_albums(
        &self,
        page: PageRequest,
    ) -> Result<Page<AlbumSummary>, QobuzError> {
        let response = self
            .favorites_get_user_favorites(FavoriteType::Albums, page)
            .await?;
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

    async fn favorites_get_user_favorites(
        &self,
        favorite_type: FavoriteType,
        page: PageRequest,
    ) -> Result<FavoritesAlbumsResponse, QobuzError> {
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
    ) -> Result<FavoritesAlbumsResponse, QobuzError> {
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
                let secret = self.state.active_secret.as_deref().ok_or(
                    QobuzError::InvalidAppSecret,
                )?;
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

        serde_json::from_value(body).map_err(QobuzError::from)
    }
}
