//! Workspace-wide symbol index for auto-import completions.
//!
//! Scans all `.sol` files in the workspace, parses them for file-level symbol
//! definitions, and maintains an index for quick lookup during completion.

use crate::symbols::{self, SymbolKind};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A symbol exported from a file (defined at file level).
#[derive(Debug, Clone)]
pub struct ExportedSymbol {
    pub name: String,
    pub kind: SymbolKind,
    pub file_path: PathBuf,
}

/// Workspace-wide index of exported symbols for auto-import suggestions.
#[derive(Debug, Default)]
pub struct WorkspaceIndex {
    /// Map from symbol name to all files that export it.
    symbols: HashMap<String, Vec<ExportedSymbol>>,
    /// Map from file path to its exported symbol names (for invalidation).
    file_symbols: HashMap<PathBuf, Vec<String>>,
}

/// Symbol kinds that are exportable from a file.
fn is_exportable(kind: SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Contract
            | SymbolKind::Interface
            | SymbolKind::Library
            | SymbolKind::Function
            | SymbolKind::Error
            | SymbolKind::Event
            | SymbolKind::Struct
            | SymbolKind::Enum
            | SymbolKind::Udvt
    )
}

impl WorkspaceIndex {
    /// Create an empty workspace index.
    pub fn new() -> Self {
        Self::default()
    }

    /// Build the index by scanning all `.sol` files under `root`.
    ///
    /// Respects `.gitignore` and skips common non-source directories.
    pub fn build(root: &Path) -> Self {
        let mut index = Self::new();

        let walker = ignore::WalkBuilder::new(root)
            .hidden(true)
            .filter_entry(|entry| {
                let name = entry.file_name().to_string_lossy();
                // Skip common non-source directories.
                if entry.file_type().is_some_and(|ft| ft.is_dir()) {
                    return !matches!(
                        name.as_ref(),
                        "node_modules" | "out" | "artifacts" | "cache" | "typechain-types"
                    );
                }
                true
            })
            .build();

        for entry in walker.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("sol") {
                continue;
            }
            if let Ok(source) = std::fs::read_to_string(path) {
                index.index_file(path, &source);
            }
        }

        index
    }

    /// Re-index a single file (removes old entries first).
    pub fn update_file(&mut self, path: &Path, source: &str) {
        self.remove_file(path);
        self.index_file(path, source);
    }

    /// Remove all entries for a file.
    pub fn remove_file(&mut self, path: &Path) {
        if let Some(old_names) = self.file_symbols.remove(path) {
            for name in &old_names {
                if let Some(entries) = self.symbols.get_mut(name) {
                    entries.retain(|e| e.file_path != path);
                    if entries.is_empty() {
                        self.symbols.remove(name);
                    }
                }
            }
        }
    }

    /// Look up all exported symbols with the given exact name.
    pub fn find_symbol(&self, name: &str) -> &[ExportedSymbol] {
        self.symbols.get(name).map_or(&[], |v| v.as_slice())
    }

    /// Return all exported symbols whose name starts with `prefix`.
    pub fn symbols_matching(&self, prefix: &str) -> Vec<&ExportedSymbol> {
        if prefix.is_empty() {
            return self.symbols.values().flatten().collect();
        }
        self.symbols
            .iter()
            .filter(|(name, _)| name.starts_with(prefix))
            .flat_map(|(_, entries)| entries)
            .collect()
    }

    /// Index a single file's exported symbols.
    fn index_file(&mut self, path: &Path, source: &str) {
        let filename = path.to_string_lossy().to_string();
        let table = match symbols::build_symbol_table(source, &filename) {
            Some(t) => t,
            None => return,
        };

        let mut names = Vec::new();
        for sym in table.file_level_symbols() {
            if is_exportable(sym.kind) {
                names.push(sym.name.clone());
                self.symbols
                    .entry(sym.name.clone())
                    .or_default()
                    .push(ExportedSymbol {
                        name: sym.name.clone(),
                        kind: sym.kind,
                        file_path: path.to_path_buf(),
                    });
            }
        }

        if !names.is_empty() {
            self.file_symbols.insert(path.to_path_buf(), names);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_build_from_directory() {
        let dir = tempfile::tempdir().unwrap();

        fs::write(
            dir.path().join("Token.sol"),
            r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Token {
    uint256 public supply;
}

interface IERC20 {
    function totalSupply() external view returns (uint256);
}
"#,
        )
        .unwrap();

        fs::write(
            dir.path().join("Errors.sol"),
            r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

error Unauthorized();
error InsufficientBalance(uint256 available, uint256 required);
"#,
        )
        .unwrap();

        let index = WorkspaceIndex::build(dir.path());

        assert_eq!(index.find_symbol("Token").len(), 1);
        assert_eq!(index.find_symbol("Token")[0].kind, SymbolKind::Contract);

        assert_eq!(index.find_symbol("IERC20").len(), 1);
        assert_eq!(index.find_symbol("IERC20")[0].kind, SymbolKind::Interface);

        assert_eq!(index.find_symbol("Unauthorized").len(), 1);
        assert_eq!(index.find_symbol("Unauthorized")[0].kind, SymbolKind::Error);

        assert_eq!(index.find_symbol("InsufficientBalance").len(), 1);

        // State variables should NOT be indexed.
        assert!(index.find_symbol("supply").is_empty());
    }

    #[test]
    fn test_update_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Test.sol");

        let source_v1 = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract OldName {}
"#;
        fs::write(&path, source_v1).unwrap();

        let mut index = WorkspaceIndex::build(dir.path());
        assert_eq!(index.find_symbol("OldName").len(), 1);

        let source_v2 = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract NewName {}
"#;
        index.update_file(&path, source_v2);

        assert!(index.find_symbol("OldName").is_empty());
        assert_eq!(index.find_symbol("NewName").len(), 1);
    }

    #[test]
    fn test_symbols_matching() {
        let dir = tempfile::tempdir().unwrap();

        fs::write(
            dir.path().join("Test.sol"),
            r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract TestToken {}
contract TestVault {}
contract Other {}
"#,
        )
        .unwrap();

        let index = WorkspaceIndex::build(dir.path());

        let matches = index.symbols_matching("Test");
        assert_eq!(matches.len(), 2);

        let matches = index.symbols_matching("Other");
        assert_eq!(matches.len(), 1);

        let matches = index.symbols_matching("NonExistent");
        assert!(matches.is_empty());
    }

    #[test]
    fn test_skips_node_modules() {
        let dir = tempfile::tempdir().unwrap();

        let nm = dir.path().join("node_modules").join("lib");
        fs::create_dir_all(&nm).unwrap();
        fs::write(
            nm.join("Dep.sol"),
            r#"pragma solidity ^0.8.0;
contract Dep {}
"#,
        )
        .unwrap();

        fs::write(
            dir.path().join("Main.sol"),
            r#"pragma solidity ^0.8.0;
contract Main {}
"#,
        )
        .unwrap();

        let index = WorkspaceIndex::build(dir.path());

        assert_eq!(index.find_symbol("Main").len(), 1);
        assert!(index.find_symbol("Dep").is_empty());
    }
}
