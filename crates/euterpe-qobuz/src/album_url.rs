//! Parse Qobuz album URLs and bare album refs for `album/get`.

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AlbumUrlError {
    #[error("url must not be empty")]
    Empty,
    #[error("not a Qobuz album URL or album id")]
    NotAlbum,
}

/// Extract the `album_id` argument for `album/get` from a play.qobuz.com URL, www.qobuz.com link, or bare ref.
pub fn parse_album_url(input: &str) -> Result<String, AlbumUrlError> {
    let s = input.trim();
    if s.is_empty() {
        return Err(AlbumUrlError::Empty);
    }

    if !s.contains("://") && !s.chars().any(char::is_whitespace) {
        return Ok(s.to_string());
    }

    let lower = s.to_lowercase();
    if lower.contains("/track/")
        || lower.contains("/artist/")
        || lower.contains("/playlist/")
        || lower.contains("/label/")
    {
        return Err(AlbumUrlError::NotAlbum);
    }

    let marker = "/album/";
    let idx = lower
        .find(marker)
        .ok_or(AlbumUrlError::NotAlbum)?;
    let rest = &s[idx + marker.len()..];
    let path = rest.split(['?', '#']).next().unwrap_or(rest);
    let segments: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    if segments.is_empty() {
        return Err(AlbumUrlError::NotAlbum);
    }

    let last = segments.last().copied().unwrap();
    if segments.len() >= 2 && last.chars().all(|c| c.is_ascii_digit()) {
        return Ok(last.to_string());
    }
    Ok(segments[0].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_ref_and_catalog_id() {
        assert_eq!(
            parse_album_url("zg7pv28g4mldg").unwrap(),
            "zg7pv28g4mldg"
        );
        assert_eq!(parse_album_url("393908828").unwrap(), "393908828");
    }

    #[test]
    fn play_urls() {
        assert_eq!(
            parse_album_url("https://play.qobuz.com/album/zg7pv28g4mldg").unwrap(),
            "zg7pv28g4mldg"
        );
        assert_eq!(
            parse_album_url("https://play.qobuz.com/us-en/album/my-slug/393908828").unwrap(),
            "393908828"
        );
        assert_eq!(
            parse_album_url("https://www.qobuz.com/fr-fr/album/foo-bar/123?utm=1").unwrap(),
            "123"
        );
    }

    #[test]
    fn rejects_non_album() {
        assert_eq!(
            parse_album_url("https://play.qobuz.com/track/1"),
            Err(AlbumUrlError::NotAlbum)
        );
        assert_eq!(parse_album_url(""), Err(AlbumUrlError::Empty));
        assert_eq!(
            parse_album_url("https://example.com/"),
            Err(AlbumUrlError::NotAlbum)
        );
    }
}
