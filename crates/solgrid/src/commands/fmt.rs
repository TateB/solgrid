use crate::cache::sha256_hex;
use crate::Cli;
use rayon::prelude::*;
use solgrid_config::{resolve_config, Config};
use solgrid_formatter::{check_formatted, format_source};
use std::path::PathBuf;

pub fn run(paths: &[PathBuf], cli: &Cli) -> i32 {
    let explicit_config = super::load_explicit_config(cli);

    if cli.stdin {
        let config = explicit_config.unwrap_or_else(|| {
            resolve_config(&std::env::current_dir().unwrap_or_default()).unwrap_or_else(|error| {
                eprintln!("Error loading config: {error}");
                std::process::exit(2);
            })
        });
        return run_stdin(&config);
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

    let outcomes = super::install_with_thread_count(prepared.thread_count, || {
        prepared
            .files
            .par_iter()
            .map(|file| {
                let source = match std::fs::read_to_string(&file.path) {
                    Ok(source) => source,
                    Err(error) => {
                        return FormatOutcome {
                            path: file.path.clone(),
                            path_display: file.path_display.clone(),
                            cache_dir: file.cache_dir.clone(),
                            config_hash: file.config_hash.clone(),
                            status: FormatStatus::Error(format!(
                                "Error reading {}: {error}",
                                file.path.display()
                            )),
                        };
                    }
                };

                let source_hash = sha256_hex(&source);
                if let Some(caches) = &caches {
                    if let Some(cache) = caches.get(&file.cache_dir) {
                        if let Some(entry) =
                            cache.check_hashed(&file.path_display, &source_hash, &file.config_hash)
                        {
                            if entry.is_formatted {
                                return FormatOutcome {
                                    path: file.path.clone(),
                                    path_display: file.path_display.clone(),
                                    cache_dir: file.cache_dir.clone(),
                                    config_hash: file.config_hash.clone(),
                                    status: FormatStatus::CachedFormatted,
                                };
                            }
                        }
                    }
                }

                let status = if cli.diff {
                    match check_formatted(&source, &file.config.format) {
                        Ok(true) => FormatStatus::AlreadyFormatted {
                            content_hash: source_hash,
                        },
                        Ok(false) => FormatStatus::WouldReformat,
                        Err(error) => FormatStatus::Error(format!(
                            "Error formatting {}: {error}",
                            file.path.display()
                        )),
                    }
                } else {
                    match format_source(&source, &file.config.format) {
                        Ok(formatted) => {
                            if formatted == source {
                                FormatStatus::AlreadyFormatted {
                                    content_hash: source_hash,
                                }
                            } else {
                                FormatStatus::Formatted { formatted }
                            }
                        }
                        Err(error) => FormatStatus::Error(format!(
                            "Error formatting {}: {error}",
                            file.path.display()
                        )),
                    }
                };

                FormatOutcome {
                    path: file.path.clone(),
                    path_display: file.path_display.clone(),
                    cache_dir: file.cache_dir.clone(),
                    config_hash: file.config_hash.clone(),
                    status,
                }
            })
            .collect::<Vec<_>>()
    });

    let mut changed = 0usize;
    let mut errors = 0usize;
    let mut cache_updates = Vec::new();

    for outcome in outcomes {
        match outcome.status {
            FormatStatus::CachedFormatted => {}
            FormatStatus::AlreadyFormatted { content_hash } => {
                cache_updates.push(super::CacheUpdate {
                    cache_dir: outcome.cache_dir,
                    path: outcome.path_display,
                    content_hash,
                    config_hash: outcome.config_hash,
                    diagnostic_count: 0,
                    is_formatted: true,
                });
            }
            FormatStatus::WouldReformat => {
                eprintln!("Would reformat: {}", outcome.path.display());
                changed += 1;
            }
            FormatStatus::Formatted { formatted } => {
                if cli.diff {
                    unreachable!("formatted output is only produced in write mode");
                }

                if let Err(error) = std::fs::write(&outcome.path, &formatted) {
                    eprintln!("Error writing {}: {error}", outcome.path.display());
                    errors += 1;
                } else {
                    changed += 1;
                    cache_updates.push(super::CacheUpdate {
                        cache_dir: outcome.cache_dir,
                        path: outcome.path_display,
                        content_hash: sha256_hex(&formatted),
                        config_hash: outcome.config_hash,
                        diagnostic_count: 0,
                        is_formatted: true,
                    });
                }
            }
            FormatStatus::Error(message) => {
                eprintln!("{message}");
                errors += 1;
            }
        }
    }

    if let Some(caches) = &mut caches {
        super::apply_cache_updates(caches, cache_updates);
        super::save_caches(caches);
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
    path_display: String,
    cache_dir: String,
    config_hash: String,
    status: FormatStatus,
}

enum FormatStatus {
    CachedFormatted,
    AlreadyFormatted { content_hash: String },
    WouldReformat,
    Formatted { formatted: String },
    Error(String),
}
