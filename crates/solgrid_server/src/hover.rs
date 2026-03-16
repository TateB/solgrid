//! Hover — symbol signatures, NatSpec documentation, and rule documentation.

use crate::builtins::{self, BuiltinDef};
use crate::symbols::{SymbolDef, SymbolKind};
use crate::{convert, symbols};
use solgrid_diagnostics::{FixAvailability, RuleMeta};
use solgrid_linter::LintEngine;
use tower_lsp_server::ls_types;

/// Generate hover information for a position in a document.
///
/// If the position overlaps with a diagnostic, shows the rule documentation.
/// Otherwise, if a symbol can be resolved, shows its signature and NatSpec.
pub fn hover_at_position(
    engine: &LintEngine,
    lsp_diagnostics: &[ls_types::Diagnostic],
    position: &ls_types::Position,
    source: &str,
) -> Option<ls_types::Hover> {
    // First: check if hovering over a diagnostic
    if let Some(hover) = hover_for_diagnostic(engine, lsp_diagnostics, position) {
        return Some(hover);
    }

    // Second: try code intelligence (signature + NatSpec)
    hover_for_symbol(source, position)
}

pub fn hover_for_diagnostic(
    engine: &LintEngine,
    lsp_diagnostics: &[ls_types::Diagnostic],
    position: &ls_types::Position,
) -> Option<ls_types::Hover> {
    for diag in lsp_diagnostics {
        if !position_in_range(position, &diag.range) {
            continue;
        }

        let rule_id = match &diag.code {
            Some(ls_types::NumberOrString::String(id)) => id.as_str(),
            _ => continue,
        };

        if let Some(rule) = engine.registry().get(rule_id) {
            let meta = rule.meta();
            let content = format_rule_documentation(meta);
            return Some(ls_types::Hover {
                contents: ls_types::HoverContents::Markup(ls_types::MarkupContent {
                    kind: ls_types::MarkupKind::Markdown,
                    value: content,
                }),
                range: Some(diag.range),
            });
        }
    }

    None
}

/// Generate hover information for a symbol at a position.
///
/// Shows the definition signature (e.g., function header, variable type) as a
/// Solidity code block, followed by any NatSpec documentation.
/// Falls back to built-in definitions for native Solidity/Yul identifiers.
pub fn hover_for_symbol(source: &str, position: &ls_types::Position) -> Option<ls_types::Hover> {
    let offset = convert::position_to_offset(source, *position);
    let table = symbols::build_symbol_table(source, "buffer.sol");

    // Try member access first: `Container.member`
    if let Some((container, _member, member_range)) =
        symbols::find_member_access_at_offset(source, offset)
    {
        let member_name = &source[member_range.clone()];

        // 1a. Try user-defined symbol resolution
        if let Some(tbl) = &table {
            if let Some(container_def) = tbl.resolve(&container, offset) {
                if let Some(member_def) = tbl.resolve_member(container_def, member_name) {
                    return Some(hover_for_user_symbol(
                        source,
                        member_def,
                        tbl,
                        &member_range,
                    ));
                }
            }
        }

        // 1b. Fall back to builtin namespace member (msg.sender, abi.encode, etc.)
        if let Some(builtin) = builtins::lookup_solidity_member(&container, member_name) {
            return Some(make_builtin_hover(builtin, source, &member_range));
        }

        // Could not resolve member access — fall through to try the identifier alone
    }

    // Try simple identifier
    let (name, ident_range) = symbols::find_ident_at_offset(source, offset)?;

    // 2a. Try user-defined symbol resolution
    if let Some(tbl) = &table {
        if let Some(def) = tbl.resolve(&name, offset) {
            return Some(hover_for_user_symbol(source, def, tbl, &ident_range));
        }
    }

    // 2b. Try Solidity global function (keccak256, require, etc.)
    if let Some(builtin) = builtins::lookup_solidity_global(&name) {
        return Some(make_builtin_hover(builtin, source, &ident_range));
    }

    // 2c. Try Solidity namespace (hovering on `msg`, `block`, `abi`, etc.)
    if let Some(builtin) = builtins::lookup_solidity_namespace(&name) {
        return Some(make_builtin_hover(builtin, source, &ident_range));
    }

    // 2d. Try Yul built-in (only inside assembly blocks)
    if is_inside_assembly(source, offset) {
        if let Some(builtin) = builtins::lookup_yul_builtin(&name) {
            return Some(make_builtin_hover(builtin, source, &ident_range));
        }
    }

    None
}

/// Build a hover response for a user-defined symbol (extracted from the original inline logic).
fn hover_for_user_symbol(
    source: &str,
    def: &SymbolDef,
    table: &symbols::SymbolTable,
    ident_range: &std::ops::Range<usize>,
) -> ls_types::Hover {
    let signature = extract_signature(source, def, table);

    // For parameters/returns, extract the relevant @param/@return from the parent function.
    let doc_md = if matches!(
        def.kind,
        SymbolKind::Parameter | SymbolKind::ReturnParameter
    ) {
        table
            .find_enclosing_function(def.def_span.start)
            .and_then(|func_def| extract_natspec(source, func_def.def_span.start))
            .and_then(|raw| {
                let is_return = def.kind == SymbolKind::ReturnParameter;
                extract_param_doc(&raw, &def.name, is_return)
            })
    } else {
        extract_natspec(source, def.def_span.start).map(|doc| format_natspec(&doc))
    };

    let mut content = String::new();
    content.push_str("```solidity\n");
    content.push_str(&signature);
    content.push_str("\n```");

    if let Some(doc) = &doc_md {
        content.push_str("\n\n---\n\n");
        content.push_str(doc);
    }

    let range = convert::span_to_range(source, ident_range);

    ls_types::Hover {
        contents: ls_types::HoverContents::Markup(ls_types::MarkupContent {
            kind: ls_types::MarkupKind::Markdown,
            value: content,
        }),
        range: Some(range),
    }
}

/// Build a hover response for a built-in definition.
fn make_builtin_hover(
    builtin: &BuiltinDef,
    source: &str,
    ident_range: &std::ops::Range<usize>,
) -> ls_types::Hover {
    let mut content = String::new();
    content.push_str("```solidity\n");
    content.push_str(builtin.signature);
    content.push_str("\n```");

    if !builtin.description.is_empty() {
        content.push_str("\n\n---\n\n");
        content.push_str(builtin.description);
    }

    let range = convert::span_to_range(source, ident_range);

    ls_types::Hover {
        contents: ls_types::HoverContents::Markup(ls_types::MarkupContent {
            kind: ls_types::MarkupKind::Markdown,
            value: content,
        }),
        range: Some(range),
    }
}

/// Heuristic check: is the byte offset inside an `assembly { }` block?
///
/// Walks backward from `offset`, tracking brace depth. When an unmatched `{`
/// is found, checks whether it is preceded by the `assembly` keyword.
fn is_inside_assembly(source: &str, offset: usize) -> bool {
    let before = &source[..offset.min(source.len())];
    let bytes = before.as_bytes();
    let mut depth: i32 = 0;
    let mut i = bytes.len();

    while i > 0 {
        i -= 1;
        match bytes[i] {
            b'}' => depth += 1,
            b'{' => {
                depth -= 1;
                if depth < 0 {
                    // Found an unmatched opening brace — check for `assembly` keyword.
                    let preceding = &before[..i];
                    let trimmed = preceding.trim_end();
                    if trimmed.ends_with("assembly") {
                        let before_keyword = trimmed.len() - "assembly".len();
                        // Make sure it's a standalone keyword (not part of another identifier).
                        if before_keyword == 0
                            || !before.as_bytes()[before_keyword - 1].is_ascii_alphanumeric()
                        {
                            return true;
                        }
                    }
                    return false;
                }
            }
            _ => {}
        }
    }

    false
}

/// Extract a human-readable signature from a symbol's definition span.
fn extract_signature(source: &str, def: &SymbolDef, table: &symbols::SymbolTable) -> String {
    let text = &source[def.def_span.clone()];

    match def.kind {
        // Show container with its members.
        SymbolKind::Contract | SymbolKind::Interface | SymbolKind::Library => {
            format_container_signature(source, def, table)
        }

        // Truncate at `{` to get the header only.
        SymbolKind::Function | SymbolKind::Modifier => truncate_at_char(text, '{'),

        // Truncate at `;`.
        SymbolKind::Event | SymbolKind::Error | SymbolKind::Udvt => truncate_at_char(text, ';'),

        // Show full definition (usually short).
        SymbolKind::Struct | SymbolKind::Enum | SymbolKind::EnumVariant => {
            normalize_whitespace(text)
        }

        // Truncate at assignment `=` or `;` to show type without initializer.
        // Uses `find_assignment_eq` to avoid matching `=>` in mapping types.
        SymbolKind::StateVariable | SymbolKind::LocalVariable => {
            let trimmed = if let Some(eq) = find_assignment_eq(text) {
                if let Some(semi) = text.find(';') {
                    &text[..eq.min(semi)]
                } else {
                    &text[..eq]
                }
            } else {
                truncate_at_char_raw(text, ';')
            };
            normalize_whitespace(trimmed)
        }

        // Struct fields: show type + name (truncate at `;`).
        SymbolKind::StructField => truncate_at_char(text, ';'),

        // Parameters: show name: type with (parameter)/(return) prefix.
        SymbolKind::Parameter => format_param_hover(source, def, "(parameter)"),
        SymbolKind::ReturnParameter => format_param_hover(source, def, "(return)"),
    }
}

/// Format a parameter or return value for hover display as `(prefix) type name`.
fn format_param_hover(source: &str, def: &SymbolDef, prefix: &str) -> String {
    let text = &source[def.def_span.clone()];
    format!("{} {}", prefix, normalize_whitespace(text))
}

/// Format a contract/interface/library hover showing its header and members.
fn format_container_signature(
    source: &str,
    def: &SymbolDef,
    table: &symbols::SymbolTable,
) -> String {
    let text = &source[def.def_span.clone()];
    let header = truncate_at_char(text, '{');

    let scope_id = match def.scope {
        Some(id) => id,
        None => return header,
    };

    let members = table.scope_symbols(scope_id);
    if members.is_empty() {
        return header;
    }

    const MAX_MEMBERS: usize = 20;
    let mut lines = Vec::new();
    lines.push(format!("{} {{", header));

    for (i, member) in members.iter().enumerate() {
        if i >= MAX_MEMBERS {
            let remaining = members.len() - MAX_MEMBERS;
            lines.push(format!("  // ... and {} more", remaining));
            break;
        }
        let member_text = &source[member.def_span.clone()];
        let member_sig = match member.kind {
            SymbolKind::Function | SymbolKind::Modifier => truncate_at_char(member_text, '{'),
            SymbolKind::Event | SymbolKind::Error | SymbolKind::Udvt => {
                truncate_at_char(member_text, ';')
            }
            SymbolKind::StateVariable => {
                let trimmed = if let Some(eq) = find_assignment_eq(member_text) {
                    if let Some(semi) = member_text.find(';') {
                        &member_text[..eq.min(semi)]
                    } else {
                        &member_text[..eq]
                    }
                } else {
                    truncate_at_char_raw(member_text, ';')
                };
                normalize_whitespace(trimmed)
            }
            SymbolKind::Struct => format!("struct {}", member.name),
            SymbolKind::Enum => format!("enum {}", member.name),
            _ => continue,
        };
        lines.push(format!(
            "  {};",
            member_sig.trim_end_matches(';').trim_end()
        ));
    }

    lines.push("}".to_string());
    lines.join("\n")
}

/// Extract NatSpec comment preceding an item.
fn extract_natspec(source: &str, item_start: usize) -> Option<String> {
    let before = &source[..item_start];
    let trimmed = before.trim_end();
    if trimmed.is_empty() {
        return None;
    }

    // Check for /** ... */ block comment
    if trimmed.ends_with("*/") {
        if let Some(block_start) = trimmed.rfind("/**") {
            return Some(trimmed[block_start..].to_string());
        }
        return None;
    }

    // Check for consecutive /// lines
    let mut natspec_lines = Vec::new();
    for line in before.lines().rev() {
        let trimmed_line = line.trim();
        if trimmed_line.is_empty() {
            if natspec_lines.is_empty() {
                continue;
            } else {
                break;
            }
        }
        if trimmed_line.starts_with("///") {
            natspec_lines.push(trimmed_line.to_string());
        } else {
            break;
        }
    }

    if natspec_lines.is_empty() {
        return None;
    }

    natspec_lines.reverse();
    Some(natspec_lines.join("\n"))
}

/// Strip NatSpec comment markers from a single line, returning the inner content.
fn strip_natspec_marker(line: &str) -> &str {
    let trimmed = line.trim();
    if let Some(rest) = trimmed.strip_prefix("///") {
        rest.trim()
    } else if let Some(rest) = trimmed.strip_prefix("/**") {
        rest.trim()
    } else if trimmed.starts_with("*/") {
        ""
    } else if let Some(rest) = trimmed.strip_prefix("* ") {
        rest.trim()
    } else if let Some(rest) = trimmed.strip_prefix('*') {
        rest.trim()
    } else {
        trimmed
    }
}

/// Format NatSpec into readable markdown.
fn format_natspec(natspec: &str) -> String {
    let mut lines = Vec::new();
    for line in natspec.lines() {
        let content = strip_natspec_marker(line);
        if content.is_empty() {
            continue;
        }

        if content.starts_with('@') {
            // Format NatSpec tags as bold
            if let Some(rest) = content.strip_prefix("@notice ") {
                lines.push(rest.to_string());
            } else if let Some(rest) = content.strip_prefix("@dev ") {
                lines.push(format!("*Dev:* {rest}"));
            } else if let Some(rest) = content.strip_prefix("@param ") {
                lines.push(format!("**@param** {rest}"));
            } else if let Some(rest) = content.strip_prefix("@return ") {
                lines.push(format!("**@return** {rest}"));
            } else if let Some(rest) = content.strip_prefix("@title ") {
                lines.push(format!("**{rest}**"));
            } else if let Some(rest) = content.strip_prefix("@author ") {
                lines.push(format!("*Author: {rest}*"));
            } else {
                lines.push(content.to_string());
            }
        } else {
            lines.push(content.to_string());
        }
    }
    lines.join("\n\n")
}

/// Extract the description for a specific `@param` or `@return` tag from raw NatSpec.
fn extract_param_doc(natspec: &str, param_name: &str, is_return: bool) -> Option<String> {
    let tag = if is_return { "@return" } else { "@param" };

    for line in natspec.lines() {
        let content = strip_natspec_marker(line).trim_end_matches("*/").trim_end();
        if let Some(rest) = content.strip_prefix(tag) {
            let rest = rest.trim_start();
            if let Some(after_name) = rest.strip_prefix(param_name) {
                if after_name.is_empty() || after_name.starts_with(char::is_whitespace) {
                    let desc = after_name.trim();
                    if !desc.is_empty() {
                        return Some(desc.to_string());
                    }
                }
            }
        }
    }
    None
}

/// Find the position of an assignment `=` that is not part of `=>`, `==`, `!=`, `<=`, `>=`.
fn find_assignment_eq(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i < len {
        if bytes[i] == b'=' {
            // Skip `=>` (mapping arrow)
            if i + 1 < len && bytes[i + 1] == b'>' {
                i += 2;
                continue;
            }
            // Skip `==`
            if i + 1 < len && bytes[i + 1] == b'=' {
                i += 2;
                continue;
            }
            // Skip `!=`, `<=`, `>=` (we're on the `=` following `!`, `<`, `>`)
            if i > 0 && matches!(bytes[i - 1], b'!' | b'<' | b'>') {
                i += 1;
                continue;
            }
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Truncate text at the first occurrence of `ch`, trim, and normalize whitespace.
fn truncate_at_char(text: &str, ch: char) -> String {
    let truncated = truncate_at_char_raw(text, ch);
    normalize_whitespace(truncated)
}

fn truncate_at_char_raw(text: &str, ch: char) -> &str {
    if let Some(pos) = text.find(ch) {
        text[..pos].trim_end()
    } else {
        text.trim_end()
    }
}

/// Collapse consecutive whitespace (including newlines) into single spaces.
fn normalize_whitespace(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut prev_was_ws = false;
    for ch in text.trim().chars() {
        if ch.is_whitespace() {
            if !prev_was_ws {
                result.push(' ');
                prev_was_ws = true;
            }
        } else {
            result.push(ch);
            prev_was_ws = false;
        }
    }
    result
}

/// Format rule documentation as Markdown.
pub fn format_rule_documentation(meta: &RuleMeta) -> String {
    let mut doc = String::new();

    doc.push_str(&format!("## {} `{}`\n\n", meta.category, meta.name));
    doc.push_str(&format!("**Rule:** `{}`\n\n", meta.id));
    doc.push_str(&format!("**Severity:** {}\n\n", meta.default_severity));
    doc.push_str(&format!("{}\n", meta.description));

    match meta.fix_availability {
        FixAvailability::Available(safety) => {
            doc.push_str(&format!("\n**Auto-fix:** {} ({})\n", "available", safety));
        }
        FixAvailability::None => {
            doc.push_str("\n**Auto-fix:** not available\n");
        }
    }

    doc.push_str(&format!(
        "\n---\n*Disable with:* `// solgrid-disable-next-line {}`",
        meta.id
    ));

    doc
}

/// Check if a position falls within a range.
fn position_in_range(position: &ls_types::Position, range: &ls_types::Range) -> bool {
    if position.line < range.start.line || position.line > range.end.line {
        return false;
    }
    if position.line == range.start.line && position.character < range.start.character {
        return false;
    }
    if position.line == range.end.line && position.character > range.end.character {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use solgrid_diagnostics::{FixAvailability, RuleCategory, Severity};

    #[test]
    fn test_position_in_range() {
        let range = ls_types::Range {
            start: ls_types::Position::new(1, 5),
            end: ls_types::Position::new(1, 15),
        };
        assert!(position_in_range(&ls_types::Position::new(1, 5), &range));
        assert!(position_in_range(&ls_types::Position::new(1, 10), &range));
        assert!(position_in_range(&ls_types::Position::new(1, 15), &range));
        assert!(!position_in_range(&ls_types::Position::new(0, 5), &range));
        assert!(!position_in_range(&ls_types::Position::new(1, 4), &range));
        assert!(!position_in_range(&ls_types::Position::new(1, 16), &range));
        assert!(!position_in_range(&ls_types::Position::new(2, 5), &range));
    }

    #[test]
    fn test_format_rule_documentation() {
        let meta = RuleMeta {
            id: "security/tx-origin",
            name: "tx-origin",
            category: RuleCategory::Security,
            default_severity: Severity::Error,
            description: "Avoid using tx.origin for authorization.",
            fix_availability: FixAvailability::None,
        };

        let doc = format_rule_documentation(&meta);
        assert!(doc.contains("security/tx-origin"));
        assert!(doc.contains("Avoid using tx.origin"));
        assert!(doc.contains("not available"));
        assert!(doc.contains("solgrid-disable-next-line"));
    }

    #[test]
    fn test_hover_for_diagnostic_found() {
        let engine = LintEngine::new();

        let lsp_diags = vec![ls_types::Diagnostic {
            range: ls_types::Range {
                start: ls_types::Position::new(5, 8),
                end: ls_types::Position::new(5, 30),
            },
            severity: Some(ls_types::DiagnosticSeverity::ERROR),
            code: Some(ls_types::NumberOrString::String(
                "security/tx-origin".into(),
            )),
            source: Some("solgrid".into()),
            message: "Avoid tx.origin".into(),
            ..Default::default()
        }];

        let hover = hover_for_diagnostic(&engine, &lsp_diags, &ls_types::Position::new(5, 15));
        assert!(hover.is_some());
        let hover = hover.unwrap();
        match hover.contents {
            ls_types::HoverContents::Markup(markup) => {
                assert!(markup.value.contains("tx-origin"));
            }
            _ => panic!("expected markup content"),
        }
    }

    #[test]
    fn test_hover_for_diagnostic_not_found() {
        let engine = LintEngine::new();
        let lsp_diags = vec![];
        let hover = hover_for_diagnostic(&engine, &lsp_diags, &ls_types::Position::new(0, 0));
        assert!(hover.is_none());
    }

    fn hover_value(source: &str, position: ls_types::Position) -> Option<String> {
        let hover = hover_for_symbol(source, &position)?;
        match hover.contents {
            ls_types::HoverContents::Markup(m) => Some(m.value),
            _ => None,
        }
    }

    #[test]
    fn test_hover_function() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    function transfer(address to, uint256 amount) external returns (bool) {
        return true;
    }

    function use_it() public {
        transfer(msg.sender, 100);
    }
}
"#;
        // Hover on "transfer" in the function definition (line 4, col 13)
        let val = hover_value(source, ls_types::Position::new(4, 13)).unwrap();
        assert!(
            val.contains("function transfer(address to, uint256 amount) external returns (bool)")
        );
        assert!(!val.contains("{"));
    }

    #[test]
    fn test_hover_state_variable() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    uint256 public totalSupply = 1000;
    function get() public view returns (uint256) {
        return totalSupply;
    }
}
"#;
        // Hover on "totalSupply" at usage site (line 6, on 'totalSupply')
        let offset = source.find("return totalSupply").unwrap() + 7;
        let pos = convert::offset_to_position(source, offset);
        let val = hover_value(source, pos).unwrap();
        assert!(val.contains("uint256 public totalSupply"));
        assert!(!val.contains("1000"));
    }

    #[test]
    fn test_hover_parameter() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    function add(uint256 a, uint256 b) public pure returns (uint256) {
        return a + b;
    }
}
"#;
        let offset = source.find("return a").unwrap() + 7;
        let pos = convert::offset_to_position(source, offset);
        let val = hover_value(source, pos).unwrap();
        assert!(val.contains("(parameter) uint256 a"), "got: {val}");
    }

    #[test]
    fn test_hover_contract() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract MyToken {
    uint256 x;
}
"#;
        let offset = source.find("MyToken").unwrap();
        let pos = convert::offset_to_position(source, offset);
        let val = hover_value(source, pos).unwrap();
        assert!(val.contains("contract MyToken {"), "got: {val}");
        assert!(val.contains("uint256 x"), "got: {val}");
    }

    #[test]
    fn test_hover_event() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    event Transfer(address indexed from, address indexed to, uint256 value);
}
"#;
        let offset = source.find("Transfer").unwrap();
        let pos = convert::offset_to_position(source, offset);
        let val = hover_value(source, pos).unwrap();
        assert!(
            val.contains("event Transfer(address indexed from, address indexed to, uint256 value)")
        );
    }

    #[test]
    fn test_hover_error() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    error InsufficientBalance(uint256 balance, uint256 amount);
}
"#;
        let offset = source.find("InsufficientBalance").unwrap();
        let pos = convert::offset_to_position(source, offset);
        let val = hover_value(source, pos).unwrap();
        assert!(val.contains("error InsufficientBalance(uint256 balance, uint256 amount)"));
    }

    #[test]
    fn test_hover_struct() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    struct Position { address token; uint256 amount; }
}
"#;
        let offset = source.find("Position").unwrap();
        let pos = convert::offset_to_position(source, offset);
        let val = hover_value(source, pos).unwrap();
        assert!(val.contains("struct Position"));
        assert!(val.contains("address token"));
    }

    #[test]
    fn test_hover_udvt() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

type Price is uint256;
"#;
        let offset = source.find("Price").unwrap();
        let pos = convert::offset_to_position(source, offset);
        let val = hover_value(source, pos).unwrap();
        assert!(val.contains("type Price is uint256"));
    }

    #[test]
    fn test_hover_nonident_returns_none() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {}
"#;
        // Position on a space
        assert!(hover_for_symbol(source, &ls_types::Position::new(0, 0)).is_none());
    }

    // -- Mapping and complex type tests --

    #[test]
    fn test_hover_mapping_simple() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    mapping(address => uint256) public balances;
}
"#;
        let offset = source.find("balances").unwrap();
        let pos = convert::offset_to_position(source, offset);
        let val = hover_value(source, pos).unwrap();
        assert!(val.contains("mapping(address => uint256) public balances"));
    }

    #[test]
    fn test_hover_nested_mapping() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    mapping(address => mapping(address => bool)) private _operatorApprovals;
}
"#;
        let offset = source.find("_operatorApprovals").unwrap();
        let pos = convert::offset_to_position(source, offset);
        let val = hover_value(source, pos).unwrap();
        assert!(
            val.contains("mapping(address => mapping(address => bool)) private _operatorApprovals"),
            "got: {val}"
        );
    }

    #[test]
    fn test_hover_triple_nested_mapping() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    mapping(address => mapping(address => mapping(uint256 => bool))) public nested;
}
"#;
        let offset = source.find("nested").unwrap();
        let pos = convert::offset_to_position(source, offset);
        let val = hover_value(source, pos).unwrap();
        assert!(
            val.contains(
                "mapping(address => mapping(address => mapping(uint256 => bool))) public nested"
            ),
            "got: {val}"
        );
    }

    #[test]
    fn test_hover_mapping_with_initializer() {
        // Mappings can't have initializers, but test that = inside mapping type doesn't break
        // things and that a real assignment = is handled correctly.
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    uint256 public value = 42;
}
"#;
        let offset = source.find("value").unwrap();
        let pos = convert::offset_to_position(source, offset);
        let val = hover_value(source, pos).unwrap();
        assert!(val.contains("uint256 public value"));
        assert!(!val.contains("42"));
    }

    // -- Array types --

    #[test]
    fn test_hover_dynamic_array() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    uint256[] public values;
}
"#;
        let offset = source.find("values").unwrap();
        let pos = convert::offset_to_position(source, offset);
        let val = hover_value(source, pos).unwrap();
        assert!(val.contains("uint256[] public values"), "got: {val}");
    }

    #[test]
    fn test_hover_fixed_array() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    uint256[10] public fixedValues;
}
"#;
        let offset = source.find("fixedValues").unwrap();
        let pos = convert::offset_to_position(source, offset);
        let val = hover_value(source, pos).unwrap();
        assert!(val.contains("uint256[10] public fixedValues"), "got: {val}");
    }

    #[test]
    fn test_hover_nested_array() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    address[][] public nestedAddrs;
}
"#;
        let offset = source.find("nestedAddrs").unwrap();
        let pos = convert::offset_to_position(source, offset);
        let val = hover_value(source, pos).unwrap();
        assert!(val.contains("address[][] public nestedAddrs"), "got: {val}");
    }

    // -- Complex variable types --

    #[test]
    fn test_hover_bytes_types() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    bytes public data;
    bytes32 public hash;
    string public name;
}
"#;
        let offset = source.find(" data").unwrap() + 1;
        let pos = convert::offset_to_position(source, offset);
        let val = hover_value(source, pos).unwrap();
        assert!(val.contains("bytes public data"), "got: {val}");

        let offset = source.find(" hash").unwrap() + 1;
        let pos = convert::offset_to_position(source, offset);
        let val = hover_value(source, pos).unwrap();
        assert!(val.contains("bytes32 public hash"), "got: {val}");

        let offset = source.find(" name").unwrap() + 1;
        let pos = convert::offset_to_position(source, offset);
        let val = hover_value(source, pos).unwrap();
        assert!(val.contains("string public name"), "got: {val}");
    }

    #[test]
    fn test_hover_address_payable() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    address payable public recipient;
}
"#;
        let offset = source.find("recipient").unwrap();
        let pos = convert::offset_to_position(source, offset);
        let val = hover_value(source, pos).unwrap();
        assert!(
            val.contains("address payable public recipient"),
            "got: {val}"
        );
    }

    #[test]
    fn test_hover_constant_immutable() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    uint256 public constant MAX_SUPPLY = 1000000;
    address public immutable owner;
}
"#;
        let offset = source.find("MAX_SUPPLY").unwrap();
        let pos = convert::offset_to_position(source, offset);
        let val = hover_value(source, pos).unwrap();
        assert!(
            val.contains("uint256 public constant MAX_SUPPLY"),
            "got: {val}"
        );
        assert!(!val.contains("1000000"));

        let offset = source.find("owner").unwrap();
        let pos = convert::offset_to_position(source, offset);
        let val = hover_value(source, pos).unwrap();
        assert!(val.contains("address public immutable owner"), "got: {val}");
    }

    #[test]
    fn test_hover_custom_type_variable() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    struct Info { uint256 id; address addr; }
    Info public myInfo;
}
"#;
        let offset = source.find("myInfo").unwrap();
        let pos = convert::offset_to_position(source, offset);
        let val = hover_value(source, pos).unwrap();
        assert!(val.contains("Info public myInfo"), "got: {val}");
    }

    #[test]
    fn test_hover_mapping_to_struct_array() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    struct Order { uint256 amount; }
    mapping(address => Order[]) public orders;
}
"#;
        let offset = source.find("orders").unwrap();
        let pos = convert::offset_to_position(source, offset);
        let val = hover_value(source, pos).unwrap();
        assert!(
            val.contains("mapping(address => Order[]) public orders"),
            "got: {val}"
        );
    }

    // -- Function signature variations --

    #[test]
    fn test_hover_function_visibility_modifiers() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    function pubFn() public pure returns (uint256) { return 1; }
    function intFn() internal view returns (bool) { return true; }
    function extFn() external payable {}
    function privFn() private {}
}
"#;
        let offset = source.find("pubFn").unwrap();
        let val = hover_value(source, convert::offset_to_position(source, offset)).unwrap();
        assert!(
            val.contains("function pubFn() public pure returns (uint256)"),
            "got: {val}"
        );

        let offset = source.find("intFn").unwrap();
        let val = hover_value(source, convert::offset_to_position(source, offset)).unwrap();
        assert!(
            val.contains("function intFn() internal view returns (bool)"),
            "got: {val}"
        );

        let offset = source.find("extFn").unwrap();
        let val = hover_value(source, convert::offset_to_position(source, offset)).unwrap();
        assert!(
            val.contains("function extFn() external payable"),
            "got: {val}"
        );

        let offset = source.find("privFn").unwrap();
        let val = hover_value(source, convert::offset_to_position(source, offset)).unwrap();
        assert!(val.contains("function privFn() private"), "got: {val}");
    }

    #[test]
    fn test_hover_function_complex_params() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    function complexFn(
        address[] calldata addrs,
        mapping(address => uint256) storage balances,
        bytes32 hash
    ) internal returns (bool success, uint256 count) {
        return (true, 0);
    }
}
"#;
        let offset = source.find("complexFn").unwrap();
        let val = hover_value(source, convert::offset_to_position(source, offset)).unwrap();
        assert!(val.contains("function complexFn("), "got: {val}");
        assert!(val.contains("address[] calldata addrs"), "got: {val}");
        assert!(
            val.contains("returns (bool success, uint256 count)"),
            "got: {val}"
        );
        assert!(!val.contains("{"), "got: {val}");
    }

    // -- Special constructs --

    #[test]
    fn test_hover_modifier() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    modifier onlyOwner() {
        _;
    }
}
"#;
        let offset = source.find("onlyOwner").unwrap();
        let val = hover_value(source, convert::offset_to_position(source, offset)).unwrap();
        assert!(val.contains("modifier onlyOwner()"), "got: {val}");
        assert!(!val.contains("{"));
    }

    #[test]
    fn test_hover_error_with_params() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    error TransferFailed(address from, address to, uint256 amount);
}
"#;
        let offset = source.find("TransferFailed").unwrap();
        let val = hover_value(source, convert::offset_to_position(source, offset)).unwrap();
        assert!(
            val.contains("error TransferFailed(address from, address to, uint256 amount)"),
            "got: {val}"
        );
    }

    #[test]
    fn test_hover_event_with_indexed() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    event Approval(address indexed owner, address indexed spender, uint256 value);
}
"#;
        let offset = source.find("Approval").unwrap();
        let val = hover_value(source, convert::offset_to_position(source, offset)).unwrap();
        assert!(
            val.contains(
                "event Approval(address indexed owner, address indexed spender, uint256 value)"
            ),
            "got: {val}"
        );
    }

    #[test]
    fn test_hover_enum_variants() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    enum Color { Red, Green, Blue }
}
"#;
        let offset = source.find("Color").unwrap();
        let val = hover_value(source, convert::offset_to_position(source, offset)).unwrap();
        assert!(
            val.contains("enum Color { Red, Green, Blue }"),
            "got: {val}"
        );
    }

    #[test]
    fn test_hover_contract_inheritance() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Base {}
contract Child is Base {
    uint256 x;
}
"#;
        let offset = source.find("Child").unwrap();
        let val = hover_value(source, convert::offset_to_position(source, offset)).unwrap();
        assert!(val.contains("contract Child is Base {"), "got: {val}");
        assert!(val.contains("uint256 x"), "got: {val}");
    }

    // -- Member access hover --

    #[test]
    fn test_hover_member_access_contract_function() {
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
        // Hover on "add" in "MathLib.add"
        let offset = source.find("MathLib.add(1").unwrap() + 8; // on 'a' of 'add'
        let val = hover_value(source, convert::offset_to_position(source, offset)).unwrap();
        assert!(
            val.contains("function add(uint256 a, uint256 b) internal pure returns (uint256)"),
            "got: {val}"
        );
    }

    #[test]
    fn test_hover_member_access_enum_variant() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    enum Status { Active, Paused }
    function getActive() public pure returns (Status) {
        return Status.Active;
    }
}
"#;
        // Hover on "Active" in "Status.Active"
        let offset = source.find("Status.Active").unwrap() + 7; // on 'A' of 'Active'
        let val = hover_value(source, convert::offset_to_position(source, offset)).unwrap();
        assert!(val.contains("Active"), "got: {val}");
    }

    #[test]
    fn test_hover_member_access_struct_field() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    struct Position {
        address token;
        uint256 amount;
    }
    function getToken(Position memory pos) public pure returns (address) {
        return Position.token;
    }
}
"#;
        // Hover on "token" in "Position.token"
        let offset = source.find("Position.token").unwrap() + 9; // on 't' of 'token'
        let val = hover_value(source, convert::offset_to_position(source, offset)).unwrap();
        assert!(val.contains("address token"), "got: {val}");
    }

    // -- Local variable types --

    #[test]
    fn test_hover_local_variable_types() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    function foo() public pure {
        uint256 x = 42;
        bool flag = true;
        address addr = address(0);
        bytes memory data = "";
        string memory label = "hello";
    }
}
"#;
        // Check each local var — use "name =" or "name;" patterns for precise matching
        let offset = source.find("x = 42").unwrap();
        let val = hover_value(source, convert::offset_to_position(source, offset)).unwrap();
        assert!(val.contains("uint256 x"), "got: {val}");
        assert!(!val.contains("42"));

        let offset = source.find("flag =").unwrap();
        let val = hover_value(source, convert::offset_to_position(source, offset)).unwrap();
        assert!(val.contains("bool flag"), "got: {val}");

        let offset = source.find("addr =").unwrap();
        let val = hover_value(source, convert::offset_to_position(source, offset)).unwrap();
        assert!(val.contains("address addr"), "got: {val}");

        let offset = source.find("data =").unwrap();
        let val = hover_value(source, convert::offset_to_position(source, offset)).unwrap();
        assert!(val.contains("bytes memory data"), "got: {val}");

        let offset = source.find("label =").unwrap();
        let val = hover_value(source, convert::offset_to_position(source, offset)).unwrap();
        assert!(val.contains("string memory label"), "got: {val}");
    }

    // -- extract_param_doc tests --

    #[test]
    fn test_extract_param_doc() {
        let natspec = "/// @notice Does stuff\n/// @param to The recipient\n/// @param amount The amount\n/// @return success Whether it worked";
        assert_eq!(
            extract_param_doc(natspec, "to", false),
            Some("The recipient".to_string())
        );
        assert_eq!(
            extract_param_doc(natspec, "amount", false),
            Some("The amount".to_string())
        );
        assert_eq!(
            extract_param_doc(natspec, "success", true),
            Some("Whether it worked".to_string())
        );
        // Non-existent param
        assert_eq!(extract_param_doc(natspec, "foo", false), None);
        // Param name prefix shouldn't match longer name
        assert_eq!(extract_param_doc(natspec, "to", true), None);
    }

    #[test]
    fn test_extract_param_doc_block_comment() {
        let natspec = "/** @param token The token address\n * @param amount The amount */";
        assert_eq!(
            extract_param_doc(natspec, "token", false),
            Some("The token address".to_string())
        );
        assert_eq!(
            extract_param_doc(natspec, "amount", false),
            Some("The amount".to_string())
        );
    }

    // -- find_assignment_eq tests --

    #[test]
    fn test_find_assignment_eq() {
        assert_eq!(find_assignment_eq("uint256 x = 42"), Some(10));
        assert_eq!(find_assignment_eq("mapping(a => b) x"), None);
        assert_eq!(
            find_assignment_eq("mapping(a => b) x = something"),
            Some(18)
        );
        assert_eq!(find_assignment_eq("x == y"), None);
        assert_eq!(find_assignment_eq("x != y"), None);
        assert_eq!(find_assignment_eq("x <= y"), None);
        assert_eq!(find_assignment_eq("x >= y"), None);
        assert_eq!(
            find_assignment_eq("mapping(address => mapping(address => bool)) x = val"),
            Some(47)
        );
    }

    #[test]
    fn test_hover_with_natspec() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    /// @notice Transfers tokens to the given address.
    /// @param to The recipient address
    /// @param amount The amount to transfer
    function transfer(address to, uint256 amount) external returns (bool) {
        return true;
    }
}
"#;
        let offset = source.find("transfer").unwrap();
        let pos = convert::offset_to_position(source, offset);
        let val = hover_value(source, pos).unwrap();
        assert!(val.contains("function transfer"));
        assert!(val.contains("Transfers tokens"));
        assert!(val.contains("**@param**"));
    }

    #[test]
    fn test_hover_return_parameter() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    function add(uint256 a, uint256 b) public pure returns (uint256 result) {
        result = a + b;
    }
}
"#;
        let offset = source.find("result = a").unwrap();
        let pos = convert::offset_to_position(source, offset);
        let val = hover_value(source, pos).unwrap();
        assert!(val.contains("(return) uint256 result"), "got: {val}");
    }

    #[test]
    fn test_hover_parameter_with_natspec() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    /// @notice Transfers tokens to the given address.
    /// @param to The recipient address
    /// @param amount The amount to transfer
    /// @return success Whether the transfer succeeded
    function transfer(address to, uint256 amount) external returns (bool success) {
        to;
        success = amount > 0;
    }
}
"#;
        // Hover on param "to" at usage site
        let offset = source.find("        to;").unwrap() + 8;
        let pos = convert::offset_to_position(source, offset);
        let val = hover_value(source, pos).unwrap();
        assert!(val.contains("(parameter) address to"), "got: {val}");
        assert!(val.contains("The recipient address"), "got: {val}");

        // Hover on param "amount" at usage site
        let offset = source.find("amount > 0").unwrap();
        let pos = convert::offset_to_position(source, offset);
        let val = hover_value(source, pos).unwrap();
        assert!(val.contains("(parameter) uint256 amount"), "got: {val}");
        assert!(val.contains("The amount to transfer"), "got: {val}");

        // Hover on return param "success"
        let offset = source.find("success = amount").unwrap();
        let pos = convert::offset_to_position(source, offset);
        let val = hover_value(source, pos).unwrap();
        assert!(val.contains("(return) bool success"), "got: {val}");
        assert!(val.contains("Whether the transfer succeeded"), "got: {val}");
    }

    #[test]
    fn test_hover_parameter_no_natspec() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    function add(uint256 a, uint256 b) public pure returns (uint256) {
        return a + b;
    }
}
"#;
        let offset = source.find("return a").unwrap() + 7;
        let pos = convert::offset_to_position(source, offset);
        let val = hover_value(source, pos).unwrap();
        assert!(val.contains("(parameter) uint256 a"), "got: {val}");
        assert!(
            !val.contains("---"),
            "should have no doc separator, got: {val}"
        );
    }

    #[test]
    fn test_hover_parameter_with_data_location() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    function process(bytes memory data) public pure returns (uint256) {
        return data.length;
    }
}
"#;
        let offset = source.find("data.length").unwrap();
        let pos = convert::offset_to_position(source, offset);
        let val = hover_value(source, pos).unwrap();
        assert!(val.contains("(parameter) bytes memory data"), "got: {val}");
    }

    #[test]
    fn test_hover_interface_shows_members() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

interface IERC20 {
    function totalSupply() external view returns (uint256);
    function balanceOf(address account) external view returns (uint256);
    event Transfer(address indexed from, address indexed to, uint256 value);
}
"#;
        let offset = source.find("IERC20").unwrap();
        let pos = convert::offset_to_position(source, offset);
        let val = hover_value(source, pos).unwrap();
        assert!(val.contains("interface IERC20 {"), "got: {val}");
        assert!(val.contains("function totalSupply()"), "got: {val}");
        assert!(val.contains("function balanceOf("), "got: {val}");
        assert!(val.contains("event Transfer("), "got: {val}");
        assert!(val.contains("}"), "got: {val}");
    }

    #[test]
    fn test_hover_empty_contract() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Empty {}
"#;
        let offset = source.find("Empty").unwrap();
        let pos = convert::offset_to_position(source, offset);
        let val = hover_value(source, pos).unwrap();
        assert!(val.contains("contract Empty"), "got: {val}");
    }

    #[test]
    fn test_hover_contract_with_mixed_members() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Token {
    uint256 public totalSupply;
    mapping(address => uint256) public balanceOf;
    function transfer(address to, uint256 amount) external returns (bool) {
        return true;
    }
    event Transfer(address from, address to, uint256 value);
}
"#;
        let offset = source.find("Token").unwrap();
        let pos = convert::offset_to_position(source, offset);
        let val = hover_value(source, pos).unwrap();
        assert!(val.contains("contract Token {"), "got: {val}");
        assert!(val.contains("uint256 public totalSupply"), "got: {val}");
        assert!(val.contains("function transfer("), "got: {val}");
        assert!(val.contains("event Transfer("), "got: {val}");
    }

    // -- Built-in hover tests --

    #[test]
    fn test_hover_builtin_keccak256() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    function hash(bytes memory data) public pure returns (bytes32) {
        return keccak256(data);
    }
}
"#;
        let offset = source.find("keccak256(data)").unwrap();
        let val = hover_value(source, convert::offset_to_position(source, offset)).unwrap();
        assert!(val.contains("keccak256"), "got: {val}");
        assert!(val.contains("bytes32"), "got: {val}");
        assert!(val.contains("Keccak-256"), "got: {val}");
    }

    #[test]
    fn test_hover_builtin_require() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    function foo() public pure {
        require(true);
    }
}
"#;
        let offset = source.find("require(true)").unwrap();
        let val = hover_value(source, convert::offset_to_position(source, offset)).unwrap();
        assert!(val.contains("require"), "got: {val}");
        assert!(val.contains("bool"), "got: {val}");
    }

    #[test]
    fn test_hover_builtin_msg_sender() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    function foo() public view returns (address) {
        return msg.sender;
    }
}
"#;
        // Hover on "sender" in "msg.sender"
        let offset = source.find("msg.sender").unwrap() + 4; // on 's' of 'sender'
        let val = hover_value(source, convert::offset_to_position(source, offset)).unwrap();
        assert!(val.contains("msg.sender"), "got: {val}");
        assert!(val.contains("address"), "got: {val}");
    }

    #[test]
    fn test_hover_builtin_msg_namespace() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    function foo() public view returns (address) {
        return msg.sender;
    }
}
"#;
        // Hover on "msg" itself
        let offset = source.find("msg.sender").unwrap(); // on 'm' of 'msg'
        let val = hover_value(source, convert::offset_to_position(source, offset)).unwrap();
        assert!(val.contains("sender"), "got: {val}");
        assert!(val.contains("value"), "got: {val}");
    }

    #[test]
    fn test_hover_builtin_abi_encode() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    function foo() public pure returns (bytes memory) {
        return abi.encode(uint256(1));
    }
}
"#;
        // Hover on "encode" in "abi.encode"
        let offset = source.find("abi.encode(").unwrap() + 4; // on 'e' of 'encode'
        let val = hover_value(source, convert::offset_to_position(source, offset)).unwrap();
        assert!(val.contains("abi.encode"), "got: {val}");
        assert!(val.contains("bytes memory"), "got: {val}");
    }

    #[test]
    fn test_hover_builtin_block_timestamp() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    function foo() public view returns (uint256) {
        return block.timestamp;
    }
}
"#;
        // Hover on "timestamp" in "block.timestamp"
        let offset = source.find("block.timestamp").unwrap() + 6; // on 't' of 'timestamp'
        let val = hover_value(source, convert::offset_to_position(source, offset)).unwrap();
        assert!(val.contains("block.timestamp"), "got: {val}");
        assert!(val.contains("uint256"), "got: {val}");
    }

    #[test]
    fn test_hover_yul_mload() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    function foo() public pure returns (uint256 result) {
        assembly {
            result := mload(0x40)
        }
    }
}
"#;
        let offset = source.find("mload(").unwrap();
        let val = hover_value(source, convert::offset_to_position(source, offset)).unwrap();
        assert!(val.contains("mload"), "got: {val}");
        assert!(val.contains("offset"), "got: {val}");
    }

    #[test]
    fn test_hover_yul_sstore() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    function foo() public {
        assembly {
            sstore(0, 1)
        }
    }
}
"#;
        let offset = source.find("sstore(").unwrap();
        let val = hover_value(source, convert::offset_to_position(source, offset)).unwrap();
        assert!(val.contains("sstore"), "got: {val}");
        assert!(val.contains("slot"), "got: {val}");
    }

    #[test]
    fn test_hover_user_defined_takes_precedence() {
        // A user-defined function named "gasleft" should shadow the builtin.
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    function gasleft() internal pure returns (uint256) {
        return 42;
    }
    function foo() public pure returns (uint256) {
        return gasleft();
    }
}
"#;
        // Hover on "gasleft" at the usage site
        let offset = source.find("return gasleft()").unwrap() + 7;
        let val = hover_value(source, convert::offset_to_position(source, offset)).unwrap();
        // Should show user-defined signature (internal pure), not the builtin
        assert!(val.contains("internal pure"), "got: {val}");
    }

    #[test]
    fn test_is_inside_assembly() {
        let source = r#"contract T {
    function f() public {
        uint256 x = 1;
        assembly {
            let y := mload(0x40)
        }
        uint256 z = 2;
    }
}"#;
        // Inside assembly block — on 'mload'
        let inside_offset = source.find("mload").unwrap();
        assert!(is_inside_assembly(source, inside_offset));

        // Outside assembly block — on 'z'
        let outside_offset = source.find("uint256 z").unwrap();
        assert!(!is_inside_assembly(source, outside_offset));

        // Before assembly block — on 'x'
        let before_offset = source.find("uint256 x").unwrap();
        assert!(!is_inside_assembly(source, before_offset));
    }
}
