//! Completion — suppression comment completions.

use solgrid_linter::LintEngine;
use tower_lsp_server::ls_types;

/// Suppression comment prefixes that we complete.
const SUPPRESSION_PREFIXES: &[&str] = &[
    "// solgrid-disable-next-line",
    "// solgrid-disable-line",
    "// solgrid-disable",
    "// solgrid-enable",
];

/// Generate completion items for suppression comments.
///
/// Triggered when the user is typing a comment that looks like a suppression directive.
pub fn suppression_completions(
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

    // Check if we're in a suppression comment context
    if prefix.starts_with("// solgrid-") || prefix.starts_with("// solhint-") {
        // Check if we're after the directive keyword (e.g., "// solgrid-disable-next-line ")
        for suppression_prefix in SUPPRESSION_PREFIXES {
            if prefix.starts_with(suppression_prefix) {
                // We're after the directive — complete with rule IDs
                return rule_id_completions(engine);
            }
        }

        // We're still typing the directive — complete with directive names
        return directive_completions();
    }

    // Check if the user just typed "// sol"
    if prefix.starts_with("// sol") || prefix == "//" || prefix == "// " {
        return directive_completions();
    }

    Vec::new()
}

/// Generate completions for suppression directive names.
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

/// Generate completions for rule IDs.
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

/// Get the text of a specific line (0-based).
fn get_line_text(source: &str, line: usize) -> Option<&str> {
    source.lines().nth(line)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        // Should contain well-known rules
        assert!(items.iter().any(|i| i.label == "security/tx-origin"));
    }

    #[test]
    fn test_suppression_completions_after_directive() {
        let engine = LintEngine::new();
        let source = "// solgrid-disable-next-line \n";
        let position = ls_types::Position::new(0, 29);
        let items = suppression_completions(&engine, source, &position);
        // Should return rule ID completions
        assert!(!items.is_empty());
        assert!(items.iter().any(|i| i.label == "security/tx-origin"));
    }

    #[test]
    fn test_suppression_completions_typing_directive() {
        let engine = LintEngine::new();
        let source = "// sol\n";
        let position = ls_types::Position::new(0, 6);
        let items = suppression_completions(&engine, source, &position);
        // Should return directive completions
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
}
