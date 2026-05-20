const WELL_KNOWN_NOT_IN_APP: &[&str] = &[
    "std::",
    "core::",
    "alloc::",
    "backtrace::",
    "__rust_",
    "___rust_",
    "rust_begin_unwind",
    "tokio::",
    "tower::",
    "hyper::",
    "axum::",
    "euterpe_hawk::",
    "tracing::",
    "tracing_core::",
];

/// Returns true if the frame should be dropped from the Hawk backtrace.
pub fn is_well_known_not_in_app(function: Option<&str>) -> bool {
    let Some(func) = function else {
        return false;
    };
    WELL_KNOWN_NOT_IN_APP
        .iter()
        .any(|prefix| func.starts_with(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_tokio_frames() {
        assert!(is_well_known_not_in_app(Some(
            "tokio::runtime::scheduler::multi_thread::worker"
        )));
        assert!(!is_well_known_not_in_app(Some(
            "euterpe_server::services::library_scan::run_scan"
        )));
    }
}
