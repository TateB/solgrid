use crate::cache::sha256_hex;
use crate::output;
use crate::Cli;
use rayon::prelude::*;
use solgrid_config::{resolve_config, Config};
use solgrid_diagnostics::{FileResult, Severity};
use solgrid_linter::LintEngine;
use std::path::{Path, PathBuf};

pub fn run(paths: &[PathBuf], cli: &Cli) -> i32 {
    let explicit_config = super::load_explicit_config(cli);

    if cli.stdin {
        let config = explicit_config.unwrap_or_else(|| {
            resolve_config(&std::env::current_dir().unwrap_or_default()).unwrap_or_else(|error| {
                eprintln!("Error loading config: {error}");
                std::process::exit(2);
            })
        });
        return run_stdin(&config, cli);
    }

    let prepared = super::prepare_files(paths, explicit_config).unwrap_or_else(|error| {
        eprintln!("Error loading config: {error}");
        std::process::exit(2);
    });

    if prepared.files.is_empty() {
        eprintln!("No .sol files found");
        return 0;
    }

    let mut caches = if cli.no_cache {
        None
    } else {
        Some(super::preload_caches(&prepared.files))
    };

    let results: Vec<(FileResult, Option<super::CacheUpdate>)> =
        super::install_with_thread_count(prepared.thread_count, || {
            prepared
                .files
                .par_iter()
                .map(|file| {
                    let content = match std::fs::read_to_string(&file.path) {
                        Ok(content) => content,
                        Err(error) => {
                            return (super::file_read_error_result(&file.path, &error), None);
                        }
                    };

                    let content_hash = sha256_hex(&content);
                    if let Some(caches) = &caches {
                        if let Some(cache) = caches.get(&file.cache_dir) {
                            if let Some(entry) = cache.check_hashed(
                                &file.path_display,
                                &content_hash,
                                &file.config_hash,
                            ) {
                                if entry.diagnostic_count == 0 {
                                    return (
                                        FileResult {
                                            path: file.path_display.clone(),
                                            diagnostics: vec![],
                                        },
                                        None,
                                    );
                                }
                            }
                        }
                    }

                    let engine = LintEngine::with_remappings((*file.remappings).clone());
                    let result = engine.lint_source(&content, &file.path, &file.config);
                    let update = super::CacheUpdate {
                        cache_dir: file.cache_dir.clone(),
                        path: result.path.clone(),
                        content_hash,
                        config_hash: file.config_hash.clone(),
                        diagnostic_count: result.diagnostics.len(),
                        is_formatted: false,
                    };

                    (result, Some(update))
                })
                .collect()
        });

    if let Some(caches) = &mut caches {
        super::apply_cache_updates(
            caches,
            results
                .iter()
                .filter_map(|(_, update)| update.as_ref().cloned()),
        );
        super::save_caches(caches);
    }

    let total_diagnostics: usize = results
        .iter()
        .map(|(result, _)| result.diagnostics.len())
        .sum();
    let has_errors = results
        .iter()
        .flat_map(|(result, _)| &result.diagnostics)
        .any(|diagnostic| diagnostic.severity == Severity::Error);

    let results: Vec<FileResult> = if cli.quiet {
        results
            .into_iter()
            .map(|(result, _)| FileResult {
                path: result.path,
                diagnostics: result
                    .diagnostics
                    .into_iter()
                    .filter(|diagnostic| diagnostic.severity == Severity::Error)
                    .collect(),
            })
            .collect()
    } else {
        results.into_iter().map(|(result, _)| result).collect()
    };

    output::print_results(&results, &cli.output_format);

    if total_diagnostics > 0 {
        let file_count = results
            .iter()
            .filter(|result| !result.diagnostics.is_empty())
            .count();
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

    let current_dir = std::env::current_dir().unwrap_or_default();
    let engine = LintEngine::with_remappings(super::load_workspace_remappings(&current_dir));
    let result = engine.lint_source(&source, Path::new("<stdin>"), config);

    let has_errors = result
        .diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error);

    let results = vec![result];

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
