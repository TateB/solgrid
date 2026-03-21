//! Completion — intelligent autocomplete for Solidity files.
//!
//! Provides:
//! - In-scope symbol completions (local variables, parameters, contract members)
//! - Dot completions (`msg.`, `MyContract.`, `MyEnum.`)
//! - Builtin completions (global functions, keywords, types)
//! - Auto-import completions (symbols from other workspace files)
//! - Suppression comment completions (`// solgrid-disable-next-line`)

use crate::definition::{self, uri_to_path};
use crate::resolve::ImportResolver;
use crate::symbols::{self, ImportedSymbols, SymbolKind};
use crate::workspace_index::WorkspaceIndex;
use crate::{builtins, convert};
use solgrid_linter::LintEngine;
use std::collections::HashSet;
use std::path::Path;
use tower_lsp_server::ls_types;

// ── Suppression comment completions ────────────────────────────────────────

/// Suppression comment prefixes that we complete.
const SUPPRESSION_PREFIXES: &[&str] = &[
    "// solgrid-disable-next-line",
    "// solgrid-disable-line",
    "// solgrid-disable",
    "// solgrid-enable",
];

/// Solidity built-in types for keyword completions.
const SOLIDITY_TYPES: &[&str] = &[
    "address", "bool", "string", "bytes", "bytes1", "bytes2", "bytes3", "bytes4", "bytes8",
    "bytes16", "bytes20", "bytes32", "int8", "int16", "int32", "int64", "int128", "int256",
    "uint8", "uint16", "uint32", "uint64", "uint128", "uint256",
];

/// Solidity keywords for keyword completions.
const SOLIDITY_KEYWORDS: &[&str] = &[
    "abstract",
    "anonymous",
    "assembly",
    "break",
    "calldata",
    "catch",
    "constant",
    "constructor",
    "continue",
    "contract",
    "delete",
    "do",
    "else",
    "emit",
    "enum",
    "error",
    "event",
    "external",
    "fallback",
    "for",
    "function",
    "if",
    "immutable",
    "import",
    "indexed",
    "interface",
    "internal",
    "library",
    "mapping",
    "memory",
    "modifier",
    "new",
    "override",
    "payable",
    "pragma",
    "private",
    "public",
    "pure",
    "receive",
    "return",
    "returns",
    "revert",
    "storage",
    "struct",
    "super",
    "this",
    "try",
    "type",
    "unchecked",
    "using",
    "view",
    "virtual",
    "while",
];

// ── Main entry point ───────────────────────────────────────────────────────

/// Generate completion items for the given position.
///
/// Dispatches to the appropriate completion provider based on context:
/// suppression comments, dot completions, or identifier completions.
pub fn completions(
    engine: &LintEngine,
    source: &str,
    position: &ls_types::Position,
    uri: &ls_types::Uri,
    get_source: &dyn Fn(&Path) -> Option<String>,
    resolver: &ImportResolver,
    workspace_index: &WorkspaceIndex,
) -> Vec<ls_types::CompletionItem> {
    // 1. Check for suppression comment context first.
    let suppression = suppression_completions(engine, source, position);
    if !suppression.is_empty() {
        return suppression;
    }

    let offset = convert::position_to_offset(source, *position);

    // 2. Check for dot completion context.
    if let Some(container_name) = find_dot_context(source, offset) {
        return dot_completions(source, &container_name, offset, uri, get_source, resolver);
    }

    // 3. General identifier completions.
    identifier_completions(source, offset, uri, get_source, resolver, workspace_index)
}

// ── Dot completions ────────────────────────────────────────────────────────

/// Extract the container name if the cursor is right after a `.` (dot completion context).
///
/// Given `msg.` with cursor after the dot, returns `Some("msg")`.
/// Given `MyLib.ad` with cursor on `d`, returns `Some("MyLib")`.
fn find_dot_context(source: &str, offset: usize) -> Option<String> {
    let bytes = source.as_bytes();
    if offset == 0 {
        return None;
    }

    // Walk backward from cursor to find the dot.
    let mut pos = offset;

    // First skip any partial identifier the user is typing after the dot.
    while pos > 0 && is_ident_char(bytes[pos - 1]) {
        pos -= 1;
    }

    // Now we should be right after the dot.
    if pos == 0 || bytes[pos - 1] != b'.' {
        return None;
    }
    pos -= 1; // skip the dot

    // Skip whitespace before the dot (be tolerant).
    while pos > 0 && bytes[pos - 1].is_ascii_whitespace() {
        pos -= 1;
    }

    // Extract the container identifier ending at pos.
    if pos == 0 {
        return None;
    }
    let end = pos;
    while pos > 0 && is_ident_char(bytes[pos - 1]) {
        pos -= 1;
    }
    if pos == end {
        return None;
    }
    // First char must not be a digit.
    if bytes[pos].is_ascii_digit() {
        return None;
    }

    Some(source[pos..end].to_string())
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

/// Patch source around the cursor to make it parseable.
///
/// When the user types `Status.` or `msg.sen`, the source is incomplete.
/// We replace the incomplete expression with a valid placeholder so the parser
/// can still build a symbol table for scope resolution.
fn patch_source_for_completion(source: &str, offset: usize) -> String {
    // Find the start of the current statement (scan back to `;`, `{`, or line start).
    let bytes = source.as_bytes();
    let mut stmt_start = offset;
    while stmt_start > 0 {
        let b = bytes[stmt_start - 1];
        if b == b';' || b == b'{' || b == b'}' {
            break;
        }
        stmt_start -= 1;
    }

    // Find the end of the current line or statement.
    let mut stmt_end = offset;
    while stmt_end < bytes.len() {
        let b = bytes[stmt_end];
        if b == b';' || b == b'\n' || b == b'}' {
            break;
        }
        stmt_end += 1;
    }

    // Replace the incomplete statement region with a placeholder.
    let mut patched = String::with_capacity(source.len());
    patched.push_str(&source[..stmt_start]);
    // Add enough whitespace to keep offsets roughly aligned.
    let gap = stmt_end - stmt_start;
    for _ in 0..gap {
        patched.push(' ');
    }
    patched.push_str(&source[stmt_end..]);
    patched
}

/// Generate completions for members of a container (after `.`).
fn dot_completions(
    source: &str,
    container_name: &str,
    offset: usize,
    uri: &ls_types::Uri,
    get_source: &dyn Fn(&Path) -> Option<String>,
    resolver: &ImportResolver,
) -> Vec<ls_types::CompletionItem> {
    // Check builtin namespaces first (msg, block, tx, abi, etc.).
    let members = builtins::namespace_members(container_name);
    if !members.is_empty() {
        return members
            .into_iter()
            .map(|(name, def)| ls_types::CompletionItem {
                label: name.to_string(),
                kind: Some(ls_types::CompletionItemKind::FIELD),
                detail: Some(def.signature.to_string()),
                documentation: Some(ls_types::Documentation::String(def.description.to_string())),
                ..Default::default()
            })
            .collect();
    }

    // Patch the source to handle incomplete expressions (e.g., `Status.`).
    let patched = patch_source_for_completion(source, offset);
    let table = match symbols::build_symbol_table(&patched, "buffer.sol") {
        Some(t) => t,
        // If even the patched source fails to parse, try the original.
        None => match symbols::build_symbol_table(source, "buffer.sol") {
            Some(t) => t,
            None => return Vec::new(),
        },
    };

    if let Some(container_def) = table.resolve(container_name, offset) {
        if let Some(scope_id) = container_def.scope {
            return table
                .scope_symbols(scope_id)
                .iter()
                .map(|sym| symbol_to_completion_item(sym, "a_"))
                .collect();
        }
    }

    // Try cross-file container resolution.
    if let Some(importing_file) = uri_to_path(uri) {
        if let Some(cross) = definition::resolve_cross_file_symbol(
            &table,
            container_name,
            &importing_file,
            get_source,
            resolver,
        ) {
            if let Some(container_def) = cross.table.resolve(&cross.def.name, 0) {
                if let Some(scope_id) = container_def.scope {
                    return cross
                        .table
                        .scope_symbols(scope_id)
                        .iter()
                        .map(|sym| symbol_to_completion_item(sym, "a_"))
                        .collect();
                }
            }
        }
    }

    Vec::new()
}

// ── Identifier completions ─────────────────────────────────────────────────

/// Generate completions for identifiers (not after a dot).
fn identifier_completions(
    source: &str,
    offset: usize,
    uri: &ls_types::Uri,
    get_source: &dyn Fn(&Path) -> Option<String>,
    resolver: &ImportResolver,
    workspace_index: &WorkspaceIndex,
) -> Vec<ls_types::CompletionItem> {
    // Try parsing the original source first, then fall back to a patched version.
    let table = match symbols::build_symbol_table(source, "buffer.sol") {
        Some(t) => t,
        None => {
            let patched = patch_source_for_completion(source, offset);
            match symbols::build_symbol_table(&patched, "buffer.sol") {
                Some(t) => t,
                None => return builtin_and_keyword_completions(),
            }
        }
    };

    let mut items = Vec::new();
    let mut seen_names: HashSet<String> = HashSet::new();

    // a) In-scope symbols from the current file.
    for sym in table.visible_symbols_at(offset) {
        if seen_names.insert(sym.name.clone()) {
            items.push(symbol_to_completion_item(sym, "a_"));
        }
    }

    // b) Imported cross-file symbols.
    if let Some(importing_file) = uri_to_path(uri) {
        for import in &table.imports {
            let resolved_path = match resolver.resolve(&import.path, &importing_file) {
                Some(p) => p,
                None => continue,
            };
            let imported_source = match get_source(&resolved_path) {
                Some(s) => s,
                None => continue,
            };
            let filename = resolved_path.to_string_lossy().to_string();
            let imported_table = match symbols::build_symbol_table(&imported_source, &filename) {
                Some(t) => t,
                None => continue,
            };

            match &import.symbols {
                ImportedSymbols::Named(names) => {
                    for (original, alias) in names {
                        let local_name = alias.as_deref().unwrap_or(original.as_str());
                        if seen_names.insert(local_name.to_string()) {
                            if let Some(sym) = imported_table.resolve(original, 0) {
                                let mut item = symbol_to_completion_item(sym, "b_");
                                // Use the local alias name if different.
                                if let Some(a) = alias {
                                    item.label = a.clone();
                                }
                                items.push(item);
                            }
                        }
                    }
                }
                ImportedSymbols::Plain(None) => {
                    // Plain import: all file-level symbols are in scope.
                    for sym in imported_table.file_level_symbols() {
                        if seen_names.insert(sym.name.clone()) {
                            items.push(symbol_to_completion_item(sym, "b_"));
                        }
                    }
                }
                ImportedSymbols::Plain(Some(alias)) => {
                    // import "file.sol" as Alias — the alias is a namespace.
                    if seen_names.insert(alias.clone()) {
                        items.push(ls_types::CompletionItem {
                            label: alias.clone(),
                            kind: Some(ls_types::CompletionItemKind::MODULE),
                            detail: Some(format!("import \"{}\"", import.path)),
                            sort_text: Some(format!("b_{alias}")),
                            ..Default::default()
                        });
                    }
                }
                ImportedSymbols::Glob(alias) => {
                    // import * as Alias — the alias is a namespace.
                    if seen_names.insert(alias.clone()) {
                        items.push(ls_types::CompletionItem {
                            label: alias.clone(),
                            kind: Some(ls_types::CompletionItemKind::MODULE),
                            detail: Some(format!("import * from \"{}\"", import.path)),
                            sort_text: Some(format!("b_{alias}")),
                            ..Default::default()
                        });
                    }
                }
            }
        }
    }

    // c) Builtin globals (keccak256, require, etc.).
    for (name, def) in builtins::solidity_globals() {
        if seen_names.insert(name.to_string()) {
            items.push(ls_types::CompletionItem {
                label: name.to_string(),
                kind: Some(ls_types::CompletionItemKind::FUNCTION),
                detail: Some(def.signature.to_string()),
                sort_text: Some(format!("c_{name}")),
                ..Default::default()
            });
        }
    }

    // c2) Builtin namespace names (msg, block, tx, abi, etc.).
    for name in builtins::namespace_names() {
        if seen_names.insert(name.to_string()) {
            items.push(ls_types::CompletionItem {
                label: name.to_string(),
                kind: Some(ls_types::CompletionItemKind::MODULE),
                sort_text: Some(format!("c_{name}")),
                ..Default::default()
            });
        }
    }

    // d) Solidity types and keywords.
    for &ty in SOLIDITY_TYPES {
        if seen_names.insert(ty.to_string()) {
            items.push(ls_types::CompletionItem {
                label: ty.to_string(),
                kind: Some(ls_types::CompletionItemKind::TYPE_PARAMETER),
                sort_text: Some(format!("d_{ty}")),
                ..Default::default()
            });
        }
    }
    for &kw in SOLIDITY_KEYWORDS {
        if seen_names.insert(kw.to_string()) {
            items.push(ls_types::CompletionItem {
                label: kw.to_string(),
                kind: Some(ls_types::CompletionItemKind::KEYWORD),
                sort_text: Some(format!("d_{kw}")),
                ..Default::default()
            });
        }
    }

    // e) Auto-import candidates from workspace index.
    if let Some(current_path) = uri_to_path(uri) {
        for entry in workspace_index.symbols_matching("") {
            // Skip symbols from the current file.
            if entry.file_path == current_path {
                continue;
            }
            // Skip already-available symbols.
            if seen_names.contains(&entry.name) {
                continue;
            }
            // Deduplicate: only add once per name (first file wins).
            if !seen_names.insert(entry.name.clone()) {
                continue;
            }

            let relative = compute_relative_path(&current_path, &entry.file_path);
            let import_edit = compute_import_edit(source, &entry.name, &relative);

            items.push(ls_types::CompletionItem {
                label: entry.name.clone(),
                kind: Some(symbol_kind_to_completion_kind(entry.kind)),
                detail: Some(format!("Auto import from {relative}")),
                sort_text: Some(format!("zz_{}", entry.name)),
                additional_text_edits: Some(vec![import_edit]),
                ..Default::default()
            });
        }
    }

    items
}

// ── Helpers ────────────────────────────────────────────────────────────────

/// Return only builtin and keyword completions (fallback when parsing fails).
fn builtin_and_keyword_completions() -> Vec<ls_types::CompletionItem> {
    let mut items = Vec::new();
    for (name, def) in builtins::solidity_globals() {
        items.push(ls_types::CompletionItem {
            label: name.to_string(),
            kind: Some(ls_types::CompletionItemKind::FUNCTION),
            detail: Some(def.signature.to_string()),
            sort_text: Some(format!("c_{name}")),
            ..Default::default()
        });
    }
    for name in builtins::namespace_names() {
        items.push(ls_types::CompletionItem {
            label: name.to_string(),
            kind: Some(ls_types::CompletionItemKind::MODULE),
            sort_text: Some(format!("c_{name}")),
            ..Default::default()
        });
    }
    for &ty in SOLIDITY_TYPES {
        items.push(ls_types::CompletionItem {
            label: ty.to_string(),
            kind: Some(ls_types::CompletionItemKind::TYPE_PARAMETER),
            sort_text: Some(format!("d_{ty}")),
            ..Default::default()
        });
    }
    for &kw in SOLIDITY_KEYWORDS {
        items.push(ls_types::CompletionItem {
            label: kw.to_string(),
            kind: Some(ls_types::CompletionItemKind::KEYWORD),
            sort_text: Some(format!("d_{kw}")),
            ..Default::default()
        });
    }
    items
}

/// Convert a `SymbolDef` to a `CompletionItem`.
fn symbol_to_completion_item(
    sym: &symbols::SymbolDef,
    sort_prefix: &str,
) -> ls_types::CompletionItem {
    ls_types::CompletionItem {
        label: sym.name.clone(),
        kind: Some(symbol_kind_to_completion_kind(sym.kind)),
        sort_text: Some(format!("{sort_prefix}{}", sym.name)),
        ..Default::default()
    }
}

/// Map `SymbolKind` to LSP `CompletionItemKind`.
fn symbol_kind_to_completion_kind(kind: SymbolKind) -> ls_types::CompletionItemKind {
    match kind {
        SymbolKind::Contract => ls_types::CompletionItemKind::CLASS,
        SymbolKind::Interface => ls_types::CompletionItemKind::INTERFACE,
        SymbolKind::Library => ls_types::CompletionItemKind::MODULE,
        SymbolKind::Function => ls_types::CompletionItemKind::FUNCTION,
        SymbolKind::Modifier => ls_types::CompletionItemKind::METHOD,
        SymbolKind::Event => ls_types::CompletionItemKind::EVENT,
        SymbolKind::Error => ls_types::CompletionItemKind::STRUCT,
        SymbolKind::Struct => ls_types::CompletionItemKind::STRUCT,
        SymbolKind::StructField => ls_types::CompletionItemKind::FIELD,
        SymbolKind::Enum => ls_types::CompletionItemKind::ENUM,
        SymbolKind::EnumVariant => ls_types::CompletionItemKind::ENUM_MEMBER,
        SymbolKind::Udvt => ls_types::CompletionItemKind::TYPE_PARAMETER,
        SymbolKind::StateVariable | SymbolKind::LocalVariable => {
            ls_types::CompletionItemKind::VARIABLE
        }
        SymbolKind::Parameter | SymbolKind::ReturnParameter => {
            ls_types::CompletionItemKind::VARIABLE
        }
    }
}

/// Compute a relative import path from `from_file` to `to_file`.
///
/// Returns a path like `"./Token.sol"` or `"../lib/Token.sol"`.
fn compute_relative_path(from_file: &Path, to_file: &Path) -> String {
    let from_dir = from_file.parent().unwrap_or(Path::new(""));
    let to_dir = to_file.parent().unwrap_or(Path::new(""));
    let to_name = to_file
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    // Find common prefix length.
    let from_components: Vec<_> = from_dir.components().collect();
    let to_components: Vec<_> = to_dir.components().collect();

    let common_len = from_components
        .iter()
        .zip(to_components.iter())
        .take_while(|(a, b)| a == b)
        .count();

    let ups = from_components.len() - common_len;
    let mut parts = Vec::new();

    if ups == 0 {
        parts.push(".".to_string());
    } else {
        for _ in 0..ups {
            parts.push("..".to_string());
        }
    }

    for comp in &to_components[common_len..] {
        parts.push(comp.as_os_str().to_string_lossy().to_string());
    }

    parts.push(to_name);
    parts.join("/")
}

/// Compute the `TextEdit` to insert an import statement for auto-import.
fn compute_import_edit(source: &str, symbol_name: &str, relative_path: &str) -> ls_types::TextEdit {
    let insert_line = find_import_insertion_line(source);
    let new_text = format!("import {{{symbol_name}}} from \"{relative_path}\";\n");

    ls_types::TextEdit {
        range: ls_types::Range {
            start: ls_types::Position::new(insert_line, 0),
            end: ls_types::Position::new(insert_line, 0),
        },
        new_text,
    }
}

/// Find the line number where a new import should be inserted.
///
/// Inserts after the last existing import, or after the pragma line,
/// or at line 0 if neither exists.
fn find_import_insertion_line(source: &str) -> u32 {
    let mut last_import_line: Option<u32> = None;
    let mut pragma_line: Option<u32> = None;
    let mut license_line: Option<u32> = None;

    for (i, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("import ") || trimmed.starts_with("import{") {
            last_import_line = Some(i as u32);
        } else if trimmed.starts_with("pragma ") {
            pragma_line = Some(i as u32);
        } else if trimmed.starts_with("// SPDX") {
            license_line = Some(i as u32);
        }
    }

    // Insert after last import, or after pragma, or after license, or at top.
    if let Some(line) = last_import_line {
        line + 1
    } else if let Some(line) = pragma_line {
        line + 1
    } else if let Some(line) = license_line {
        line + 1
    } else {
        0
    }
}

// ── Suppression completions (preserved from original) ──────────────────────

/// Generate completion items for suppression comments.
fn suppression_completions(
    engine: &LintEngine,
    source: &str,
    position: &ls_types::Position,
) -> Vec<ls_types::CompletionItem> {
    let line_text = match get_line_text(source, position.line as usize) {
        Some(text) => text,
        None => return Vec::new(),
    };

    let prefix = line_text
        .get(..position.character as usize)
        .unwrap_or(line_text)
        .trim();

    if prefix.starts_with("// solgrid-") || prefix.starts_with("// solhint-") {
        for suppression_prefix in SUPPRESSION_PREFIXES {
            if prefix.starts_with(suppression_prefix) {
                return rule_id_completions(engine);
            }
        }
        return directive_completions();
    }

    if prefix.starts_with("// sol") || prefix == "//" || prefix == "// " {
        return directive_completions();
    }

    Vec::new()
}

fn directive_completions() -> Vec<ls_types::CompletionItem> {
    vec![
        ls_types::CompletionItem {
            label: "solgrid-disable-next-line".into(),
            kind: Some(ls_types::CompletionItemKind::SNIPPET),
            detail: Some("Disable rule for the next line".into()),
            insert_text: Some("solgrid-disable-next-line ".into()),
            ..Default::default()
        },
        ls_types::CompletionItem {
            label: "solgrid-disable-line".into(),
            kind: Some(ls_types::CompletionItemKind::SNIPPET),
            detail: Some("Disable rule for this line".into()),
            insert_text: Some("solgrid-disable-line ".into()),
            ..Default::default()
        },
        ls_types::CompletionItem {
            label: "solgrid-disable".into(),
            kind: Some(ls_types::CompletionItemKind::SNIPPET),
            detail: Some("Disable rule for the following block".into()),
            insert_text: Some("solgrid-disable ".into()),
            ..Default::default()
        },
        ls_types::CompletionItem {
            label: "solgrid-enable".into(),
            kind: Some(ls_types::CompletionItemKind::SNIPPET),
            detail: Some("Re-enable a previously disabled rule".into()),
            insert_text: Some("solgrid-enable ".into()),
            ..Default::default()
        },
    ]
}

fn rule_id_completions(engine: &LintEngine) -> Vec<ls_types::CompletionItem> {
    engine
        .registry()
        .all_meta()
        .into_iter()
        .map(|meta| ls_types::CompletionItem {
            label: meta.id.to_string(),
            kind: Some(ls_types::CompletionItemKind::VALUE),
            detail: Some(meta.description.to_string()),
            ..Default::default()
        })
        .collect()
}

fn get_line_text(source: &str, line: usize) -> Option<&str> {
    source.lines().nth(line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace_index::WorkspaceIndex;
    use std::fs;

    fn empty_index() -> WorkspaceIndex {
        WorkspaceIndex::new()
    }

    fn noop_resolver() -> ImportResolver {
        ImportResolver::new(None)
    }

    fn noop_source(_path: &Path) -> Option<String> {
        None
    }

    // ── Suppression completions (preserved) ────────────────────────────────

    #[test]
    fn test_directive_completions() {
        let items = directive_completions();
        assert_eq!(items.len(), 4);
        assert!(items.iter().any(|i| i.label == "solgrid-disable-next-line"));
        assert!(items.iter().any(|i| i.label == "solgrid-disable-line"));
        assert!(items.iter().any(|i| i.label == "solgrid-disable"));
        assert!(items.iter().any(|i| i.label == "solgrid-enable"));
    }

    #[test]
    fn test_rule_id_completions() {
        let engine = LintEngine::new();
        let items = rule_id_completions(&engine);
        assert!(!items.is_empty());
        assert!(items.iter().any(|i| i.label == "security/tx-origin"));
    }

    #[test]
    fn test_suppression_completions_after_directive() {
        let engine = LintEngine::new();
        let source = "// solgrid-disable-next-line \n";
        let position = ls_types::Position::new(0, 29);
        let items = suppression_completions(&engine, source, &position);
        assert!(!items.is_empty());
        assert!(items.iter().any(|i| i.label == "security/tx-origin"));
    }

    #[test]
    fn test_suppression_completions_typing_directive() {
        let engine = LintEngine::new();
        let source = "// sol\n";
        let position = ls_types::Position::new(0, 6);
        let items = suppression_completions(&engine, source, &position);
        assert!(!items.is_empty());
        assert!(items.iter().any(|i| i.label == "solgrid-disable-next-line"));
    }

    #[test]
    fn test_suppression_completions_no_context() {
        let engine = LintEngine::new();
        let source = "contract Test {}\n";
        let position = ls_types::Position::new(0, 5);
        let items = suppression_completions(&engine, source, &position);
        assert!(items.is_empty());
    }

    // ── Dot context detection ──────────────────────────────────────────────

    #[test]
    fn test_find_dot_context_basic() {
        let source = "msg.";
        assert_eq!(find_dot_context(source, 4), Some("msg".to_string()));
    }

    #[test]
    fn test_find_dot_context_partial_member() {
        let source = "msg.sen";
        assert_eq!(find_dot_context(source, 7), Some("msg".to_string()));
    }

    #[test]
    fn test_find_dot_context_no_dot() {
        let source = "msg";
        assert_eq!(find_dot_context(source, 3), None);
    }

    #[test]
    fn test_find_dot_context_contract() {
        let source = "MyContract.transfer(";
        assert_eq!(find_dot_context(source, 19), Some("MyContract".to_string()));
    }

    // ── Dot completions ────────────────────────────────────────────────────

    #[test]
    fn test_dot_completions_msg() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function foo() public {
        msg.
    }
}
"#;
        let offset = source.find("msg.").unwrap() + 4;
        let pos = convert::offset_to_position(source, offset);
        let uri: ls_types::Uri = "file:///test.sol".parse().unwrap();
        let engine = LintEngine::new();

        let items = completions(
            &engine,
            source,
            &pos,
            &uri,
            &noop_source,
            &noop_resolver(),
            &empty_index(),
        );

        assert!(items.iter().any(|i| i.label == "sender"));
        assert!(items.iter().any(|i| i.label == "value"));
        assert!(items.iter().any(|i| i.label == "data"));
        assert!(items.iter().any(|i| i.label == "sig"));
    }

    #[test]
    fn test_dot_completions_enum() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    enum Status { Active, Paused, Closed }
    function foo() public {
        Status.
    }
}
"#;
        let offset = source.find("Status.").unwrap() + 7;
        let pos = convert::offset_to_position(source, offset);
        let uri: ls_types::Uri = "file:///test.sol".parse().unwrap();
        let engine = LintEngine::new();

        let items = completions(
            &engine,
            source,
            &pos,
            &uri,
            &noop_source,
            &noop_resolver(),
            &empty_index(),
        );

        assert!(items.iter().any(|i| i.label == "Active"));
        assert!(items.iter().any(|i| i.label == "Paused"));
        assert!(items.iter().any(|i| i.label == "Closed"));
    }

    #[test]
    fn test_dot_completions_library() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
library MathLib {
    function add(uint256 a, uint256 b) internal pure returns (uint256) {
        return a + b;
    }
}
contract Test {
    function foo() public pure returns (uint256) {
        return MathLib.
    }
}
"#;
        let offset = source.find("MathLib.").unwrap() + 8;
        let pos = convert::offset_to_position(source, offset);
        let uri: ls_types::Uri = "file:///test.sol".parse().unwrap();
        let engine = LintEngine::new();

        let items = completions(
            &engine,
            source,
            &pos,
            &uri,
            &noop_source,
            &noop_resolver(),
            &empty_index(),
        );

        assert!(items.iter().any(|i| i.label == "add"));
    }

    // ── In-scope completions ───────────────────────────────────────────────

    #[test]
    fn test_identifier_completions_in_scope() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    uint256 public value;
    event Transfer(address from, address to, uint256 amount);

    function foo(uint256 x) public {
        uint256 local = 1;

    }
}
"#;
        // Position on the empty line inside foo()
        let offset = source.find("uint256 local").unwrap() + 24;
        let pos = convert::offset_to_position(source, offset);
        let uri: ls_types::Uri = "file:///test.sol".parse().unwrap();
        let engine = LintEngine::new();

        let items = completions(
            &engine,
            source,
            &pos,
            &uri,
            &noop_source,
            &noop_resolver(),
            &empty_index(),
        );

        // Should include local variable, parameter, state variable, contract
        assert!(items.iter().any(|i| i.label == "local"));
        assert!(items.iter().any(|i| i.label == "x"));
        assert!(items.iter().any(|i| i.label == "value"));
        assert!(items.iter().any(|i| i.label == "Test"));
        assert!(items.iter().any(|i| i.label == "Transfer"));
        // Should include builtins
        assert!(items.iter().any(|i| i.label == "keccak256"));
        assert!(items.iter().any(|i| i.label == "msg"));
        // Should include types/keywords
        assert!(items.iter().any(|i| i.label == "uint256"));
        assert!(items.iter().any(|i| i.label == "address"));
    }

    // ── Auto-import completions ────────────────────────────────────────────

    #[test]
    fn test_auto_import_completions() {
        let dir = tempfile::tempdir().unwrap();

        let token_path = dir.path().join("Token.sol");
        fs::write(
            &token_path,
            r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Token {}
"#,
        )
        .unwrap();

        let main_path = dir.path().join("Main.sol");
        let main_source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Main {}
"#;
        fs::write(&main_path, main_source).unwrap();

        let index = WorkspaceIndex::build(dir.path());
        let uri = ls_types::Uri::from_file_path(&main_path).unwrap();
        let engine = LintEngine::new();
        let resolver = ImportResolver::new(Some(dir.path().to_path_buf()));

        // Position inside the file (after pragma)
        let offset = main_source.find("contract Main").unwrap();
        let pos = convert::offset_to_position(main_source, offset);

        let items = completions(
            &engine,
            main_source,
            &pos,
            &uri,
            &noop_source,
            &resolver,
            &index,
        );

        // Token from Token.sol should appear as auto-import
        let token_item = items.iter().find(|i| i.label == "Token");
        assert!(
            token_item.is_some(),
            "Token should appear as auto-import completion"
        );

        let token_item = token_item.unwrap();
        assert!(token_item
            .detail
            .as_ref()
            .unwrap()
            .starts_with("Auto import"));
        assert!(token_item.additional_text_edits.is_some());

        let edits = token_item.additional_text_edits.as_ref().unwrap();
        assert_eq!(edits.len(), 1);
        assert!(edits[0].new_text.contains("import {Token}"));
    }

    #[test]
    fn test_auto_import_not_duplicated() {
        let dir = tempfile::tempdir().unwrap();

        let token_path = dir.path().join("Token.sol");
        fs::write(
            &token_path,
            r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Token {}
"#,
        )
        .unwrap();

        let main_path = dir.path().join("Main.sol");
        let main_source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {Token} from "./Token.sol";

contract Main is Token {}
"#;
        fs::write(&main_path, main_source).unwrap();

        let index = WorkspaceIndex::build(dir.path());
        let uri = ls_types::Uri::from_file_path(&main_path).unwrap();
        let engine = LintEngine::new();
        let resolver = ImportResolver::new(Some(dir.path().to_path_buf()));

        let get_source = |path: &Path| -> Option<String> { std::fs::read_to_string(path).ok() };

        let offset = main_source.find("contract Main").unwrap();
        let pos = convert::offset_to_position(main_source, offset);

        let items = completions(
            &engine,
            main_source,
            &pos,
            &uri,
            &get_source,
            &resolver,
            &index,
        );

        // Token should appear exactly once (as imported, not as auto-import).
        let token_items: Vec<_> = items.iter().filter(|i| i.label == "Token").collect();
        assert_eq!(
            token_items.len(),
            1,
            "Token should appear exactly once, not duplicated"
        );
        // It should be the imported version (no auto-import edits).
        assert!(
            token_items[0].additional_text_edits.is_none(),
            "Already-imported Token should not have additional_text_edits"
        );
    }

    // ── Import edit helpers ────────────────────────────────────────────────

    #[test]
    fn test_compute_relative_path_same_dir() {
        let from = Path::new("/project/src/Main.sol");
        let to = Path::new("/project/src/Token.sol");
        assert_eq!(compute_relative_path(from, to), "./Token.sol");
    }

    #[test]
    fn test_compute_relative_path_parent_dir() {
        let from = Path::new("/project/src/sub/Main.sol");
        let to = Path::new("/project/src/Token.sol");
        assert_eq!(compute_relative_path(from, to), "../Token.sol");
    }

    #[test]
    fn test_find_import_insertion_line_after_imports() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {Foo} from "./Foo.sol";
import {Bar} from "./Bar.sol";

contract Test {}
"#;
        assert_eq!(find_import_insertion_line(source), 5);
    }

    #[test]
    fn test_find_import_insertion_line_after_pragma() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {}
"#;
        assert_eq!(find_import_insertion_line(source), 2);
    }
}
