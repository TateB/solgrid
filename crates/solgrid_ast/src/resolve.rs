//! Import path resolution — resolves Solidity import paths to filesystem paths.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

/// Resolves Solidity import paths to filesystem paths.
pub struct ImportResolver {
    workspace_root: Option<PathBuf>,
    remappings_cache: RwLock<HashMap<PathBuf, Vec<(String, PathBuf)>>>,
}

impl ImportResolver {
    /// Create a new resolver for the given workspace root.
    ///
    /// Loads remappings from `remappings.txt` or `foundry.toml` if present.
    pub fn new(workspace_root: Option<PathBuf>) -> Self {
        Self {
            workspace_root,
            remappings_cache: RwLock::new(HashMap::new()),
        }
    }

    /// Load remappings for the importing file's nearest workspace.
    pub fn remappings_for_file(&self, importing_file: &Path) -> Vec<(String, PathBuf)> {
        let Some(workspace_root) = self.workspace_root_for_file(importing_file) else {
            return Vec::new();
        };

        self.cached_remappings(&workspace_root)
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

        let workspace_root = self.workspace_root_for_file(importing_file)?;
        let remappings = self.cached_remappings(&workspace_root);

        // 2. Remappings (longest prefix match)
        if let Some(resolved) = resolve_remapping(import_path, &remappings) {
            return Some(resolved);
        }

        // 3. Foundry lib (e.g., `forge-std/src/Test.sol` → `lib/forge-std/src/Test.sol`)
        if let Some(resolved) = resolve_in_dir(&workspace_root.join("lib"), import_path) {
            return Some(resolved);
        }

        // 4. node_modules (walk up from workspace root)
        resolve_node_modules(import_path, &workspace_root)
    }

    fn workspace_root_for_file(&self, importing_file: &Path) -> Option<PathBuf> {
        let search_root = importing_file.parent().unwrap_or(importing_file);
        solgrid_config::find_workspace_root(search_root).or_else(|| {
            self.workspace_root
                .as_ref()
                .filter(|root| search_root.starts_with(root))
                .cloned()
        })
    }

    fn cached_remappings(&self, workspace_root: &Path) -> Vec<(String, PathBuf)> {
        if let Some(remappings) = self
            .remappings_cache
            .read()
            .expect("resolver remappings cache poisoned")
            .get(workspace_root)
            .cloned()
        {
            return remappings;
        }

        let remappings = solgrid_config::load_remappings(workspace_root);
        self.remappings_cache
            .write()
            .expect("resolver remappings cache poisoned")
            .insert(workspace_root.to_path_buf(), remappings.clone());
        remappings
    }
}

fn resolve_remapping(import_path: &str, remappings: &[(String, PathBuf)]) -> Option<PathBuf> {
    // Find the longest matching prefix.
    let mut best: Option<&(String, PathBuf)> = None;
    for entry in remappings {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

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
        let nm_path = dir
            .path()
            .join("node_modules/@openzeppelin/contracts/token/ERC20/ERC20.sol");
        fs::create_dir_all(nm_path.parent().unwrap()).unwrap();
        fs::write(&nm_path, "contract ERC20 {}").unwrap();

        let importing = dir.path().join("src/Main.sol");
        let resolver = ImportResolver::new(Some(dir.path().to_path_buf()));
        let resolved = resolver
            .resolve("@openzeppelin/contracts/token/ERC20/ERC20.sol", &importing)
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
            .resolve("@openzeppelin/contracts/token/ERC20/ERC20.sol", &importing)
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

    #[test]
    fn test_resolve_uses_nearest_workspace_remappings() {
        let dir = tempfile::tempdir().unwrap();
        let root_target = dir
            .path()
            .join("lib/root-contracts/contracts/token/ERC20/ERC20.sol");
        let nested_target = dir
            .path()
            .join("packages/app/lib/app-contracts/contracts/token/ERC20/ERC20.sol");
        fs::create_dir_all(root_target.parent().unwrap()).unwrap();
        fs::create_dir_all(nested_target.parent().unwrap()).unwrap();
        fs::write(&root_target, "contract RootERC20 {}").unwrap();
        fs::write(&nested_target, "contract AppERC20 {}").unwrap();
        fs::write(
            dir.path().join("remappings.txt"),
            "@openzeppelin/contracts/=lib/root-contracts/contracts/\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("packages/app/foundry.toml"),
            "[profile.default]\nremappings = [\"@openzeppelin/contracts/=lib/app-contracts/contracts/\"]\n",
        )
        .unwrap();

        let importing = dir.path().join("packages/app/src/Main.sol");
        fs::create_dir_all(importing.parent().unwrap()).unwrap();
        fs::write(&importing, "").unwrap();

        let resolver = ImportResolver::new(Some(dir.path().to_path_buf()));
        let resolved = resolver
            .resolve("@openzeppelin/contracts/token/ERC20/ERC20.sol", &importing)
            .unwrap();
        assert_eq!(resolved, nested_target);
    }
}
