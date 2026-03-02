use crate::output;
use crate::Cli;
use solgrid_config::{resolve_config, Config};
use solgrid_diagnostics::Severity;
use solgrid_linter::LintEngine;
use std::path::PathBuf;

pub fn run(paths: &[PathBuf], cli: &Cli) -> i32 {
    let config = load_config(cli);

    // Handle --stdin mode
    if cli.stdin {
        return run_stdin(&config, cli);
    }

    let engine = LintEngine::new();
    let files = super::discover_sol_files(paths);

    if files.is_empty() {
        eprintln!("No .sol files found");
        return 0;
    }

    let mut total_fixed = 0usize;
    let mut total_remaining = 0usize;
    let mut has_errors = false;
    let mut all_results = Vec::new();

    for path in &files {
        let source = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Error reading {}: {e}", path.display());
                continue;
            }
        };

        let (fixed_source, remaining) = engine.fix_source(&source, path, &config, cli.unsafe_fixes);

        if fixed_source != source {
            total_fixed += 1;
            if cli.diff {
                // Show diff
                eprintln!("--- {}", path.display());
                eprintln!("+++ {}", path.display());
                for line in diff_lines(&source, &fixed_source) {
                    eprintln!("{line}");
                }
            } else if let Err(e) = std::fs::write(path, &fixed_source) {
                eprintln!("Error writing {}: {e}", path.display());
            }
        }

        if remaining
            .diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
        {
            has_errors = true;
        }
        total_remaining += remaining.diagnostics.len();
        all_results.push(remaining);
    }

    // Show remaining diagnostics
    if total_remaining > 0 {
        output::print_results(&all_results, &cli.output_format);
    }

    eprintln!("Fixed {total_fixed} file(s), {total_remaining} remaining issue(s)");

    if has_errors {
        1
    } else {
        0
    }
}

fn run_stdin(config: &Config, cli: &Cli) -> i32 {
    use std::io::Read;
    use std::path::Path;

    let mut source = String::new();
    if let Err(e) = std::io::stdin().read_to_string(&mut source) {
        eprintln!("Error reading stdin: {e}");
        return 1;
    }

    let engine = LintEngine::new();
    let (fixed_source, remaining) =
        engine.fix_source(&source, Path::new("<stdin>"), config, cli.unsafe_fixes);

    // Write fixed source to stdout
    print!("{fixed_source}");

    let has_errors = remaining
        .diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error);

    if !remaining.diagnostics.is_empty() {
        output::print_results(&[remaining], &cli.output_format);
    }

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

/// Simple line-level diff.
fn diff_lines(old: &str, new: &str) -> Vec<String> {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();
    let mut output = Vec::new();

    let max = old_lines.len().max(new_lines.len());
    for i in 0..max {
        let old_line = old_lines.get(i).copied().unwrap_or("");
        let new_line = new_lines.get(i).copied().unwrap_or("");
        if old_line != new_line {
            if i < old_lines.len() {
                output.push(format!("-{old_line}"));
            }
            if i < new_lines.len() {
                output.push(format!("+{new_line}"));
            }
        }
    }

    output
}
