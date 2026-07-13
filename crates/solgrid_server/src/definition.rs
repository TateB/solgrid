//! Go-to-definition handler for the LSP server.

use crate::convert;
use crate::resolve::ImportResolver;
use crate::symbols::{self, ImportedSymbols, SymbolDef, SymbolKind, SymbolTable, TypePath};
use solgrid_parser::solar_ast::{self, ItemKind, Visibility};
use solgrid_parser::with_parsed_ast_sequential;
use solgrid_project::{resolve_reference_target_at_offset, NavBackend, SolarNavBackend};
use std::collections::{HashMap, HashSet};
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
        if matches!(container.as_str(), "this" | "super") {
            let importing_file = uri_to_path(uri)?;
            let snapshot = SolarNavBackend.snapshot(&importing_file, source)?;
            let target =
                resolve_reference_target_at_offset(&snapshot, offset, get_source, resolver)?;
            let (target_uri, target_source) = if target.file_path == snapshot.path {
                (uri.clone(), source.to_string())
            } else {
                (
                    path_to_uri(&target.file_path)?,
                    get_source(&target.file_path)?,
                )
            };
            return Some(ls_types::GotoDefinitionResponse::Scalar(
                ls_types::Location {
                    uri: target_uri,
                    range: convert::span_to_range(&target_source, &target.name_span),
                },
            ));
        }

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

    let inherited =
        resolve_inherited_member_symbols(source, offset, &name, uri, &table, get_source, resolver);
    if !inherited.is_empty() {
        let locations = inherited
            .into_iter()
            .filter_map(|cross| {
                Some(ls_types::Location {
                    uri: path_to_uri(&cross.resolved_path)?,
                    range: convert::span_to_range(&cross.source, &cross.def.name_span),
                })
            })
            .collect::<Vec<_>>();
        return match locations.as_slice() {
            [] => None,
            [location] => Some(ls_types::GotoDefinitionResponse::Scalar(location.clone())),
            _ => Some(ls_types::GotoDefinitionResponse::Array(locations)),
        };
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

#[derive(Clone)]
struct ResolvedContainer {
    source: String,
    table: SymbolTable,
    def: SymbolDef,
    path: PathBuf,
}

/// Resolve the accessible overload set for an unqualified inherited member.
pub(crate) fn resolve_inherited_member_symbols(
    source: &str,
    offset: usize,
    member_name: &str,
    uri: &ls_types::Uri,
    table: &SymbolTable,
    get_source: &dyn Fn(&Path) -> Option<String>,
    resolver: &ImportResolver,
) -> Vec<CrossFileSymbol> {
    let Some(current_path) = uri_to_path(uri) else {
        return Vec::new();
    };
    let filename = current_path.to_string_lossy().to_string();
    let Some((contract_def, bases)) =
        contract_context_containing_offset(source, &filename, offset, table)
    else {
        return Vec::new();
    };
    let current = ResolvedContainer {
        source: source.to_string(),
        table: table.clone(),
        def: contract_def,
        path: current_path,
    };
    let mut cache = HashMap::new();
    let mut active = HashSet::new();
    let Some(linearized) = linearized_inheritance_order(
        &current,
        Some(&bases),
        get_source,
        resolver,
        &mut cache,
        &mut active,
    ) else {
        return Vec::new();
    };
    let mut seen = HashSet::new();
    let mut resolved = Vec::new();
    for container in linearized.into_iter().skip(1) {
        for member in container
            .table
            .resolve_member_all(&container.def, member_name)
        {
            let identity = inherited_member_lookup_identity(member);
            if !seen.insert(identity) {
                continue;
            }
            if !inherited_member_is_unqualified_accessible(member) {
                continue;
            }
            resolved.push(CrossFileSymbol {
                source: container.source.clone(),
                table: container.table.clone(),
                def: (*member).clone(),
                resolved_path: container.path.clone(),
            });
        }
    }
    resolved
}

fn inherited_member_is_unqualified_accessible(def: &SymbolDef) -> bool {
    def.visibility != Some(Visibility::Private)
        && !(def.kind == SymbolKind::Function && def.visibility == Some(Visibility::External))
}

fn inherited_member_lookup_identity(def: &SymbolDef) -> (SymbolKind, String) {
    let signature = def.signature.as_ref().map_or_else(
        || def.name.clone(),
        |signature| {
            let parameters = signature
                .parameters
                .iter()
                .map(|parameter| inherited_parameter_type_identity(&parameter.label))
                .collect::<Vec<_>>()
                .join(",");
            format!("{}({parameters})", def.name)
        },
    );
    (def.kind, signature)
}

fn inherited_parameter_type_identity(label: &str) -> String {
    let trimmed = label.trim();
    let type_text = trimmed
        .rsplit_once(char::is_whitespace)
        .filter(|(_, last)| is_solidity_identifier(last) && !is_parameter_modifier(last))
        .map_or(trimmed, |(prefix, _)| prefix.trim_end());
    let normalized = type_text
        .split_whitespace()
        .filter(|part| !matches!(*part, "memory" | "storage" | "calldata"))
        .collect::<Vec<_>>()
        .join(" ");
    for (alias, canonical) in [
        ("uint", "uint256"),
        ("int", "int256"),
        ("fixed", "fixed128x18"),
        ("ufixed", "ufixed128x18"),
    ] {
        if normalized == alias {
            return canonical.to_string();
        }
        if normalized
            .strip_prefix(alias)
            .is_some_and(|suffix| suffix.starts_with('['))
        {
            return normalized.replacen(alias, canonical, 1);
        }
    }
    normalized
}

fn is_solidity_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first == '$' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
}

fn is_parameter_modifier(value: &str) -> bool {
    matches!(
        value,
        "memory" | "storage" | "calldata" | "payable" | "indexed"
    )
}

fn linearized_inheritance_order(
    current: &ResolvedContainer,
    known_bases: Option<&[TypePath]>,
    get_source: &dyn Fn(&Path) -> Option<String>,
    resolver: &ImportResolver,
    cache: &mut HashMap<(PathBuf, usize), Vec<ResolvedContainer>>,
    active: &mut HashSet<(PathBuf, usize)>,
) -> Option<Vec<ResolvedContainer>> {
    let key = resolved_container_key(current);
    if let Some(cached) = cache.get(&key) {
        return Some(cached.clone());
    }
    if !active.insert(key.clone()) {
        return None;
    }

    let owned_bases;
    let bases = if let Some(bases) = known_bases {
        bases
    } else {
        let filename = current.path.to_string_lossy().to_string();
        owned_bases = contract_bases_for_def(&current.source, &filename, &current.def);
        &owned_bases
    };
    let mut direct_bases = Vec::new();
    for base in bases {
        let Some(container) = resolve_container_path(current, base, get_source, resolver) else {
            active.remove(&key);
            return None;
        };
        direct_bases.push(container);
    }
    // Solidity gives precedence to the rightmost direct base.
    direct_bases.reverse();

    let mut sequences = Vec::new();
    for base in &direct_bases {
        let Some(linearized) =
            linearized_inheritance_order(base, None, get_source, resolver, cache, active)
        else {
            active.remove(&key);
            return None;
        };
        sequences.push(linearized);
    }
    sequences.push(direct_bases);
    let Some(merged) = merge_linearized_containers(sequences) else {
        active.remove(&key);
        return None;
    };

    let mut result = Vec::with_capacity(1 + merged.len());
    result.push(current.clone());
    result.extend(merged);
    active.remove(&key);
    cache.insert(key, result.clone());
    Some(result)
}

fn merge_linearized_containers(
    mut sequences: Vec<Vec<ResolvedContainer>>,
) -> Option<Vec<ResolvedContainer>> {
    let mut merged: Vec<ResolvedContainer> = Vec::new();
    while sequences.iter().any(|sequence| !sequence.is_empty()) {
        let candidate = sequences
            .iter()
            .filter_map(|sequence| sequence.first())
            .find(|candidate| {
                let key = resolved_container_key(candidate);
                sequences.iter().all(|sequence| {
                    !sequence
                        .iter()
                        .skip(1)
                        .any(|entry| resolved_container_key(entry) == key)
                })
            })?;
        let candidate_key = resolved_container_key(candidate);
        if !merged
            .iter()
            .any(|entry| resolved_container_key(entry) == candidate_key)
        {
            merged.push(candidate.clone());
        }
        for sequence in &mut sequences {
            if sequence
                .first()
                .is_some_and(|entry| resolved_container_key(entry) == candidate_key)
            {
                sequence.remove(0);
            }
        }
    }
    Some(merged)
}

fn resolved_container_key(container: &ResolvedContainer) -> (PathBuf, usize) {
    (container.path.clone(), container.def.name_span.start)
}

fn resolve_container_path(
    current: &ResolvedContainer,
    path: &TypePath,
    get_source: &dyn Fn(&Path) -> Option<String>,
    resolver: &ImportResolver,
) -> Option<ResolvedContainer> {
    match path.segments.as_slice() {
        [] => None,
        [name] => {
            if let Some(def) = current.table.resolve(name, 0) {
                if is_contract_container_symbol(def.kind) {
                    return Some(ResolvedContainer {
                        source: current.source.clone(),
                        table: current.table.clone(),
                        def: def.clone(),
                        path: current.path.clone(),
                    });
                }
            }

            let cross = resolve_cross_file_symbol(
                &current.table,
                name,
                &current.path,
                get_source,
                resolver,
            )?;
            is_contract_container_symbol(cross.def.kind).then_some(ResolvedContainer {
                source: cross.source,
                table: cross.table,
                def: cross.def,
                path: cross.resolved_path,
            })
        }
        [namespace, member, ..] => {
            let cross = resolve_cross_file_member_symbol(
                &current.table,
                namespace,
                member,
                &current.path,
                get_source,
                resolver,
            )?;
            is_contract_container_symbol(cross.def.kind).then_some(ResolvedContainer {
                source: cross.source,
                table: cross.table,
                def: cross.def,
                path: cross.resolved_path,
            })
        }
    }
}

fn contract_context_containing_offset(
    source: &str,
    filename: &str,
    offset: usize,
    table: &SymbolTable,
) -> Option<(SymbolDef, Vec<TypePath>)> {
    with_parsed_ast_sequential(source, filename, |source_unit| {
        for item in source_unit.items.iter() {
            let ItemKind::Contract(contract) = &item.kind else {
                continue;
            };
            let item_span = solgrid_ast::span_to_range(item.span);
            if item_span.contains(&offset) {
                let name_span = solgrid_ast::span_to_range(contract.name.span);
                let def = table
                    .file_level_symbols()
                    .iter()
                    .find(|def| def.name_span == name_span)?
                    .clone();
                return Some((def, ast_bases_to_type_paths(contract.bases.iter())));
            }
        }
        None
    })
    .ok()
    .flatten()
}

fn contract_bases_for_def(source: &str, filename: &str, def: &SymbolDef) -> Vec<TypePath> {
    with_parsed_ast_sequential(source, filename, |source_unit| {
        for item in source_unit.items.iter() {
            let ItemKind::Contract(contract) = &item.kind else {
                continue;
            };
            if solgrid_ast::span_to_range(contract.name.span) == def.name_span {
                return ast_bases_to_type_paths(contract.bases.iter());
            }
        }
        Vec::new()
    })
    .unwrap_or_default()
}

fn ast_bases_to_type_paths<'iter, 'ast>(
    bases: impl Iterator<Item = &'iter solar_ast::Modifier<'ast>>,
) -> Vec<TypePath>
where
    'ast: 'iter,
{
    bases
        .map(|base| ast_path_to_type_path(&base.name))
        .collect()
}

fn ast_path_to_type_path(path: &solar_ast::AstPath<'_>) -> TypePath {
    TypePath {
        segments: path
            .segments()
            .iter()
            .map(|segment| segment.as_str().to_string())
            .collect(),
    }
}

fn is_contract_container_symbol(kind: SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Contract | SymbolKind::Interface | SymbolKind::Library
    )
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
        let target = match &import.symbols {
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
                    Some(n) => (n, false),
                    None => continue,
                }
            }
            ImportedSymbols::Plain(None) => (container_name, false),
            ImportedSymbols::Plain(Some(alias)) if alias == container_name => (member_name, true),
            ImportedSymbols::Glob(alias) if alias == container_name => (member_name, true),
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

        // Namespace imports resolve `Alias.Member` directly to the imported
        // file's file-level export named `Member`.
        if target.1 {
            if let Some(def) = imported_table.resolve(target.0, 0) {
                return Some(CrossFileSymbol {
                    source: imported_source,
                    def: def.clone(),
                    table: imported_table,
                    resolved_path: resolved,
                });
            }

            // Keep resolving the namespace member as a file-level symbol when
            // the imported file re-exports it. Falling through to the
            // container-member path would instead search for `Member.Member`.
            if let Some(result) = resolve_cross_file_symbol_inner(
                &imported_table,
                target.0,
                &resolved,
                get_source,
                resolver,
                visited,
            ) {
                return Some(result);
            }

            continue;
        }

        // Try direct resolution: find the container, then the member.
        if let Some(container_def) = imported_table.resolve(target.0, 0) {
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
            target.0,
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
    fn test_goto_definition_resolves_this_member_to_current_contract() {
        let source = r#"pragma solidity ^0.8.0;
contract Main {
    function selected() public {}
    function run() external { this.selected(); }
}
"#;
        let uri: ls_types::Uri = "file:///test.sol".parse().unwrap();
        let usage = source.find("this.selected").unwrap() + "this.".len();
        let result = goto_definition(
            source,
            &convert::offset_to_position(source, usage),
            &uri,
            &noop_source,
            &noop_resolver(),
        );

        let ls_types::GotoDefinitionResponse::Scalar(location) = result.unwrap() else {
            panic!("expected scalar response");
        };
        let declaration = source.find("selected() public").unwrap();
        assert_eq!(location.uri, uri);
        assert_eq!(
            location.range.start,
            convert::offset_to_position(source, declaration)
        );
    }

    #[test]
    fn test_goto_definition_resolves_super_member_to_next_c3_base() {
        let source = r#"pragma solidity ^0.8.0;
contract Left {
    function selected() public virtual {}
}
contract Right {
    function selected() public virtual {}
}
contract Main is Left, Right {
    function selected() public override(Left, Right) { super.selected(); }
}
"#;
        let uri: ls_types::Uri = "file:///test.sol".parse().unwrap();
        let usage = source.find("super.selected").unwrap() + "super.".len();
        let result = goto_definition(
            source,
            &convert::offset_to_position(source, usage),
            &uri,
            &noop_source,
            &noop_resolver(),
        );

        let ls_types::GotoDefinitionResponse::Scalar(location) = result.unwrap() else {
            panic!("expected scalar response");
        };
        let declaration = source.rfind("selected() public virtual").unwrap();
        assert_eq!(location.uri, uri);
        assert_eq!(
            location.range.start,
            convert::offset_to_position(source, declaration)
        );
    }

    #[test]
    fn test_goto_definition_resolves_inherited_unqualified_member() {
        let source = r#"pragma solidity ^0.8.0;
contract Base {
    function selected() internal {}
}
contract Main is Base {
    function run() external { selected(); }
}
"#;
        let uri: ls_types::Uri = "file:///test.sol".parse().unwrap();
        let usage = source.rfind("selected();").unwrap();
        let result = goto_definition(
            source,
            &convert::offset_to_position(source, usage),
            &uri,
            &noop_source,
            &noop_resolver(),
        );

        let ls_types::GotoDefinitionResponse::Scalar(location) = result.unwrap() else {
            panic!("expected scalar response");
        };
        let declaration = source.find("selected() internal").unwrap();
        assert_eq!(location.uri, uri);
        assert_eq!(
            location.range.start,
            convert::offset_to_position(source, declaration)
        );
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
    fn test_cross_file_member_access_via_namespace_import() {
        let dir = tempfile::tempdir().unwrap();

        let lib_path = dir.path().join("Lib.sol");
        let lib_source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

library TokenLib {
    function mint(address to, uint256 amount) internal {}
}
"#;
        fs::write(&lib_path, lib_source).unwrap();

        let main_source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "./Lib.sol" as Lib;

contract Main {
    function doMint() public {
        Lib.TokenLib.mint(msg.sender, 100);
    }
}
"#;
        let main_path = dir.path().join("Main.sol");
        fs::write(&main_path, "").unwrap();

        let uri = ls_types::Uri::from_file_path(&main_path).unwrap();
        let resolver = ImportResolver::new(Some(dir.path().to_path_buf()));
        let get_source = |path: &Path| -> Option<String> { fs::read_to_string(path).ok() };

        // Click on "TokenLib" in `Lib.TokenLib.mint(...)`
        let offset = main_source.find("Lib.TokenLib.mint").unwrap() + 4;
        let pos = convert::offset_to_position(main_source, offset);

        let result = goto_definition(main_source, &pos, &uri, &get_source, &resolver);
        assert!(
            result.is_some(),
            "expected definition for namespace-imported Lib.TokenLib"
        );
        if let Some(ls_types::GotoDefinitionResponse::Scalar(loc)) = result {
            let expected_uri =
                ls_types::Uri::from_file_path(lib_path.canonicalize().unwrap()).unwrap();
            assert_eq!(loc.uri, expected_uri);
            assert_ne!(loc.range, ls_types::Range::default());
        } else {
            panic!("expected scalar response");
        }
    }

    #[test]
    fn test_cross_file_member_access_via_transitive_namespace_re_export() {
        let dir = tempfile::tempdir().unwrap();

        let token_path = dir.path().join("Token.sol");
        let token_source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Token {}
"#;
        fs::write(&token_path, token_source).unwrap();

        let barrel_path = dir.path().join("Barrel.sol");
        fs::write(
            &barrel_path,
            r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {Token} from "./Token.sol";
"#,
        )
        .unwrap();

        let main_source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import * as Barrel from "./Barrel.sol";

contract Main {
    Barrel.Token private token;
}
"#;
        let main_path = dir.path().join("Main.sol");
        fs::write(&main_path, "").unwrap();

        let uri = ls_types::Uri::from_file_path(&main_path).unwrap();
        let resolver = ImportResolver::new(Some(dir.path().to_path_buf()));
        let get_source = |path: &Path| -> Option<String> { fs::read_to_string(path).ok() };

        let offset = main_source.find("Barrel.Token").unwrap() + "Barrel.".len();
        let pos = convert::offset_to_position(main_source, offset);

        let result = goto_definition(main_source, &pos, &uri, &get_source, &resolver);
        assert!(
            result.is_some(),
            "expected definition for transitively re-exported Barrel.Token"
        );
        if let Some(ls_types::GotoDefinitionResponse::Scalar(loc)) = result {
            let expected_uri =
                ls_types::Uri::from_file_path(token_path.canonicalize().unwrap()).unwrap();
            assert_eq!(loc.uri, expected_uri);
            let expected_offset = token_source.find("contract Token").unwrap() + "contract ".len();
            assert_eq!(
                loc.range.start,
                convert::offset_to_position(token_source, expected_offset)
            );
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
    fn test_goto_definition_inherited_interface_error() {
        let dir = tempfile::tempdir().unwrap();

        let interface_path = dir.path().join("IRentPriceOracle.sol");
        let interface_source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

interface IRentPriceOracle {
    error NotValid(string label);
}
"#;
        fs::write(&interface_path, interface_source).unwrap();

        let main_source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {IRentPriceOracle} from "./IRentPriceOracle.sol";

contract StandardRentPriceOracle is IRentPriceOracle {
    function fail(string calldata label) external {
        revert NotValid(label);
    }
}
"#;
        let main_path = dir.path().join("StandardRentPriceOracle.sol");
        fs::write(&main_path, main_source).unwrap();

        let uri = ls_types::Uri::from_file_path(&main_path).unwrap();
        let resolver = ImportResolver::new(Some(dir.path().to_path_buf()));
        let get_source = |path: &Path| -> Option<String> { fs::read_to_string(path).ok() };

        let offset = main_source.find("revert NotValid").unwrap() + 7;
        let pos = convert::offset_to_position(main_source, offset);

        let result = goto_definition(main_source, &pos, &uri, &get_source, &resolver);
        assert!(
            result.is_some(),
            "should resolve inherited interface error in revert"
        );
        if let Some(ls_types::GotoDefinitionResponse::Scalar(loc)) = result {
            let expected_uri =
                ls_types::Uri::from_file_path(interface_path.canonicalize().unwrap()).unwrap();
            assert_eq!(loc.uri, expected_uri);
            let name_offset = interface_source.find("error NotValid").unwrap() + 6;
            let expected_pos = convert::offset_to_position(interface_source, name_offset);
            assert_eq!(loc.range.start, expected_pos);
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

    #[test]
    fn test_inherited_lookup_uses_solidity_c3_precedence() {
        let source = r#"
contract A {
    function selected() internal virtual {}
}
contract B is A {
    function selected() internal virtual override {}
}
contract D is A {}
contract C is B, D {
    function run() internal { selected(); }
}
"#;
        let path = tempfile::NamedTempFile::new().unwrap();
        fs::write(path.path(), source).unwrap();
        let uri = ls_types::Uri::from_file_path(path.path()).unwrap();
        let table = symbols::build_symbol_table(source, "C.sol").unwrap();
        let offset = source.find("selected();").unwrap();
        let resolved = resolve_inherited_member_symbols(
            source,
            offset,
            "selected",
            &uri,
            &table,
            &noop_source,
            &noop_resolver(),
        )
        .into_iter()
        .next()
        .expect("inherited function should resolve");

        let expected = source
            .find("function selected() internal virtual override")
            .unwrap()
            + "function ".len();
        assert_eq!(resolved.def.name_span.start, expected);
    }

    #[test]
    fn test_inherited_lookup_does_not_expose_private_members() {
        let source = r#"
contract Base {
    function hidden() private {}
}
contract Derived is Base {
    function run() internal { hidden(); }
}
"#;
        let path = tempfile::NamedTempFile::new().unwrap();
        fs::write(path.path(), source).unwrap();
        let uri = ls_types::Uri::from_file_path(path.path()).unwrap();
        let table = symbols::build_symbol_table(source, "Derived.sol").unwrap();
        let offset = source.find("hidden();").unwrap();

        assert!(resolve_inherited_member_symbols(
            source,
            offset,
            "hidden",
            &uri,
            &table,
            &noop_source,
            &noop_resolver(),
        )
        .is_empty());
    }

    #[test]
    fn test_inherited_lookup_combines_distinct_overloads_across_c3_order() {
        let source = r#"
contract A {
    function overloaded(uint256 value) internal {}
}
contract B is A {
    function overloaded(address value) internal {}
}
contract C is B {
    function run() internal { overloaded(1); }
}
"#;
        let path = tempfile::NamedTempFile::new().unwrap();
        fs::write(path.path(), source).unwrap();
        let uri = ls_types::Uri::from_file_path(path.path()).unwrap();
        let table = symbols::build_symbol_table(source, "C.sol").unwrap();
        let offset = source.find("overloaded(1)").unwrap();

        let resolved = resolve_inherited_member_symbols(
            source,
            offset,
            "overloaded",
            &uri,
            &table,
            &noop_source,
            &noop_resolver(),
        );
        let labels = resolved
            .iter()
            .filter_map(|symbol| symbol.def.signature.as_ref())
            .map(|signature| signature.label.as_str())
            .collect::<Vec<_>>();

        assert_eq!(labels.len(), 2);
        assert!(labels.iter().any(|label| label.contains("address value")));
        assert!(labels.iter().any(|label| label.contains("uint256 value")));
    }

    #[test]
    fn test_inherited_lookup_excludes_external_functions_from_unqualified_calls() {
        let source = r#"
contract Base {
    function externalOnly() external {}
}
contract Derived is Base {
    function run() internal { externalOnly(); }
}
"#;
        let path = tempfile::NamedTempFile::new().unwrap();
        fs::write(path.path(), source).unwrap();
        let uri = ls_types::Uri::from_file_path(path.path()).unwrap();
        let table = symbols::build_symbol_table(source, "Derived.sol").unwrap();
        let offset = source.find("externalOnly();").unwrap();

        assert!(resolve_inherited_member_symbols(
            source,
            offset,
            "externalOnly",
            &uri,
            &table,
            &noop_source,
            &noop_resolver(),
        )
        .is_empty());
    }

    #[test]
    fn test_inaccessible_nearest_override_shadows_lower_signature() {
        let source = r#"
contract Base {
    function shadowed(uint256 value) public virtual {}
}
contract Middle is Base {
    function shadowed(uint256 value) external override {}
}
contract Derived is Middle {
    function run() internal { shadowed(1); }
}
"#;
        let path = tempfile::NamedTempFile::new().unwrap();
        fs::write(path.path(), source).unwrap();
        let uri = ls_types::Uri::from_file_path(path.path()).unwrap();
        let table = symbols::build_symbol_table(source, "Derived.sol").unwrap();
        let offset = source.find("shadowed(1)").unwrap();

        assert!(resolve_inherited_member_symbols(
            source,
            offset,
            "shadowed",
            &uri,
            &table,
            &noop_source,
            &noop_resolver(),
        )
        .is_empty());
    }
}
