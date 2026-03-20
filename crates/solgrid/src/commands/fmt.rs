use crate::cache::Cache;
use crate::Cli;
use rayon::prelude::*;
use solgrid_config::{resolve_config, Config, ConfigResolver};
use solgrid_formatter::{check_formatted, format_source};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub fn run(paths: &[PathBuf], cli: &Cli) -> i32 {
    let explicit_config = super::load_explicit_config(cli);

    if cli.stdin {
        let config = explicit_config
            .unwrap_or_else(|| resolve_config(&std::env::current_dir().unwrap_or_default()));
        return run_stdin(&config);
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
    let resolver = Arc::new(Mutex::new(ConfigResolver::new(explicit_config)));
    let caches = if cli.no_cache {
        None
    } else {
        Some(Arc::new(Mutex::new(HashMap::<String, Cache>::new())))
    };

    let outcomes = super::install_with_thread_count(thread_count, || {
        files
            .par_iter()
            .map(|path| {
                let config = resolver
                    .lock()
                    .expect("config resolver poisoned")
                    .resolve_for_path(path);
                let config_hash = crate::cache::sha256_hex(&format!("{:?}", config));
                let cache_dir = config.global.cache_dir.clone();

                let source = match std::fs::read_to_string(path) {
                    Ok(source) => source,
                    Err(error) => {
                        return FormatOutcome {
                            path: path.clone(),
                            cache_dir,
                            config_hash,
                            source: None,
                            status: FormatStatus::Error(format!(
                                "Error reading {}: {error}",
                                path.display()
                            )),
                        };
                    }
                };

                if let Some(ref caches) = caches {
                    let mut caches = caches.lock().expect("cache map poisoned");
                    let cache = caches
                        .entry(cache_dir.clone())
                        .or_insert_with(|| Cache::load(Path::new(&cache_dir)));
                    let path_str = path.display().to_string();
                    if let Some(entry) = cache.check(&path_str, &source, &config_hash) {
                        if entry.is_formatted {
                            return FormatOutcome {
                                path: path.clone(),
                                cache_dir,
                                config_hash,
                                source: Some(source),
                                status: FormatStatus::CachedFormatted,
                            };
                        }
                    }
                }

                let status = if cli.diff {
                    match check_formatted(&source, &config.format) {
                        Ok(true) => FormatStatus::AlreadyFormatted,
                        Ok(false) => FormatStatus::WouldReformat,
                        Err(error) => FormatStatus::Error(format!(
                            "Error formatting {}: {error}",
                            path.display()
                        )),
                    }
                } else {
                    match format_source(&source, &config.format) {
                        Ok(formatted) => {
                            if formatted == source {
                                FormatStatus::AlreadyFormatted
                            } else {
                                FormatStatus::Formatted(formatted)
                            }
                        }
                        Err(error) => FormatStatus::Error(format!(
                            "Error formatting {}: {error}",
                            path.display()
                        )),
                    }
                };

                FormatOutcome {
                    path: path.clone(),
                    cache_dir,
                    config_hash,
                    source: Some(source),
                    status,
                }
            })
            .collect::<Vec<_>>()
    });

    let mut changed = 0usize;
    let mut errors = 0usize;

    for outcome in outcomes {
        match outcome.status {
            FormatStatus::CachedFormatted => {}
            FormatStatus::AlreadyFormatted => {
                if let (Some(ref caches), Some(source)) = (&caches, &outcome.source) {
                    let mut caches = caches.lock().expect("cache map poisoned");
                    let cache = caches
                        .entry(outcome.cache_dir.clone())
                        .or_insert_with(|| Cache::load(Path::new(&outcome.cache_dir)));
                    cache.update(
                        &outcome.path.display().to_string(),
                        source,
                        &outcome.config_hash,
                        0,
                        true,
                    );
                }
            }
            FormatStatus::WouldReformat => {
                eprintln!("Would reformat: {}", outcome.path.display());
                changed += 1;
            }
            FormatStatus::Formatted(formatted) => {
                if cli.diff {
                    unreachable!("formatted output is only produced in write mode");
                }

                if let Err(error) = std::fs::write(&outcome.path, &formatted) {
                    eprintln!("Error writing {}: {error}", outcome.path.display());
                    errors += 1;
                } else {
                    changed += 1;
                    if let Some(ref caches) = caches {
                        let mut caches = caches.lock().expect("cache map poisoned");
                        let cache = caches
                            .entry(outcome.cache_dir.clone())
                            .or_insert_with(|| Cache::load(Path::new(&outcome.cache_dir)));
                        cache.update(
                            &outcome.path.display().to_string(),
                            &formatted,
                            &outcome.config_hash,
                            0,
                            true,
                        );
                    }
                }
            }
            FormatStatus::Error(message) => {
                eprintln!("{message}");
                errors += 1;
            }
        }
    }

    if let Some(caches) = caches {
        let caches = caches.lock().expect("cache map poisoned");
        for cache in caches.values() {
            if let Err(error) = cache.save() {
                eprintln!("warning: failed to save cache: {error}");
            }
        }
    }

    if changed > 0 {
        if cli.diff {
            eprintln!("{changed} file(s) would be reformatted");
        } else {
            eprintln!("Reformatted {changed} file(s)");
        }
    }

    if errors > 0 || (cli.diff && changed > 0) {
        1
    } else {
        0
    }
}

fn run_stdin(config: &Config) -> i32 {
    use std::io::Read;

    let mut source = String::new();
    if let Err(e) = std::io::stdin().read_to_string(&mut source) {
        eprintln!("Error reading stdin: {e}");
        return 1;
    }

    match format_source(&source, &config.format) {
        Ok(formatted) => {
            print!("{formatted}");
            0
        }
        Err(e) => {
            eprintln!("Error formatting: {e}");
            1
        }
    }
}

struct FormatOutcome {
    path: PathBuf,
    cache_dir: String,
    config_hash: String,
    source: Option<String>,
    status: FormatStatus,
}

enum FormatStatus {
    CachedFormatted,
    AlreadyFormatted,
    WouldReformat,
    Formatted(String),
    Error(String),
}
