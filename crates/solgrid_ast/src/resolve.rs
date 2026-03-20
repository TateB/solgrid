//! Import path resolution — resolves Solidity import paths to filesystem paths.

use std::path::{Path, PathBuf};

/// Resolves Solidity import paths to filesystem paths.
pub struct ImportResolver {
    workspace_root: Option<PathBuf>,
    remappings: Vec<(String, PathBuf)>,
}

impl ImportResolver {
    /// Create a new resolver for the given workspace root.
    ///
    /// Loads remappings from `remappings.txt` or `foundry.toml` if present.
    pub fn new(workspace_root: Option<PathBuf>) -> Self {
        let remappings = workspace_root
            .as_ref()
            .map(|root| load_remappings(root))
            .unwrap_or_default();
        Self {
            workspace_root,
            remappings,
        }
    }

    /// Resolve an import path to a filesystem path.
    ///
    /// Tries in order: relative paths, remappings, foundry lib, node_modules.
    pub fn resolve(&self, import_path: &str, importing_file: &Path) -> Option<PathBuf> {
        if import_path.starts_with('.') {
            let dir = importing_file.parent()?;
            let resolved = dir.join(import_path);
            return resolved.canonicalize().ok().filter(|path| path.exists());
        }

        if let Some(resolved) = self.resolve_remapping(import_path) {
            return Some(resolved);
        }

        let workspace_root = self.workspace_root.as_deref()?;

        if let Some(resolved) = resolve_in_dir(&workspace_root.join("lib"), import_path) {
            return Some(resolved);
        }

        resolve_node_modules(import_path, workspace_root)
    }

    fn resolve_remapping(&self, import_path: &str) -> Option<PathBuf> {
        let mut best: Option<&(String, PathBuf)> = None;
        for entry in &self.remappings {
            if import_path.starts_with(&entry.0) {
                match best {
                    None => best = Some(entry),
                    Some(prev) if entry.0.len() > prev.0.len() => best = Some(entry),
                    _ => {}
                }
            }
        }

        let (prefix, target) = best?;
        let rest = &import_path[prefix.len()..];
        let resolved = target.join(rest);
        if resolved.exists() {
            Some(resolved)
        } else {
            None
        }
    }
}

fn resolve_in_dir(dir: &Path, import_path: &str) -> Option<PathBuf> {
    let candidate = dir.join(import_path);
    if candidate.exists() {
        Some(candidate)
    } else {
        None
    }
}

fn resolve_node_modules(import_path: &str, start: &Path) -> Option<PathBuf> {
    let mut current = start.to_path_buf();
    loop {
        let candidate = current.join("node_modules").join(import_path);
        if candidate.exists() {
            return Some(candidate);
        }
        if !current.pop() {
            break;
        }
    }
    None
}

fn load_remappings(workspace_root: &Path) -> Vec<(String, PathBuf)> {
    let remappings_file = workspace_root.join("remappings.txt");
    if let Ok(content) = std::fs::read_to_string(&remappings_file) {
        return parse_remappings(&content, workspace_root);
    }

    let foundry_file = workspace_root.join("foundry.toml");
    if let Ok(content) = std::fs::read_to_string(&foundry_file) {
        if let Ok(table) = content.parse::<toml::Table>() {
            if let Some(remappings) = table
                .get("profile")
                .and_then(|profile| profile.get("default"))
                .and_then(|default| default.get("remappings"))
                .and_then(|remappings| remappings.as_array())
            {
                let text = remappings
                    .iter()
                    .filter_map(|value| value.as_str())
                    .collect::<Vec<_>>()
                    .join("\n");
                return parse_remappings(&text, workspace_root);
            }
        }
    }

    Vec::new()
}

fn parse_remappings(content: &str, workspace_root: &Path) -> Vec<(String, PathBuf)> {
    let mut result = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let mapping = if let Some(colon_pos) = line.find(':') {
            if line[colon_pos..].contains('=') {
                &line[colon_pos + 1..]
            } else {
                line
            }
        } else {
            line
        };

        if let Some(eq_pos) = mapping.find('=') {
            let prefix = mapping[..eq_pos].to_string();
            let target = &mapping[eq_pos + 1..];
            let target_path = if Path::new(target).is_absolute() {
                PathBuf::from(target)
            } else {
                workspace_root.join(target)
            };
            result.push((prefix, target_path));
        }
    }
    result
}
