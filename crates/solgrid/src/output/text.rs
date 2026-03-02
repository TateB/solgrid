use solgrid_diagnostics::{FileResult, Severity};

/// Print results as colored text output.
pub fn print_results(results: &[FileResult]) {
    for result in results {
        if result.diagnostics.is_empty() {
            continue;
        }

        for diag in &result.diagnostics {
            // Compute line and column from source offset
            let (line, col) = offset_to_line_col_from_file(&result.path, diag.span.start);

            let severity_str = match diag.severity {
                Severity::Error => "\x1b[31merror\x1b[0m",
                Severity::Warning => "\x1b[33mwarning\x1b[0m",
                Severity::Info => "\x1b[36minfo\x1b[0m",
            };

            println!(
                "  \x1b[1m{}:{}:{}\x1b[0m: {}: {} [{}]",
                result.path, line, col, severity_str, diag.message, diag.rule_id
            );
        }
    }
}

/// Read a file and compute line/col from byte offset.
/// Falls back to (1, 1) if file can't be read.
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
