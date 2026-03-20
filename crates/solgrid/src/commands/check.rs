use crate::cache::Cache;
use crate::output;
use crate::Cli;
use rayon::prelude::*;
use solgrid_config::{resolve_config, Config, ConfigResolver};
use solgrid_diagnostics::{FileResult, Severity};
use solgrid_linter::LintEngine;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub fn run(paths: &[PathBuf], cli: &Cli) -> i32 {
    let explicit_config = super::load_explicit_config(cli);

    if cli.stdin {
        let config = explicit_config
            .unwrap_or_else(|| resolve_config(&std::env::current_dir().unwrap_or_default()));
        return run_stdin(&config, cli);
    }

    let mut discovery_resolver = ConfigResolver::new(explicit_config.clone());
    let files = super::discover_sol_files(paths, &mut discovery_resolver);

    if files.is_empty() {
        eprintln!("No .sol files found");
        return 0;
    }

    let thread_probe = super::thread_probe_path(paths);
    let thread_count = discovery_resolver
        .resolve_for_path(&thread_probe)
        .global
        .threads;

    let engine = LintEngine::new();
    let resolver = Arc::new(Mutex::new(ConfigResolver::new(explicit_config)));
    let caches = if cli.no_cache {
        None
    } else {
        Some(Arc::new(Mutex::new(HashMap::<String, Cache>::new())))
    };

    let results: Vec<FileResult> = super::install_with_thread_count(thread_count, || {
        files
            .par_iter()
            .map(|path| {
                let config = resolver
                    .lock()
                    .expect("config resolver poisoned")
                    .resolve_for_path(path);
                let config_hash = crate::cache::sha256_hex(&format!("{:?}", config));
                let path_str = path.display().to_string();

                if let Ok(content) = std::fs::read_to_string(path) {
                    if let Some(ref caches) = caches {
                        let mut caches = caches.lock().expect("cache map poisoned");
                        let cache = caches
                            .entry(config.global.cache_dir.clone())
                            .or_insert_with(|| Cache::load(Path::new(&config.global.cache_dir)));

                        if let Some(entry) = cache.check(&path_str, &content, &config_hash) {
                            if entry.diagnostic_count == 0 {
                                return FileResult {
                                    path: path_str,
                                    diagnostics: vec![],
                                };
                            }
                        }
                    }

                    let result = engine.lint_source(&content, path, &config);

                    if let Some(ref caches) = caches {
                        let mut caches = caches.lock().expect("cache map poisoned");
                        let cache = caches
                            .entry(config.global.cache_dir.clone())
                            .or_insert_with(|| Cache::load(Path::new(&config.global.cache_dir)));
                        cache.update(
                            &result.path,
                            &content,
                            &config_hash,
                            result.diagnostics.len(),
                            false,
                        );
                    }

                    result
                } else {
                    engine.lint_file(path, &config)
                }
            })
            .collect()
    });

    if let Some(caches) = caches {
        let caches = caches.lock().expect("cache map poisoned");
        for cache in caches.values() {
            if let Err(error) = cache.save() {
                eprintln!("warning: failed to save cache: {error}");
            }
        }
    }

    let total_diagnostics: usize = results.iter().map(|r| r.diagnostics.len()).sum();
    let has_errors = results
        .iter()
        .flat_map(|r| &r.diagnostics)
        .any(|d| d.severity == Severity::Error);

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

    let engine = LintEngine::new();
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
