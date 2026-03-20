use crate::cache::Cache;
use crate::output;
use crate::Cli;
use rayon::prelude::*;
use solgrid_config::{resolve_config, Config};
use solgrid_diagnostics::{FileResult, Severity};
use solgrid_linter::LintEngine;
use std::path::{Path, PathBuf};

pub fn run(paths: &[PathBuf], cli: &Cli) -> i32 {
    let config = load_config(cli);

    // Handle --stdin mode
    if cli.stdin {
        return run_stdin(&config, cli);
    }

    let workspace_root =
        solgrid_config::find_workspace_root(&std::env::current_dir().unwrap_or_default());
    let remappings = workspace_root
        .map(|root| solgrid_config::load_remappings(&root))
        .unwrap_or_default();
    let engine = LintEngine::with_remappings(remappings);
    let files = super::discover_sol_files(paths);

    if files.is_empty() {
        eprintln!("No .sol files found");
        return 0;
    }

    // Load cache unless --no-cache
    let config_hash = crate::cache::sha256_hex(&format!("{:?}", config));
    let cache = if !cli.no_cache {
        Some(Cache::load(
            Path::new(&config.global.cache_dir),
            &config_hash,
        ))
    } else {
        None
    };

    let results: Vec<FileResult> = files
        .par_iter()
        .map(|path| {
            let path_str = path.display().to_string();

            // Check cache — skip files with 0 diagnostics that haven't changed
            if let Some(ref cache) = cache {
                if let Ok(content) = std::fs::read_to_string(path) {
                    if let Some(entry) = cache.check(&path_str, &content) {
                        if entry.diagnostic_count == 0 {
                            return FileResult {
                                path: path_str,
                                diagnostics: vec![],
                            };
                        }
                    }
                }
            }

            engine.lint_file(path, &config)
        })
        .collect();

    // Update cache
    if let Some(mut cache) = cache {
        for result in &results {
            if let Ok(content) = std::fs::read_to_string(&result.path) {
                cache.update(&result.path, &content, result.diagnostics.len(), false);
            }
        }
        if let Err(e) = cache.save() {
            eprintln!("warning: failed to save cache: {e}");
        }
    }

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
        eprintln!("\nFound {total_diagnostics} issue(s) in {file_count} file(s)");
    }

    if has_errors {
        1
    } else {
        0
    }
}

fn run_stdin(config: &Config, cli: &Cli) -> i32 {
    use std::io::Read;

    let mut source = String::new();
    if let Err(e) = std::io::stdin().read_to_string(&mut source) {
        eprintln!("Error reading stdin: {e}");
        return 1;
    }

    let workspace_root =
        solgrid_config::find_workspace_root(&std::env::current_dir().unwrap_or_default());
    let remappings = workspace_root
        .map(|root| solgrid_config::load_remappings(&root))
        .unwrap_or_default();
    let engine = LintEngine::with_remappings(remappings);
    let result = engine.lint_source(&source, Path::new("<stdin>"), config);

    let has_errors = result
        .diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error);

    let results = vec![result];

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

    if has_errors {
        1
    } else {
        0
    }
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
