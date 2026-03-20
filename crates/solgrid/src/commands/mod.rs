pub mod check;
pub mod explain;
pub mod fix;
pub mod fmt;
pub mod list_rules;
pub mod migrate;

use glob::Pattern;
use ignore::WalkBuilder;
use rayon::ThreadPoolBuilder;
use solgrid_config::{Config, ConfigResolver};
use std::path::{Path, PathBuf};

/// Discover .sol files from the given paths.
pub fn discover_sol_files(paths: &[PathBuf], resolver: &mut ConfigResolver) -> Vec<PathBuf> {
    let mut files = Vec::new();

    if paths.is_empty() {
        collect_sol_files(Path::new("."), resolver, &mut files, true);
    } else {
        for path in paths {
            if path.is_file() {
                if path.extension().is_some_and(|ext| ext == "sol") {
                    files.push(path.clone());
                }
            } else if path.is_dir() {
                collect_sol_files(path, resolver, &mut files, true);
            }
        }
    }

    files.sort();
    files.dedup();
    files
}

pub fn thread_probe_path(paths: &[PathBuf]) -> PathBuf {
    paths.first().cloned().unwrap_or_else(|| PathBuf::from("."))
}

pub fn install_with_thread_count<T, F>(threads: usize, f: F) -> T
where
    T: Send,
    F: FnOnce() -> T + Send,
{
    if threads > 0 {
        ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .expect("failed to create rayon thread pool")
            .install(f)
    } else {
        f()
    }
}

fn collect_sol_files(
    dir: &Path,
    resolver: &mut ConfigResolver,
    files: &mut Vec<PathBuf>,
    apply_include: bool,
) {
    let config = resolver.resolve_for_path(dir);
    let include_patterns = compile_patterns(&config.global.include);
    let exclude_patterns = compile_patterns(&config.global.exclude);
    let walker = WalkBuilder::new(dir)
        .hidden(true)
        .git_ignore(config.global.respect_gitignore)
        .build();

    for entry in walker.flatten() {
        let path = entry.path();
        if !path.is_file() || path.extension().is_none_or(|ext| ext != "sol") {
            continue;
        }

        let relative = path.strip_prefix(dir).unwrap_or(path);
        if matches_patterns(relative, &exclude_patterns) {
            continue;
        }
        if apply_include && !matches_patterns(relative, &include_patterns) {
            continue;
        }

        files.push(path.to_path_buf());
    }
}

fn compile_patterns(patterns: &[String]) -> Vec<Pattern> {
    patterns
        .iter()
        .filter_map(|pattern| Pattern::new(pattern).ok())
        .collect()
}

fn matches_patterns(path: &Path, patterns: &[Pattern]) -> bool {
    if patterns.is_empty() {
        return false;
    }

    let slash_path = path.to_string_lossy().replace('\\', "/");
    patterns.iter().any(|pattern| pattern.matches(&slash_path))
}

pub fn load_explicit_config(cli: &crate::Cli) -> Option<Config> {
    cli.config.as_ref().map(|config_path| {
        solgrid_config::load_config(config_path).unwrap_or_else(|error| {
            eprintln!("Error loading config: {error}");
            std::process::exit(2);
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_discovery_respects_include_and_exclude() {
        let root =
            std::env::temp_dir().join(format!("solgrid_discovery_{}_{}", std::process::id(), 1));
        let src = root.join("src");
        let test = root.join("test");
        fs::create_dir_all(&src).unwrap();
        fs::create_dir_all(&test).unwrap();
        fs::write(
            root.join("solgrid.toml"),
            "[global]\ninclude = [\"src/**/*.sol\"]\nexclude = [\"src/Ignore.sol\"]\n",
        )
        .unwrap();
        fs::write(src.join("Keep.sol"), "contract Keep {}").unwrap();
        fs::write(src.join("Ignore.sol"), "contract Ignore {}").unwrap();
        fs::write(test.join("Skip.sol"), "contract Skip {}").unwrap();

        let mut resolver = ConfigResolver::new(None);
        let files = discover_sol_files(std::slice::from_ref(&root), &mut resolver);
        let labels: Vec<_> = files
            .iter()
            .filter_map(|path| path.file_name().and_then(|name| name.to_str()))
            .collect();

        assert_eq!(labels, vec!["Keep.sol"]);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn test_explicit_file_bypasses_exclude_rules() {
        let root =
            std::env::temp_dir().join(format!("solgrid_discovery_{}_{}", std::process::id(), 2));
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("solgrid.toml"),
            "[global]\nexclude = [\"src/**/*.sol\"]\n",
        )
        .unwrap();
        let file = root.join("src/Token.sol");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "contract Token {}").unwrap();

        let mut resolver = ConfigResolver::new(None);
        let files = discover_sol_files(std::slice::from_ref(&file), &mut resolver);
        assert_eq!(files, vec![file]);

        let _ = fs::remove_dir_all(root);
    }
}
