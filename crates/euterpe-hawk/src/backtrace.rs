use std::path::Path;

use backtrace::Backtrace;

use crate::config::HawkConfig;
use crate::event::BacktraceFrame;
use crate::source::read_near_lines;
use crate::trim::is_well_known_not_in_app;

/// Capture stack frames for Hawk payload (latest call first, per Python catcher).
pub fn capture_backtrace(config: &HawkConfig, skip_frames: usize) -> Vec<BacktraceFrame> {
    let bt = Backtrace::new();
    let mut frames = Vec::new();

    for frame in bt.frames().iter().skip(skip_frames) {
        let symbols: Vec<_> = frame.symbols().to_vec();
        if symbols.is_empty() {
            continue;
        }
        for symbol in symbols {
            let Some(file) = symbol.filename().map(|p| p.display().to_string()) else {
                continue;
            };
            let line = symbol.lineno().unwrap_or(0);
            let function = symbol.name().map(|n| n.to_string());
            if config.backtrace_trim && is_well_known_not_in_app(function.as_deref()) {
                continue;
            }
            let mut source_code = Vec::new();
            if config.source_code_enabled && line > 0 {
                source_code = read_near_lines(Path::new(&file), line, config.source_code_lines);
            }
            frames.push(BacktraceFrame {
                file,
                line,
                function,
                source_code,
            });
        }
    }

    frames.reverse();
    frames
}
