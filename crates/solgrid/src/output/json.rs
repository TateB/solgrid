use solgrid_diagnostics::FileResult;

/// Print results as JSON.
pub fn print_results(results: &[FileResult]) {
    let output: Vec<&FileResult> = results
        .iter()
        .filter(|r| !r.diagnostics.is_empty())
        .collect();

    match serde_json::to_string_pretty(&output) {
        Ok(json) => println!("{json}"),
        Err(e) => eprintln!("Error serializing JSON: {e}"),
    }
}
