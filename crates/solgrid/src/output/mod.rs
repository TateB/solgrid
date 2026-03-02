pub mod json;
pub mod text;

use solgrid_diagnostics::FileResult;

/// Print results using the specified output format.
pub fn print_results(results: &[FileResult], format: &str) {
    match format {
        "json" => json::print_results(results),
        _ => text::print_results(results),
    }
}
