use crate::Cli;
use solgrid_config::{resolve_config, Config};
use solgrid_formatter::{check_formatted, format_source};
use std::path::PathBuf;

pub fn run(paths: &[PathBuf], cli: &Cli) -> i32 {
    let config = load_config(cli);
    let files = super::discover_sol_files(paths);

    if files.is_empty() {
        eprintln!("No .sol files found");
        return 0;
    }

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

        if cli.diff {
            // Check mode: just report if files are not formatted
            match check_formatted(&source, &config.format) {
                Ok(true) => {} // Already formatted
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
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error formatting {}: {e}", path.display());
                    errors += 1;
                }
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

    if errors > 0 { 1 } else if cli.diff && changed > 0 { 1 } else { 0 }
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
