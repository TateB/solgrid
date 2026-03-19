//! Go-to-definition handler for the LSP server.

use crate::convert;
use crate::resolve::ImportResolver;
use crate::symbols::{self, ImportedSymbols, SymbolDef, SymbolTable};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tower_lsp_server::ls_types;

/// A symbol resolved from an imported file.
pub(crate) struct CrossFileSymbol {
    /// Source text of the file where the symbol is defined.
    pub source: String,
    /// Symbol table of the file where the symbol is defined.
    pub table: SymbolTable,
    /// The resolved symbol definition.
    pub def: SymbolDef,
    /// Filesystem path of the file where the symbol is defined.
    pub resolved_path: PathBuf,
}

/// Handle a go-to-definition request.
///
/// Returns the location of the definition for the identifier at `position`,
/// or `None` if no definition is found (unknown symbol, parse error, etc.).
///
/// `get_source` resolves a filesystem path to source text (checking open
/// documents first, then falling back to disk).
pub fn goto_definition(
    source: &str,
    position: &ls_types::Position,
    uri: &ls_types::Uri,
    get_source: &dyn Fn(&Path) -> Option<String>,
    resolver: &ImportResolver,
) -> Option<ls_types::GotoDefinitionResponse> {
    let offset = convert::position_to_offset(source, *position);

    let table = symbols::build_symbol_table(source, "buffer.sol")?;

    // Check if cursor is on an import path string — navigate to that file.
    for import in &table.imports {
        if import.path_span.contains(&offset) {
            let importing_file = uri_to_path(uri)?;
            let resolved = resolver.resolve(&import.path, &importing_file)?;
            let target_uri = path_to_uri(&resolved)?;
            return Some(ls_types::GotoDefinitionResponse::Scalar(
                ls_types::Location {
                    uri: target_uri,
                    range: ls_types::Range::default(),
                },
            ));
        }
    }

    // Try member access: `Container.member`
    if let Some((container, _member, member_range)) =
        symbols::find_member_access_at_offset(source, offset)
    {
        if let Some(container_def) = table.resolve(&container, offset) {
            let member_name = &source[member_range.clone()];
            if let Some(member_def) = table.resolve_member(container_def, member_name) {
                let range = convert::span_to_range(source, &member_def.name_span);
                return Some(ls_types::GotoDefinitionResponse::Scalar(
                    ls_types::Location {
                        uri: uri.clone(),
                        range,
                    },
                ));
            }
        }

        // Cross-file member access: resolve container in imports, then member in that file.
        let importing_file = uri_to_path(uri)?;
        if let Some(result) = resolve_cross_file_member(
            &table,
            &container,
            &source[member_range],
            &importing_file,
            get_source,
            resolver,
        ) {
            return Some(result);
        }

        return None;
    }

    let (name, _ident_range) = symbols::find_ident_at_offset(source, offset)?;

    // Try same-file resolution first.
    if let Some(def) = table.resolve(&name, offset) {
        let range = convert::span_to_range(source, &def.name_span);
        return Some(ls_types::GotoDefinitionResponse::Scalar(
            ls_types::Location {
                uri: uri.clone(),
                range,
            },
        ));
    }

    // Cross-file: check for alias/glob imports first (navigate to the file).
    let importing_file = uri_to_path(uri)?;
    for import in &table.imports {
        match &import.symbols {
            ImportedSymbols::Plain(Some(alias)) if alias == &name => {
                let resolved = resolver.resolve(&import.path, &importing_file)?;
                let target_uri = path_to_uri(&resolved)?;
                return Some(ls_types::GotoDefinitionResponse::Scalar(
                    ls_types::Location {
                        uri: target_uri,
                        range: ls_types::Range::default(),
                    },
                ));
            }
            ImportedSymbols::Glob(alias) if alias == &name => {
                let resolved = resolver.resolve(&import.path, &importing_file)?;
                let target_uri = path_to_uri(&resolved)?;
                return Some(ls_types::GotoDefinitionResponse::Scalar(
                    ls_types::Location {
                        uri: target_uri,
                        range: ls_types::Range::default(),
                    },
                ));
            }
            _ => {}
        }
    }

    // Cross-file: resolve the symbol (with transitive import support).
    if let Some(cross) =
        resolve_cross_file_symbol(&table, &name, &importing_file, get_source, resolver)
    {
        let range = convert::span_to_range(&cross.source, &cross.def.name_span);
        let target_uri = path_to_uri(&cross.resolved_path)?;
        return Some(ls_types::GotoDefinitionResponse::Scalar(
            ls_types::Location {
                uri: target_uri,
                range,
            },
        ));
    }

    None
}

/// Resolve a `Container.member` access across file boundaries.
///
/// Looks through imports to find `container_name`, loads that file's symbol table,
/// then resolves `member_name` inside the container's scope.
fn resolve_cross_file_member(
    table: &symbols::SymbolTable,
    container_name: &str,
    member_name: &str,
    importing_file: &Path,
    get_source: &dyn Fn(&Path) -> Option<String>,
    resolver: &ImportResolver,
) -> Option<ls_types::GotoDefinitionResponse> {
    let cross = resolve_cross_file_member_symbol(
        table,
        container_name,
        member_name,
        importing_file,
        get_source,
        resolver,
    )?;
    let range = convert::span_to_range(&cross.source, &cross.def.name_span);
    let target_uri = path_to_uri(&cross.resolved_path)?;
    Some(ls_types::GotoDefinitionResponse::Scalar(
        ls_types::Location {
            uri: target_uri,
            range,
        },
    ))
}

/// Resolve a symbol name by searching the current file's imports.
///
/// Follows import chains transitively (e.g., FileC imports from FileB which
/// re-exports from FileA) with cycle detection.
pub(crate) fn resolve_cross_file_symbol(
    current_table: &SymbolTable,
    name: &str,
    importing_file: &Path,
    get_source: &dyn Fn(&Path) -> Option<String>,
    resolver: &ImportResolver,
) -> Option<CrossFileSymbol> {
    let mut visited = HashSet::new();
    resolve_cross_file_symbol_inner(
        current_table,
        name,
        importing_file,
        get_source,
        resolver,
        &mut visited,
    )
}

fn resolve_cross_file_symbol_inner(
    current_table: &SymbolTable,
    name: &str,
    importing_file: &Path,
    get_source: &dyn Fn(&Path) -> Option<String>,
    resolver: &ImportResolver,
    visited: &mut HashSet<PathBuf>,
) -> Option<CrossFileSymbol> {
    for import in &current_table.imports {
        let target_name = match &import.symbols {
            ImportedSymbols::Named(names) => {
                let mut found = None;
                for (original, alias) in names {
                    let local_name = alias.as_deref().unwrap_or(original.as_str());
                    if local_name == name {
                        found = Some(original.as_str());
                        break;
                    }
                }
                match found {
                    Some(n) => n,
                    None => continue,
                }
            }
            ImportedSymbols::Plain(alias) => {
                if alias.is_some() {
                    continue;
                }
                // Plain import without alias — all file-level symbols are in scope.
                name
            }
            ImportedSymbols::Glob(_) => continue,
        };

        let resolved = match resolver.resolve(&import.path, importing_file) {
            Some(p) => p,
            None => continue,
        };

        if !visited.insert(resolved.clone()) {
            continue;
        }

        let imported_source = match get_source(&resolved) {
            Some(s) => s,
            None => continue,
        };
        let filename = resolved.to_string_lossy().to_string();
        let imported_table = match symbols::build_symbol_table(&imported_source, &filename) {
            Some(t) => t,
            None => continue,
        };

        // Try direct resolution in this file.
        if let Some(def) = imported_table.resolve(target_name, 0) {
            let def = def.clone();
            return Some(CrossFileSymbol {
                source: imported_source,
                def,
                table: imported_table,
                resolved_path: resolved,
            });
        }

        // Not defined here — follow this file's imports transitively.
        if let Some(result) = resolve_cross_file_symbol_inner(
            &imported_table,
            target_name,
            &resolved,
            get_source,
            resolver,
            visited,
        ) {
            return Some(result);
        }
    }
    None
}

/// Resolve a `Container.member` access across file boundaries.
///
/// Follows import chains transitively with cycle detection.
pub(crate) fn resolve_cross_file_member_symbol(
    current_table: &SymbolTable,
    container_name: &str,
    member_name: &str,
    importing_file: &Path,
    get_source: &dyn Fn(&Path) -> Option<String>,
    resolver: &ImportResolver,
) -> Option<CrossFileSymbol> {
    let mut visited = HashSet::new();
    resolve_cross_file_member_symbol_inner(
        current_table,
        container_name,
        member_name,
        importing_file,
        get_source,
        resolver,
        &mut visited,
    )
}

fn resolve_cross_file_member_symbol_inner(
    current_table: &SymbolTable,
    container_name: &str,
    member_name: &str,
    importing_file: &Path,
    get_source: &dyn Fn(&Path) -> Option<String>,
    resolver: &ImportResolver,
    visited: &mut HashSet<PathBuf>,
) -> Option<CrossFileSymbol> {
    for import in &current_table.imports {
        let target_name = match &import.symbols {
            ImportedSymbols::Named(names) => {
                let mut found = None;
                for (original, alias) in names {
                    let local = alias.as_deref().unwrap_or(original.as_str());
                    if local == container_name {
                        found = Some(original.as_str());
                        break;
                    }
                }
                match found {
                    Some(n) => n,
                    None => continue,
                }
            }
            ImportedSymbols::Plain(None) => container_name,
            _ => continue,
        };

        let resolved = match resolver.resolve(&import.path, importing_file) {
            Some(p) => p,
            None => continue,
        };

        if !visited.insert(resolved.clone()) {
            continue;
        }

        let imported_source = match get_source(&resolved) {
            Some(s) => s,
            None => continue,
        };
        let filename = resolved.to_string_lossy().to_string();
        let imported_table = match symbols::build_symbol_table(&imported_source, &filename) {
            Some(t) => t,
            None => continue,
        };

        // Try direct resolution: find the container, then the member.
        if let Some(container_def) = imported_table.resolve(target_name, 0) {
            if let Some(member_def) = imported_table.resolve_member(container_def, member_name) {
                let def = member_def.clone();
                return Some(CrossFileSymbol {
                    source: imported_source,
                    def,
                    table: imported_table,
                    resolved_path: resolved,
                });
            }
        }

        // Not defined here — follow this file's imports transitively.
        if let Some(result) = resolve_cross_file_member_symbol_inner(
            &imported_table,
            target_name,
            member_name,
            &resolved,
            get_source,
            resolver,
            visited,
        ) {
            return Some(result);
        }
    }
    None
}

pub(crate) fn uri_to_path(uri: &ls_types::Uri) -> Option<std::path::PathBuf> {
    uri.to_file_path().map(|p| p.into_owned())
}

fn path_to_uri(path: &Path) -> Option<ls_types::Uri> {
    ls_types::Uri::from_file_path(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn noop_resolver() -> ImportResolver {
        ImportResolver::new(None)
    }

    fn noop_source(_path: &Path) -> Option<String> {
        None
    }

    #[test]
    fn test_same_file_definition() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    uint256 public value;
    function get() public view returns (uint256) {
        return value;
    }
}
"#;
        let uri: ls_types::Uri = "file:///test.sol".parse().unwrap();
        // Position on "value" in `return value;`
        let offset = source.find("return value").unwrap() + 7;
        let pos = convert::offset_to_position(source, offset);

        let result = goto_definition(source, &pos, &uri, &noop_source, &noop_resolver());
        assert!(result.is_some());
        if let Some(ls_types::GotoDefinitionResponse::Scalar(loc)) = result {
            assert_eq!(loc.uri, uri);
        } else {
            panic!("expected scalar response");
        }
    }

    #[test]
    fn test_cross_file_named_import() {
        let dir = tempfile::tempdir().unwrap();

        // Create the imported file.
        let token_path = dir.path().join("Token.sol");
        let token_source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Token {
    uint256 public supply;
}
"#;
        fs::write(&token_path, token_source).unwrap();

        // Main file with named import.
        let main_source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {Token} from "./Token.sol";

contract Main is Token {}
"#;
        let main_path = dir.path().join("Main.sol");
        fs::write(&main_path, "").unwrap(); // just needs to exist for path resolution

        let uri = ls_types::Uri::from_file_path(&main_path).unwrap();
        let resolver = ImportResolver::new(Some(dir.path().to_path_buf()));

        let get_source = |path: &Path| -> Option<String> { fs::read_to_string(path).ok() };

        // Click on "Token" in `contract Main is Token {}`
        let offset = main_source.find("is Token").unwrap() + 3;
        let pos = convert::offset_to_position(main_source, offset);

        let result = goto_definition(main_source, &pos, &uri, &get_source, &resolver);
        assert!(result.is_some());
        if let Some(ls_types::GotoDefinitionResponse::Scalar(loc)) = result {
            let expected_uri =
                ls_types::Uri::from_file_path(token_path.canonicalize().unwrap()).unwrap();
            assert_eq!(loc.uri, expected_uri);
            // Should point to the "Token" name in the contract definition.
            assert_ne!(loc.range, ls_types::Range::default());
        } else {
            panic!("expected scalar response");
        }
    }

    #[test]
    fn test_cross_file_import_path_click() {
        let dir = tempfile::tempdir().unwrap();

        let token_path = dir.path().join("Token.sol");
        fs::write(&token_path, "contract Token {}").unwrap();

        let main_source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {Token} from "./Token.sol";

contract Main {}
"#;
        let main_path = dir.path().join("Main.sol");
        fs::write(&main_path, "").unwrap();

        let uri = ls_types::Uri::from_file_path(&main_path).unwrap();
        let resolver = ImportResolver::new(Some(dir.path().to_path_buf()));
        let get_source = |path: &Path| -> Option<String> { fs::read_to_string(path).ok() };

        // Click on the import path string "./Token.sol"
        let offset = main_source.find("./Token.sol").unwrap() + 2;
        let pos = convert::offset_to_position(main_source, offset);

        let result = goto_definition(main_source, &pos, &uri, &get_source, &resolver);
        assert!(result.is_some());
        if let Some(ls_types::GotoDefinitionResponse::Scalar(loc)) = result {
            // Should navigate to the imported file at line 0, col 0.
            assert_eq!(loc.range, ls_types::Range::default());
        } else {
            panic!("expected scalar response");
        }
    }

    #[test]
    fn test_same_file_member_access_function() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

library MathLib {
    function add(uint256 a, uint256 b) internal pure returns (uint256) {
        return a + b;
    }
}

contract Test {
    function foo() public pure returns (uint256) {
        return MathLib.add(1, 2);
    }
}
"#;
        let uri: ls_types::Uri = "file:///test.sol".parse().unwrap();
        // Click on "add" in `MathLib.add(1, 2)`
        let offset = source.find("MathLib.add(1").unwrap() + 8;
        let pos = convert::offset_to_position(source, offset);

        let result = goto_definition(source, &pos, &uri, &noop_source, &noop_resolver());
        assert!(result.is_some(), "expected definition for MathLib.add");
        if let Some(ls_types::GotoDefinitionResponse::Scalar(loc)) = result {
            assert_eq!(loc.uri, uri);
            // Should point to "add" in the library function definition.
            let name_offset = source.find("function add(uint256").unwrap() + 9;
            let expected_pos = convert::offset_to_position(source, name_offset);
            assert_eq!(loc.range.start, expected_pos);
        } else {
            panic!("expected scalar response");
        }
    }

    #[test]
    fn test_same_file_member_access_enum_variant() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    enum Status { Active, Paused }
    function getActive() public pure returns (Status) {
        return Status.Active;
    }
}
"#;
        let uri: ls_types::Uri = "file:///test.sol".parse().unwrap();
        let offset = source.find("Status.Active").unwrap() + 7;
        let pos = convert::offset_to_position(source, offset);

        let result = goto_definition(source, &pos, &uri, &noop_source, &noop_resolver());
        assert!(result.is_some(), "expected definition for Status.Active");
        if let Some(ls_types::GotoDefinitionResponse::Scalar(loc)) = result {
            assert_eq!(loc.uri, uri);
        } else {
            panic!("expected scalar response");
        }
    }

    #[test]
    fn test_same_file_member_access_struct_field() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    struct Info { uint256 id; address owner; }
    function getOwner(Info memory info) public pure returns (address) {
        return Info.owner;
    }
}
"#;
        let uri: ls_types::Uri = "file:///test.sol".parse().unwrap();
        let offset = source.find("Info.owner").unwrap() + 5;
        let pos = convert::offset_to_position(source, offset);

        let result = goto_definition(source, &pos, &uri, &noop_source, &noop_resolver());
        assert!(result.is_some(), "expected definition for Info.owner");
        if let Some(ls_types::GotoDefinitionResponse::Scalar(loc)) = result {
            assert_eq!(loc.uri, uri);
        } else {
            panic!("expected scalar response");
        }
    }

    #[test]
    fn test_cross_file_member_access() {
        let dir = tempfile::tempdir().unwrap();

        let token_path = dir.path().join("Token.sol");
        let token_source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

library TokenLib {
    function mint(address to, uint256 amount) internal {}
}
"#;
        fs::write(&token_path, token_source).unwrap();

        let main_source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {TokenLib} from "./Token.sol";

contract Main {
    function doMint() public {
        TokenLib.mint(msg.sender, 100);
    }
}
"#;
        let main_path = dir.path().join("Main.sol");
        fs::write(&main_path, "").unwrap();

        let uri = ls_types::Uri::from_file_path(&main_path).unwrap();
        let resolver = ImportResolver::new(Some(dir.path().to_path_buf()));
        let get_source = |path: &Path| -> Option<String> { fs::read_to_string(path).ok() };

        // Click on "mint" in `TokenLib.mint(msg.sender, 100)`
        let offset = main_source.find("TokenLib.mint(msg").unwrap() + 9;
        let pos = convert::offset_to_position(main_source, offset);

        let result = goto_definition(main_source, &pos, &uri, &get_source, &resolver);
        assert!(
            result.is_some(),
            "expected definition for cross-file TokenLib.mint"
        );
        if let Some(ls_types::GotoDefinitionResponse::Scalar(loc)) = result {
            let expected_uri =
                ls_types::Uri::from_file_path(token_path.canonicalize().unwrap()).unwrap();
            assert_eq!(loc.uri, expected_uri);
            assert_ne!(loc.range, ls_types::Range::default());
        } else {
            panic!("expected scalar response");
        }
    }

    #[test]
    fn test_unresolvable_import_returns_none() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {Missing} from "./NonExistent.sol";

contract Main {
    Missing m;
}
"#;
        let uri: ls_types::Uri = "file:///test/Main.sol".parse().unwrap();

        // Click on "Missing" in `Missing m;`
        let offset = source.find("Missing m").unwrap();
        let pos = convert::offset_to_position(source, offset);

        let result = goto_definition(source, &pos, &uri, &noop_source, &noop_resolver());
        // Should return None since the import can't be resolved.
        assert!(result.is_none());
    }

    #[test]
    fn test_goto_definition_error_usage_in_revert() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    error CustomError(uint256 code);

    function fail() external {
        revert CustomError(1);
    }
}
"#;
        let uri: ls_types::Uri = "file:///test.sol".parse().unwrap();

        // Click on "CustomError" in `revert CustomError(1);`
        let offset = source.find("revert CustomError").unwrap() + 7;
        let pos = convert::offset_to_position(source, offset);

        let result = goto_definition(source, &pos, &uri, &noop_source, &noop_resolver());
        assert!(result.is_some(), "should resolve error in revert statement");
        if let Some(ls_types::GotoDefinitionResponse::Scalar(loc)) = result {
            assert_eq!(loc.uri, uri);
            // Should point to the error declaration name
            let name_offset = source.find("error CustomError").unwrap() + 6;
            let expected_pos = convert::offset_to_position(source, name_offset);
            assert_eq!(loc.range.start, expected_pos);
        } else {
            panic!("expected scalar response");
        }
    }

    #[test]
    fn test_goto_definition_cross_file_error() {
        let dir = tempfile::tempdir().unwrap();

        let errors_path = dir.path().join("Errors.sol");
        let errors_source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

error CustomError(uint256 code);
"#;
        fs::write(&errors_path, errors_source).unwrap();

        let main_source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {CustomError} from "./Errors.sol";

contract Test {
    function fail() external {
        revert CustomError(1);
    }
}
"#;
        let main_path = dir.path().join("Main.sol");
        fs::write(&main_path, "").unwrap();

        let uri = ls_types::Uri::from_file_path(&main_path).unwrap();
        let resolver = ImportResolver::new(Some(dir.path().to_path_buf()));
        let get_source = |path: &Path| -> Option<String> { fs::read_to_string(path).ok() };

        // Click on "CustomError" in `revert CustomError(1);`
        let offset = main_source.find("revert CustomError").unwrap() + 7;
        let pos = convert::offset_to_position(main_source, offset);

        let result = goto_definition(main_source, &pos, &uri, &get_source, &resolver);
        assert!(
            result.is_some(),
            "should resolve cross-file error in revert"
        );
        if let Some(ls_types::GotoDefinitionResponse::Scalar(loc)) = result {
            let expected_uri =
                ls_types::Uri::from_file_path(errors_path.canonicalize().unwrap()).unwrap();
            assert_eq!(loc.uri, expected_uri);
            assert_ne!(loc.range, ls_types::Range::default());
        } else {
            panic!("expected scalar response");
        }
    }

    #[test]
    fn test_transitive_import_goto_definition() {
        // FileA defines ThingOne, FileB re-exports it, FileC imports from FileB.
        let dir = tempfile::tempdir().unwrap();

        let file_a = dir.path().join("FileA.sol");
        fs::write(
            &file_a,
            r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

error ThingOne(uint256 code);
"#,
        )
        .unwrap();

        let file_b = dir.path().join("FileB.sol");
        fs::write(
            &file_b,
            r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {ThingOne} from "./FileA.sol";
"#,
        )
        .unwrap();

        let file_c_source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {ThingOne} from "./FileB.sol";

contract Test {
    function fail() external {
        revert ThingOne(1);
    }
}
"#;
        let file_c = dir.path().join("FileC.sol");
        fs::write(&file_c, "").unwrap();

        let uri = ls_types::Uri::from_file_path(&file_c).unwrap();
        let resolver = ImportResolver::new(Some(dir.path().to_path_buf()));
        let get_source = |path: &Path| -> Option<String> { fs::read_to_string(path).ok() };

        // Click on "ThingOne" in `revert ThingOne(1);`
        let offset = file_c_source.find("revert ThingOne").unwrap() + 7;
        let pos = convert::offset_to_position(file_c_source, offset);

        let result = goto_definition(file_c_source, &pos, &uri, &get_source, &resolver);
        assert!(
            result.is_some(),
            "should resolve transitively imported symbol"
        );
        if let Some(ls_types::GotoDefinitionResponse::Scalar(loc)) = result {
            // Should navigate to FileA where ThingOne is actually defined.
            let expected_uri =
                ls_types::Uri::from_file_path(file_a.canonicalize().unwrap()).unwrap();
            assert_eq!(loc.uri, expected_uri);
            assert_ne!(loc.range, ls_types::Range::default());
        } else {
            panic!("expected scalar response");
        }
    }

    #[test]
    fn test_transitive_import_member_access() {
        // FileA defines a library, FileB re-exports it, FileC uses Lib.method().
        let dir = tempfile::tempdir().unwrap();

        let file_a = dir.path().join("FileA.sol");
        fs::write(
            &file_a,
            r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

library MathLib {
    function add(uint256 a, uint256 b) internal pure returns (uint256) {
        return a + b;
    }
}
"#,
        )
        .unwrap();

        let file_b = dir.path().join("FileB.sol");
        fs::write(
            &file_b,
            r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {MathLib} from "./FileA.sol";
"#,
        )
        .unwrap();

        let file_c_source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {MathLib} from "./FileB.sol";

contract Test {
    function foo() public pure returns (uint256) {
        return MathLib.add(1, 2);
    }
}
"#;
        let file_c = dir.path().join("FileC.sol");
        fs::write(&file_c, "").unwrap();

        let uri = ls_types::Uri::from_file_path(&file_c).unwrap();
        let resolver = ImportResolver::new(Some(dir.path().to_path_buf()));
        let get_source = |path: &Path| -> Option<String> { fs::read_to_string(path).ok() };

        // Click on "add" in `MathLib.add(1, 2)`
        let offset = file_c_source.find("MathLib.add(1").unwrap() + 8;
        let pos = convert::offset_to_position(file_c_source, offset);

        let result = goto_definition(file_c_source, &pos, &uri, &get_source, &resolver);
        assert!(
            result.is_some(),
            "should resolve transitively imported member access"
        );
        if let Some(ls_types::GotoDefinitionResponse::Scalar(loc)) = result {
            let expected_uri =
                ls_types::Uri::from_file_path(file_a.canonicalize().unwrap()).unwrap();
            assert_eq!(loc.uri, expected_uri);
            assert_ne!(loc.range, ls_types::Range::default());
        } else {
            panic!("expected scalar response");
        }
    }
}
