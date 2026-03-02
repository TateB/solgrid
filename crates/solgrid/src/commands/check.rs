use crate::output;
use crate::Cli;
use rayon::prelude::*;
use solgrid_config::{resolve_config, Config};
use solgrid_diagnostics::{FileResult, Severity};
use solgrid_linter::LintEngine;
use std::path::PathBuf;

pub fn run(paths: &[PathBuf], cli: &Cli) -> i32 {
    let config = load_config(cli);
    let engine = LintEngine::new();
    let files = super::discover_sol_files(paths);

    if files.is_empty() {
        eprintln!("No .sol files found");
        return 0;
    }

    let results: Vec<FileResult> = files
        .par_iter()
        .map(|path| engine.lint_file(path, &config))
        .collect();

    let total_diagnostics: usize = results.iter().map(|r| r.diagnostics.len()).sum();
    let has_errors = results
        .iter()
        .flat_map(|r| &r.diagnostics)
        .any(|d| d.severity == Severity::Error);

    // Filter by quiet mode
    let results: Vec<FileResult> = if cli.quiet {
        results
            .into_iter()
            .map(|r| FileResult {
                path: r.path,
                diagnostics: r
                    .diagnostics
                    .into_iter()
                    .filter(|d| d.severity == Severity::Error)
                    .collect(),
            })
            .collect()
    } else {
        results
    };

    output::print_results(&results, &cli.output_format);

    if total_diagnostics > 0 {
        let file_count = results.iter().filter(|r| !r.diagnostics.is_empty()).count();
        eprintln!(
            "\nFound {total_diagnostics} issue(s) in {file_count} file(s)"
        );
    }

    if has_errors { 1 } else if total_diagnostics > 0 { 0 } else { 0 }
}

fn load_config(cli: &Cli) -> Config {
    if let Some(config_path) = &cli.config {
        match solgrid_config::load_config(config_path) {
            Ok(config) => config,
            Err(e) => {
                eprintln!("Error loading config: {e}");
                std::process::exit(2);
            }
        }
    } else {
        resolve_config(&std::env::current_dir().unwrap_or_default())
    }
}
