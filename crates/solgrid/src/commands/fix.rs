use crate::output;
use crate::Cli;
use rayon::prelude::*;
use solgrid_config::{resolve_config, Config};
use solgrid_diagnostics::{FileResult, Severity};
use solgrid_linter::LintEngine;
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

    let outcomes = super::install_with_thread_count(prepared.thread_count, || {
        prepared
            .files
            .par_iter()
            .map(|file| {
                let source = match std::fs::read_to_string(&file.path) {
                    Ok(source) => source,
                    Err(error) => {
                        return FixOutcome {
                            path: file.path.clone(),
                            source: None,
                            fixed_source: None,
                            remaining: None,
                            read_error: Some(format!(
                                "Error reading {}: {error}",
                                file.path.display()
                            )),
                        };
                    }
                };

                let engine = LintEngine::with_remappings((*file.remappings).clone());
                let (fixed_source, remaining) =
                    engine.fix_source(&source, &file.path, &file.config, cli.unsafe_fixes);

                FixOutcome {
                    path: file.path.clone(),
                    source: Some(source),
                    fixed_source: Some(fixed_source),
                    remaining: Some(remaining),
                    read_error: None,
                }
            })
            .collect::<Vec<_>>()
    });

    let mut total_fixed = 0usize;
    let mut total_remaining = 0usize;
    let mut has_errors = false;
    let mut all_results = Vec::new();

    // Compute in parallel, but keep writes and diff output serial for stable output ordering.
    for outcome in outcomes {
        if let Some(error) = outcome.read_error {
            eprintln!("{error}");
            continue;
        }

        let source = outcome
            .source
            .expect("source should exist when read succeeds");
        let fixed_source = outcome
            .fixed_source
            .expect("fixed source should exist when read succeeds");
        let remaining = outcome
            .remaining
            .expect("remaining diagnostics should exist when read succeeds");

        if fixed_source != source {
            total_fixed += 1;
            if cli.diff {
                eprintln!("--- {}", outcome.path.display());
                eprintln!("+++ {}", outcome.path.display());
                for line in diff_lines(&source, &fixed_source) {
                    eprintln!("{line}");
                }
            } else if let Err(error) = std::fs::write(&outcome.path, &fixed_source) {
                eprintln!("Error writing {}: {error}", outcome.path.display());
            }
        }

        if remaining
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error)
        {
            has_errors = true;
        }
        total_remaining += remaining.diagnostics.len();
        all_results.push(remaining);
    }

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

    let current_dir = std::env::current_dir().unwrap_or_default();
    let engine = LintEngine::with_remappings(super::load_workspace_remappings(&current_dir));
    let (fixed_source, remaining) =
        engine.fix_source(&source, Path::new("<stdin>"), config, cli.unsafe_fixes);

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

struct FixOutcome {
    path: PathBuf,
    source: Option<String>,
    fixed_source: Option<String>,
    remaining: Option<FileResult>,
    read_error: Option<String>,
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
