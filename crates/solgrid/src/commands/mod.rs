pub mod check;
pub mod explain;
pub mod fix;
pub mod fmt;
pub mod list_rules;
pub mod migrate;

use crate::cache::{config_hash, Cache};
use glob::Pattern;
use ignore::WalkBuilder;
use rayon::ThreadPoolBuilder;
use solgrid_config::{Config, ConfigResolver};
use solgrid_diagnostics::{Diagnostic, FileResult, Severity};
use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone)]
pub struct PreparedFile {
    pub path: PathBuf,
    pub path_display: String,
    pub config: Arc<Config>,
    pub remappings: Arc<Vec<(String, PathBuf)>>,
    pub config_hash: String,
    pub cache_dir: String,
}

pub struct PreparedFiles {
    pub files: Vec<PreparedFile>,
    pub thread_count: usize,
}

#[derive(Clone)]
pub struct CacheUpdate {
    pub cache_dir: String,
    pub path: String,
    pub content_hash: String,
    pub config_hash: String,
    pub diagnostic_count: usize,
    pub is_formatted: bool,
}

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

pub fn load_workspace_remappings(start_path: &Path) -> Vec<(String, PathBuf)> {
    workspace_root_for_path(start_path)
        .as_ref()
        .map(|root| solgrid_config::load_remappings(root))
        .unwrap_or_default()
}

pub fn prepare_files(paths: &[PathBuf], explicit_config: Option<Config>) -> PreparedFiles {
    let mut discovery_resolver = ConfigResolver::new(explicit_config.clone());
    let files = discover_sol_files(paths, &mut discovery_resolver);
    let thread_probe = thread_probe_path(paths);
    let thread_count = discovery_resolver
        .resolve_for_path(&thread_probe)
        .global
        .threads;

    let mut resolver = ConfigResolver::new(explicit_config);
    let mut remappings_cache: HashMap<Option<PathBuf>, Arc<Vec<(String, PathBuf)>>> =
        HashMap::new();
    let files = files
        .into_iter()
        .map(|path| {
            let config = resolver.resolve_for_path(&path);
            let workspace_root = workspace_root_for_path(&path);
            let remappings = remappings_cache
                .entry(workspace_root.clone())
                .or_insert_with(|| {
                    Arc::new(
                        workspace_root
                            .as_ref()
                            .map(|root| solgrid_config::load_remappings(root))
                            .unwrap_or_default(),
                    )
                })
                .clone();
            let config_hash = config_hash(&config);
            let cache_dir = config.global.cache_dir.clone();
            let path_display = path.display().to_string();

            PreparedFile {
                path,
                path_display,
                config,
                remappings,
                config_hash,
                cache_dir,
            }
        })
        .collect();

    PreparedFiles {
        files,
        thread_count,
    }
}

fn workspace_root_for_path(path: &Path) -> Option<PathBuf> {
    let search_root = if path.is_dir() {
        path
    } else {
        path.parent().unwrap_or(path)
    };

    solgrid_config::find_workspace_root(search_root).or_else(|| {
        solgrid_config::find_workspace_root(&std::env::current_dir().unwrap_or_default())
    })
}

pub fn preload_caches(prepared_files: &[PreparedFile]) -> HashMap<String, Cache> {
    let cache_dirs: HashSet<String> = prepared_files
        .iter()
        .map(|file| file.cache_dir.clone())
        .collect();

    cache_dirs
        .into_iter()
        .map(|cache_dir| {
            let cache = Cache::load(Path::new(&cache_dir));
            (cache_dir, cache)
        })
        .collect()
}

pub fn apply_cache_updates(
    caches: &mut HashMap<String, Cache>,
    updates: impl IntoIterator<Item = CacheUpdate>,
) {
    for update in updates {
        let Some(cache) = caches.get_mut(&update.cache_dir) else {
            continue;
        };

        cache.update_hashed(
            &update.path,
            update.content_hash,
            update.config_hash,
            update.diagnostic_count,
            update.is_formatted,
        );
    }
}

pub fn save_caches(caches: &HashMap<String, Cache>) {
    for cache in caches.values() {
        if let Err(error) = cache.save() {
            eprintln!("warning: failed to save cache: {error}");
        }
    }
}

pub fn file_read_error_result(path: &Path, error: &io::Error) -> FileResult {
    FileResult {
        path: path.display().to_string(),
        diagnostics: vec![Diagnostic::new(
            "internal",
            format!("failed to read file: {error}"),
            Severity::Error,
            0..0,
        )],
    }
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

    /// Finding #5: empty `include = []` blocks all files because
    /// `matches_patterns` returns false for empty patterns, and the negation
    /// in `collect_sol_files` causes every file to be skipped.
    #[test]
    fn test_empty_include_discovers_no_files() {
        let root =
            std::env::temp_dir().join(format!("solgrid_discovery_{}_{}", std::process::id(), 3));
        let src = root.join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(root.join("solgrid.toml"), "[global]\ninclude = []\n").unwrap();
        fs::write(src.join("Token.sol"), "contract Token {}").unwrap();

        let mut resolver = ConfigResolver::new(None);
        let files = discover_sol_files(std::slice::from_ref(&root), &mut resolver);
        // With include = [], no files should be discovered.
        assert!(
            files.is_empty(),
            "empty include list should discover zero files, got: {files:?}"
        );

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

    #[test]
    fn test_prepare_files_uses_nearest_config_and_stable_hash() {
        let root = std::env::temp_dir().join(format!(
            "solgrid_prepare_files_{}_{}",
            std::process::id(),
            1
        ));
        let nested = root.join("src");
        fs::create_dir_all(&nested).unwrap();
        fs::write(
            root.join("solgrid.toml"),
            "[lint.rules]\n\"gas/custom-errors\" = \"warn\"\n",
        )
        .unwrap();
        fs::write(nested.join("Token.sol"), "contract Token {}").unwrap();

        let prepared = prepare_files(std::slice::from_ref(&root), None);
        assert_eq!(prepared.files.len(), 1);
        assert_eq!(prepared.files[0].path, nested.join("Token.sol"));
        assert!(!prepared.files[0].config_hash.is_empty());

        let prepared_again = prepare_files(std::slice::from_ref(&root), None);
        assert_eq!(
            prepared.files[0].config_hash,
            prepared_again.files[0].config_hash
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn test_prepare_files_loads_remappings_from_file_workspace() {
        let root = std::env::temp_dir().join(format!(
            "solgrid_prepare_remappings_{}_{}",
            std::process::id(),
            1
        ));
        let contracts = root.join("src/contracts");
        fs::create_dir_all(&contracts).unwrap();
        fs::write(root.join("remappings.txt"), "@src/=src/\n").unwrap();
        let file = contracts.join("Token.sol");
        fs::write(&file, "contract Token {}").unwrap();

        let prepared = prepare_files(std::slice::from_ref(&file), None);
        assert_eq!(prepared.files.len(), 1);
        let remappings = prepared.files[0].remappings.as_ref();
        assert_eq!(remappings.len(), 1);
        assert_eq!(remappings[0].0, "@src/");
        assert_eq!(remappings[0].1, root.join("src/"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn test_file_read_error_result_is_internal_error() {
        let error = io::Error::new(io::ErrorKind::PermissionDenied, "denied");
        let result = file_read_error_result(Path::new("Denied.sol"), &error);
        assert_eq!(result.path, "Denied.sol");
        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(result.diagnostics[0].rule_id, "internal");
        assert_eq!(result.diagnostics[0].severity, Severity::Error);
    }
}
