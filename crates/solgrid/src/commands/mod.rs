pub mod check;
pub mod explain;
pub mod fix;
pub mod fmt;
pub mod list_rules;
pub mod migrate;

use ignore::WalkBuilder;
use solgrid_linter::LintEngine;
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

pub(super) fn engine_for_lint_path(path: &Path) -> LintEngine {
    let workspace_root = path
        .parent()
        .and_then(solgrid_config::find_workspace_root)
        .or_else(|| {
            solgrid_config::find_workspace_root(&std::env::current_dir().unwrap_or_default())
        });
    let remappings = workspace_root
        .map(|root| solgrid_config::load_remappings(&root))
        .unwrap_or_default();
    LintEngine::with_remappings(remappings)
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

#[cfg(test)]
mod tests {
    use super::engine_for_lint_path;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn engine_for_lint_path_prefers_file_workspace_remappings() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let workspace = std::env::temp_dir().join(format!("solgrid-command-test-{unique}"));
        let src = workspace.join("src");
        fs::create_dir_all(&src).expect("create src");
        fs::write(workspace.join("remappings.txt"), "@src/=src/\n").expect("write remappings");

        let engine = engine_for_lint_path(&src.join("Token.sol"));
        let result = engine.lint_source(
            "pragma solidity ^0.8.0;\nimport \"../src/Helper.sol\";\n",
            &src.join("Token.sol"),
            &solgrid_config::Config::default(),
        );

        assert!(result
            .diagnostics
            .iter()
            .any(|diag| diag.rule_id == "style/prefer-remappings"
                && diag.message.contains("@src/Helper.sol")));

        fs::remove_dir_all(&workspace).expect("cleanup");
    }
}
