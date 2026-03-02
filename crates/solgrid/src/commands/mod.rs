pub mod check;
pub mod explain;
pub mod fix;
pub mod fmt;
pub mod list_rules;

use ignore::WalkBuilder;
use std::path::{Path, PathBuf};

/// Discover .sol files from the given paths.
pub fn discover_sol_files(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut files = Vec::new();

    if paths.is_empty() {
        // Default: current directory
        collect_sol_files(Path::new("."), &mut files);
    } else {
        for path in paths {
            if path.is_file() {
                if path.extension().is_some_and(|ext| ext == "sol") {
                    files.push(path.clone());
                }
            } else if path.is_dir() {
                collect_sol_files(path, &mut files);
            }
        }
    }

    files.sort();
    files
}

fn collect_sol_files(dir: &Path, files: &mut Vec<PathBuf>) {
    let walker = WalkBuilder::new(dir).hidden(true).git_ignore(true).build();

    for entry in walker.flatten() {
        let path = entry.path();
        if path.is_file() && path.extension().is_some_and(|ext| ext == "sol") {
            files.push(path.to_path_buf());
        }
    }
}
