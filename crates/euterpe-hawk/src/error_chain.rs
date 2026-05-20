use std::error::Error;

/// Format `Error::source()` chain for Hawk `payload.description`.
pub fn format_error_chain(err: &dyn Error) -> Option<String> {
    let mut sources = Vec::new();
    let mut current = err.source();
    while let Some(src) = current {
        sources.push(src.to_string());
        current = src.source();
    }
    if sources.is_empty() {
        None
    } else {
        Some(sources.join(": "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;
    use thiserror::Error;

    #[derive(Debug, Error)]
    #[error("outer: {0}")]
    struct Outer(#[from] io::Error);

    #[test]
    fn chains_multiple_sources() {
        let err = Outer(io::Error::new(io::ErrorKind::NotFound, "missing file"));
        let desc = format_error_chain(&err).expect("description");
        assert!(desc.contains("missing file"));
    }
}
