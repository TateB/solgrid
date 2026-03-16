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
        // 1. Relative paths (start with `.` or `..`)
        if import_path.starts_with('.') {
            let dir = importing_file.parent()?;
            let resolved = dir.join(import_path);
            return resolved.canonicalize().ok().filter(|p| p.exists());
        }

        // 2. Remappings (longest prefix match)
        if let Some(resolved) = self.resolve_remapping(import_path) {
            return Some(resolved);
        }

        let ws_root = self.workspace_root.as_deref()?;

        // 3. Foundry lib (e.g., `forge-std/src/Test.sol` → `lib/forge-std/src/Test.sol`)
        if let Some(resolved) = resolve_in_dir(&ws_root.join("lib"), import_path) {
            return Some(resolved);
        }

        // 4. node_modules (walk up from workspace root)
        resolve_node_modules(import_path, ws_root)
    }

    fn resolve_remapping(&self, import_path: &str) -> Option<PathBuf> {
        // Find the longest matching prefix.
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

/// Try to resolve an import path under a given directory.
///
/// E.g., `resolve_in_dir("lib", "forge-std/src/Test.sol")` checks `lib/forge-std/src/Test.sol`.
fn resolve_in_dir(dir: &Path, import_path: &str) -> Option<PathBuf> {
    let candidate = dir.join(import_path);
    if candidate.exists() {
        Some(candidate)
    } else {
        None
    }
}

/// Walk up from `start` looking for `node_modules/{import_path}`.
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

/// Load remappings from `remappings.txt` or `foundry.toml` at the workspace root.
///
/// Format: `prefix=target` per line. Optional `context:` prefix is ignored.
fn load_remappings(workspace_root: &Path) -> Vec<(String, PathBuf)> {
    // Try remappings.txt first.
    let remappings_file = workspace_root.join("remappings.txt");
    if let Ok(content) = std::fs::read_to_string(&remappings_file) {
        return parse_remappings(&content, workspace_root);
    }

    // Try foundry.toml [profile.default.remappings].
    let foundry_file = workspace_root.join("foundry.toml");
    if let Ok(content) = std::fs::read_to_string(&foundry_file) {
        if let Ok(table) = content.parse::<toml::Table>() {
            if let Some(remappings) = table
                .get("profile")
                .and_then(|p| p.get("default"))
                .and_then(|d| d.get("remappings"))
                .and_then(|r| r.as_array())
            {
                let text: String = remappings
                    .iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join("\n");
                return parse_remappings(&text, workspace_root);
            }
        }
    }

    Vec::new()
}

/// Parse remapping lines of the form `[context:]prefix=target`.
fn parse_remappings(content: &str, workspace_root: &Path) -> Vec<(String, PathBuf)> {
    let mut result = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Strip optional `context:` prefix.
        let mapping = if let Some(colon_pos) = line.find(':') {
            // Only treat as context prefix if `=` comes after the colon.
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_parse_remappings_basic() {
        let content = "@openzeppelin/=lib/openzeppelin-contracts/\nforge-std/=lib/forge-std/src/\n";
        let result = parse_remappings(content, Path::new("/project"));

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].0, "@openzeppelin/");
        assert_eq!(
            result[0].1,
            PathBuf::from("/project/lib/openzeppelin-contracts/")
        );
        assert_eq!(result[1].0, "forge-std/");
        assert_eq!(result[1].1, PathBuf::from("/project/lib/forge-std/src/"));
    }

    #[test]
    fn test_parse_remappings_with_context() {
        let content = "ds-test:ds-test/=lib/ds-test/src/\n";
        let result = parse_remappings(content, Path::new("/project"));

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, "ds-test/");
    }

    #[test]
    fn test_parse_remappings_empty_and_comments() {
        let content = "# comment\n\n  \n@oz/=lib/oz/\n";
        let result = parse_remappings(content, Path::new("/project"));

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, "@oz/");
    }

    #[test]
    fn test_resolve_relative_path() {
        let dir = tempfile::tempdir().unwrap();
        let src_dir = dir.path().join("src");
        fs::create_dir_all(&src_dir).unwrap();
        let token_file = src_dir.join("Token.sol");
        fs::write(&token_file, "contract Token {}").unwrap();
        let main_file = src_dir.join("Main.sol");
        fs::write(&main_file, "").unwrap();

        let resolver = ImportResolver::new(Some(dir.path().to_path_buf()));
        let resolved = resolver.resolve("./Token.sol", &main_file).unwrap();
        assert_eq!(resolved, token_file.canonicalize().unwrap());
    }

    #[test]
    fn test_resolve_node_modules() {
        let dir = tempfile::tempdir().unwrap();
        let nm_path =
            dir.path()
                .join("node_modules/@openzeppelin/contracts/token/ERC20/ERC20.sol");
        fs::create_dir_all(nm_path.parent().unwrap()).unwrap();
        fs::write(&nm_path, "contract ERC20 {}").unwrap();

        let importing = dir.path().join("src/Main.sol");
        let resolver = ImportResolver::new(Some(dir.path().to_path_buf()));
        let resolved = resolver
            .resolve(
                "@openzeppelin/contracts/token/ERC20/ERC20.sol",
                &importing,
            )
            .unwrap();
        assert_eq!(resolved, nm_path);
    }

    #[test]
    fn test_resolve_foundry_lib() {
        let dir = tempfile::tempdir().unwrap();
        let lib_path = dir.path().join("lib/forge-std/src/Test.sol");
        fs::create_dir_all(lib_path.parent().unwrap()).unwrap();
        fs::write(&lib_path, "contract Test {}").unwrap();

        let importing = dir.path().join("src/Main.sol");
        let resolver = ImportResolver::new(Some(dir.path().to_path_buf()));
        let resolved = resolver
            .resolve("forge-std/src/Test.sol", &importing)
            .unwrap();
        assert_eq!(resolved, lib_path);
    }

    #[test]
    fn test_resolve_remappings() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir
            .path()
            .join("lib/openzeppelin-contracts/contracts/token/ERC20/ERC20.sol");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, "contract ERC20 {}").unwrap();

        // Write remappings.txt
        fs::write(
            dir.path().join("remappings.txt"),
            "@openzeppelin/contracts/=lib/openzeppelin-contracts/contracts/\n",
        )
        .unwrap();

        let importing = dir.path().join("src/Main.sol");
        let resolver = ImportResolver::new(Some(dir.path().to_path_buf()));
        let resolved = resolver
            .resolve(
                "@openzeppelin/contracts/token/ERC20/ERC20.sol",
                &importing,
            )
            .unwrap();
        assert_eq!(resolved, target);
    }

    #[test]
    fn test_resolve_nonexistent_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let importing = dir.path().join("src/Main.sol");
        let resolver = ImportResolver::new(Some(dir.path().to_path_buf()));
        assert!(resolver.resolve("./NonExistent.sol", &importing).is_none());
        assert!(resolver
            .resolve("nonexistent-pkg/Foo.sol", &importing)
            .is_none());
    }
}
