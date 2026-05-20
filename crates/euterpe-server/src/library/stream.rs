use std::path::Path;

use axum::body::Body;
use axum::http::{StatusCode, header};
use axum::response::Response;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;

use crate::error::ApiError;
use crate::library::tags::audio_content_type;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ByteRange {
    start: u64,
    end: u64,
}

fn parse_bytes_range(header: &str, file_size: u64) -> Result<ByteRange, ()> {
    if file_size == 0 {
        return Err(());
    }
    let spec = header.strip_prefix("bytes=").ok_or(())?;
    if spec.contains(',') {
        return Err(());
    }
    let (start_s, end_s) = spec.split_once('-').ok_or(())?;
    let last = file_size - 1;
    let (start, end) = if start_s.is_empty() {
        let suffix: u64 = end_s.parse().map_err(|_| ())?;
        if suffix == 0 {
            return Err(());
        }
        let start = file_size.saturating_sub(suffix);
        (start, last)
    } else if end_s.is_empty() {
        let start: u64 = start_s.parse().map_err(|_| ())?;
        if start > last {
            return Err(());
        }
        (start, last)
    } else {
        let start: u64 = start_s.parse().map_err(|_| ())?;
        let end: u64 = end_s.parse().map_err(|_| ())?;
        if start > end || start > last {
            return Err(());
        }
        (start, end.min(last))
    };
    Ok(ByteRange { start, end })
}

pub async fn audio_file_response(
    path: &Path,
    range_header: Option<&str>,
) -> Result<Response, ApiError> {
    let meta = tokio::fs::metadata(path)
        .await
        .map_err(|_| ApiError::Message("audio file not found".into()))?;
    let file_size = meta.len();
    let content_type = audio_content_type(path);

    if let Some(range) = range_header {
        match parse_bytes_range(range, file_size) {
            Ok(r) => {
                let length = r.end - r.start + 1;
                let mut file = File::open(path)
                    .await
                    .map_err(|_| ApiError::Message("audio file not found".into()))?;
                file.seek(std::io::SeekFrom::Start(r.start))
                    .await
                    .map_err(|e| ApiError::Message(e.to_string()))?;
                let stream = ReaderStream::new(file.take(length));
                let content_range = format!("bytes {}-{}/{}", r.start, r.end, file_size);
                return Response::builder()
                    .status(StatusCode::PARTIAL_CONTENT)
                    .header(header::CONTENT_TYPE, content_type)
                    .header(header::ACCEPT_RANGES, "bytes")
                    .header(header::CONTENT_LENGTH, length)
                    .header(header::CONTENT_RANGE, content_range)
                    .body(Body::from_stream(stream))
                    .map_err(|e| ApiError::Message(e.to_string()));
            }
            Err(()) => {
                return Response::builder()
                    .status(StatusCode::RANGE_NOT_SATISFIABLE)
                    .header(header::CONTENT_RANGE, format!("bytes */{file_size}"))
                    .body(Body::empty())
                    .map_err(|e| ApiError::Message(e.to_string()));
            }
        }
    }

    let file = File::open(path)
        .await
        .map_err(|_| ApiError::Message("audio file not found".into()))?;
    let stream = ReaderStream::new(file);
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_LENGTH, file_size)
        .body(Body::from_stream(stream))
        .map_err(|e| ApiError::Message(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_range_prefix() {
        let r = parse_bytes_range("bytes=0-1023", 5000).unwrap();
        assert_eq!(r.start, 0);
        assert_eq!(r.end, 1023);
    }

    #[test]
    fn parse_range_suffix() {
        let r = parse_bytes_range("bytes=-500", 2000).unwrap();
        assert_eq!(r.start, 1500);
        assert_eq!(r.end, 1999);
    }

    #[test]
    fn parse_range_open_end() {
        let r = parse_bytes_range("bytes=100-", 2000).unwrap();
        assert_eq!(r.start, 100);
        assert_eq!(r.end, 1999);
    }
}
