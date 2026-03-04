use crate::cache::Cache;
use crate::Cli;
use solgrid_config::{resolve_config, Config};
use solgrid_formatter::{check_formatted, format_source};
use std::path::{Path, PathBuf};

pub fn run(paths: &[PathBuf], cli: &Cli) -> i32 {
    let config = load_config(cli);

    // Handle --stdin mode
    if cli.stdin {
        return run_stdin(&config);
    }

    let files = super::discover_sol_files(paths);

    if files.is_empty() {
        eprintln!("No .sol files found");
        return 0;
    }

    // Load cache unless --no-cache
    let config_hash = crate::cache::sha256_hex(&format!("{:?}", config));
    let mut cache = if !cli.no_cache {
        Some(Cache::load(
            Path::new(&config.global.cache_dir),
            &config_hash,
        ))
    } else {
        None
    };

    let mut changed = 0usize;
    let mut errors = 0usize;

    for path in &files {
        let source = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Error reading {}: {e}", path.display());
                errors += 1;
                continue;
            }
        };

        let path_str = path.display().to_string();

        // Check cache — skip files that are already formatted
        if let Some(ref cache) = cache {
            if let Some(entry) = cache.check(&path_str, &source) {
                if entry.is_formatted {
                    continue;
                }
            }
        }

        if cli.diff {
            // Check mode: just report if files are not formatted
            match check_formatted(&source, &config.format) {
                Ok(true) => {
                    if let Some(ref mut cache) = cache {
                        cache.update(&path_str, &source, 0, true);
                    }
                }
                Ok(false) => {
                    eprintln!("Would reformat: {}", path.display());
                    changed += 1;
                }
                Err(e) => {
                    eprintln!("Error formatting {}: {e}", path.display());
                    errors += 1;
                }
            }
        } else {
            // Format mode: format and write
            match format_source(&source, &config.format) {
                Ok(formatted) => {
                    if formatted != source {
                        if let Err(e) = std::fs::write(path, &formatted) {
                            eprintln!("Error writing {}: {e}", path.display());
                            errors += 1;
                        } else {
                            changed += 1;
                            if let Some(ref mut cache) = cache {
                                cache.update(&path_str, &formatted, 0, true);
                            }
                        }
                    } else if let Some(ref mut cache) = cache {
                        cache.update(&path_str, &source, 0, true);
                    }
                }
                Err(e) => {
                    eprintln!("Error formatting {}: {e}", path.display());
                    errors += 1;
                }
            }
        }
    }

    // Save cache
    if let Some(ref cache) = cache {
        if let Err(e) = cache.save() {
            eprintln!("warning: failed to save cache: {e}");
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
