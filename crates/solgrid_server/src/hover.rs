//! Hover — rule documentation and code intelligence on hover.

use crate::convert;
use crate::definition;
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

fn hover_for_diagnostic(
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

fn hover_for_symbol(source: &str, position: &ls_types::Position) -> Option<ls_types::Hover> {
    let offset = convert::position_to_offset(source, *position);
    let resolved = definition::find_definition(source, offset)?;

    let signature = extract_signature(source, &resolved);
    let natspec = extract_natspec(source, resolved.item_range.start);

    let mut content = String::new();
    if let Some(sig) = &signature {
        content.push_str("```solidity\n");
        content.push_str(sig);
        content.push_str("\n```\n");
    }
    if let Some(doc) = &natspec {
        if !content.is_empty() {
            content.push_str("\n---\n\n");
        }
        content.push_str(&format_natspec(doc));
    }

    if content.is_empty() {
        return None;
    }

    let start = convert::offset_to_position(source, resolved.name_range.start);
    let end = convert::offset_to_position(source, resolved.name_range.end);

    Some(ls_types::Hover {
        contents: ls_types::HoverContents::Markup(ls_types::MarkupContent {
            kind: ls_types::MarkupKind::Markdown,
            value: content,
        }),
        range: Some(ls_types::Range { start, end }),
    })
}

/// Extract the signature of an item (text up to `{` or `;`).
fn extract_signature(source: &str, resolved: &definition::ResolvedSymbol) -> Option<String> {
    let item_text = source.get(resolved.item_range.clone())?;
    // Find the first `{` or `;` to get just the signature
    let end = item_text
        .find('{')
        .or_else(|| item_text.find(';'))
        .unwrap_or(item_text.len());
    let sig = item_text[..end].trim();
    if sig.is_empty() {
        return None;
    }
    Some(sig.to_string())
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

/// Format NatSpec into readable markdown.
fn format_natspec(natspec: &str) -> String {
    let mut lines = Vec::new();
    for line in natspec.lines() {
        let trimmed = line.trim();
        // Strip comment markers
        let content = if let Some(rest) = trimmed.strip_prefix("///") {
            rest.trim()
        } else if let Some(rest) = trimmed.strip_prefix("/**") {
            rest.trim()
        } else if trimmed.starts_with("*/") {
            continue;
        } else if let Some(rest) = trimmed.strip_prefix("* ") {
            rest.trim()
        } else if let Some(rest) = trimmed.strip_prefix('*') {
            rest.trim()
        } else {
            trimmed
        };

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
        } else if !content.is_empty() {
            lines.push(content.to_string());
        }
    }
    lines.join("\n\n")
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
}
