use md5::{Digest, Md5};

/// How to sign `favorite/getUserFavorites` requests.
///
/// Live API behavior varies; `euterpe-qobuz` tries modes in fallback order on 400.
/// Reference: `docs/references/qobuz-sync` (TimestampOnly), `qobuz-dl/qopy.py` (TimestampSecret),
/// `streamrip` (None).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum FavoritesSignMode {
    /// streamrip: no `request_ts` / `request_sig`.
    None,
    /// qobuz-sync: `md5("favoritegetUserFavorites" + timestamp)`.
    #[default]
    TimestampOnly,
    /// qopy: `md5("favoritegetUserFavorites" + timestamp + secret)`.
    TimestampSecret,
}

impl FavoritesSignMode {
    pub fn fallback_order() -> &'static [Self] {
        &[Self::TimestampSecret, Self::TimestampOnly, Self::None]
    }
}

pub fn md5_hex(input: &str) -> String {
    let digest = Md5::digest(input.as_bytes());
    format!("{:x}", digest)
}

/// Sign `track/getFileUrl` (qopy / streamrip use `time.time()` float in the sig string).
pub fn sign_track_file_url(format_id: u8, track_id: u64, request_ts: f64, secret: &str) -> String {
    let raw = format!(
        "trackgetFileUrlformat_id{format_id}intentstreamtrack_id{track_id}{request_ts}{secret}"
    );
    md5_hex(&raw)
}

pub fn sign_favorites(request_ts: i64, secret: Option<&str>) -> String {
    let raw = match secret {
        Some(sec) => format!("favoritegetUserFavorites{request_ts}{sec}"),
        None => format!("favoritegetUserFavorites{request_ts}"),
    };
    md5_hex(&raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_track_file_url_golden() {
        let sig = sign_track_file_url(6, 12_345_678, 1_715_900_000.0, "test_secret");
        assert_eq!(sig, "968e435667d547d87708bc87e09ce2d0");
    }

    #[test]
    fn sign_favorites_timestamp_only_golden() {
        let sig = sign_favorites(1_715_900_000, None);
        assert_eq!(sig, "1d50d7893bc7009d1930672d55127ba7");
    }

    #[test]
    fn sign_favorites_timestamp_secret_golden() {
        let sig = sign_favorites(1_715_900_000, Some("test_secret"));
        assert_eq!(sig, "bdfd6f5af83e82b8052b74cfbafeb992");
    }
}
