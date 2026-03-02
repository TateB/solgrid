//! GitHub Actions annotation output format.
//!
//! Produces `::error`, `::warning`, and `::notice` annotations that GitHub
//! Actions surfaces inline on pull requests.

use solgrid_diagnostics::{FileResult, Severity};

/// Print results as GitHub Actions workflow annotations.
pub fn print_results(results: &[FileResult]) {
    for result in results {
        for diag in &result.diagnostics {
            let (line, col) = offset_to_line_col_from_file(&result.path, diag.span.start);

            let level = match diag.severity {
                Severity::Error => "error",
                Severity::Warning => "warning",
                Severity::Info => "notice",
            };

            println!(
                "::{level} file={},line={line},col={col},title={}::{}",
                result.path, diag.rule_id, diag.message
            );
        }
    }
}

/// Read a file and compute line/col from byte offset.
fn offset_to_line_col_from_file(path: &str, offset: usize) -> (usize, usize) {
    if let Ok(source) = std::fs::read_to_string(path) {
        offset_to_line_col(&source, offset)
    } else {
        (1, 1)
    }
}

fn offset_to_line_col(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    for (i, ch) in source.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}
