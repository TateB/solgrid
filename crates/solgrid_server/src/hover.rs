//! Hover — rule documentation on diagnostic hover.

use solgrid_diagnostics::{FixAvailability, RuleMeta};
use solgrid_linter::LintEngine;
use tower_lsp_server::ls_types;

/// Generate hover information for a position in a document.
///
/// If the position overlaps with a diagnostic, shows the rule documentation.
pub fn hover_for_diagnostic(
    engine: &LintEngine,
    lsp_diagnostics: &[ls_types::Diagnostic],
    position: &ls_types::Position,
) -> Option<ls_types::Hover> {
    // Find diagnostics at this position
    for diag in lsp_diagnostics {
        if !position_in_range(position, &diag.range) {
            continue;
        }

        // Extract rule ID from the diagnostic code
        let rule_id = match &diag.code {
            Some(ls_types::NumberOrString::String(id)) => id.as_str(),
            _ => continue,
        };

        // Look up the rule documentation
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
