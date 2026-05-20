use std::fs;
use std::path::Path;

use crate::event::SourceLine;

/// Read ±`margin` lines around `line` (1-based), matching hawk.python `get_near_filelines`.
pub fn read_near_lines(path: &Path, line: u32, margin: usize) -> Vec<SourceLine> {
    let Ok(content) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() || line == 0 {
        return Vec::new();
    }
    let error_idx = line.saturating_sub(1) as usize;
    let start = error_idx.saturating_sub(margin);
    let end = (error_idx + margin + 1).min(lines.len());
    (start..end)
        .map(|idx| SourceLine {
            line: (idx + 1) as u32,
            content: lines[idx].to_string(),
        })
        .collect()
}
