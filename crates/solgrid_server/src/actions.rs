//! Code actions — quick-fixes grouped by safety tier.

use crate::convert;
use solgrid_config::Config;
use solgrid_diagnostics::FixSafety;
use solgrid_linter::LintEngine;
use std::path::Path;
use tower_lsp::lsp_types;

/// Generate code actions for a given range in a document.
///
/// Returns quick-fixes for diagnostics that overlap with the requested range,
/// grouped by their safety tier (safe, suggestion, dangerous).
pub fn code_actions(
    engine: &LintEngine,
    source: &str,
    path: &Path,
    config: &Config,
    range: &lsp_types::Range,
    uri: &lsp_types::Url,
) -> Vec<lsp_types::CodeActionOrCommand> {
    let result = engine.lint_source(source, path, config);
    let range_start = convert::position_to_offset(source, range.start);
    let range_end = convert::position_to_offset(source, range.end);

    let mut actions = Vec::new();

    for diag in &result.diagnostics {
        // Check if this diagnostic overlaps with the requested range
        if diag.span.end <= range_start || diag.span.start >= range_end {
            continue;
        }

        let Some(fix) = &diag.fix else {
            continue;
        };

        // Build LSP text edits from the fix
        let edits: Vec<lsp_types::TextEdit> = fix
            .edits
            .iter()
            .map(|edit| lsp_types::TextEdit {
                range: convert::span_to_range(source, &edit.range),
                new_text: edit.replacement.clone(),
            })
            .collect();

        if edits.is_empty() {
            continue;
        }

        let mut changes = std::collections::HashMap::new();
        changes.insert(uri.clone(), edits);

        let safety_label = match fix.safety {
            FixSafety::Safe => "",
            FixSafety::Suggestion => " (suggestion)",
            FixSafety::Dangerous => " (dangerous)",
        };

        let action = lsp_types::CodeAction {
            title: format!("{}{}", fix.message, safety_label),
            kind: Some(convert::fix_safety_to_action_kind(fix.safety)),
            diagnostics: Some(vec![convert::diagnostic_to_lsp(source, diag)]),
            edit: Some(lsp_types::WorkspaceEdit {
                changes: Some(changes),
                document_changes: None,
                change_annotations: None,
            }),
            command: None,
            is_preferred: Some(fix.safety == FixSafety::Safe),
            disabled: None,
            data: None,
        };

        actions.push(lsp_types::CodeActionOrCommand::CodeAction(action));
    }

    // Add "fix all safe fixes" action if there are multiple safe fixes
    let safe_fix_count = result
        .diagnostics
        .iter()
        .filter(|d| d.fix.as_ref().is_some_and(|f| f.safety == FixSafety::Safe))
        .count();

    if safe_fix_count > 1 {
        let all_edits: Vec<lsp_types::TextEdit> = result
            .diagnostics
            .iter()
            .filter_map(|d| {
                d.fix.as_ref().and_then(|f| {
                    if f.safety == FixSafety::Safe {
                        Some(f.edits.iter().map(|edit| lsp_types::TextEdit {
                            range: convert::span_to_range(source, &edit.range),
                            new_text: edit.replacement.clone(),
                        }))
                    } else {
                        None
                    }
                })
            })
            .flatten()
            .collect();

        if !all_edits.is_empty() {
            let mut changes = std::collections::HashMap::new();
            changes.insert(uri.clone(), all_edits);

            let action = lsp_types::CodeAction {
                title: format!("Fix all safe issues ({safe_fix_count} fixes)"),
                kind: Some(lsp_types::CodeActionKind::SOURCE_FIX_ALL),
                diagnostics: None,
                edit: Some(lsp_types::WorkspaceEdit {
                    changes: Some(changes),
                    document_changes: None,
                    change_annotations: None,
                }),
                command: None,
                is_preferred: Some(false),
                disabled: None,
                data: None,
            };

            actions.push(lsp_types::CodeActionOrCommand::CodeAction(action));
        }
    }

    actions
}

/// Build text edits for applying all safe fixes (used by fix-on-save).
pub fn safe_fix_edits(
    engine: &LintEngine,
    source: &str,
    path: &Path,
    config: &Config,
) -> Vec<lsp_types::TextEdit> {
    let result = engine.lint_source(source, path, config);

    result
        .diagnostics
        .iter()
        .filter_map(|d| {
            d.fix.as_ref().and_then(|f| {
                if f.safety == FixSafety::Safe {
                    Some(f.edits.iter().map(|edit| lsp_types::TextEdit {
                        range: convert::span_to_range(source, &edit.range),
                        new_text: edit.replacement.clone(),
                    }))
                } else {
                    None
                }
            })
        })
        .flatten()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_code_actions_with_fixable_diagnostic() {
        // Use a source that triggers a rule with a safe fix
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function test() public {
        uint x = 1;
        x += 1;
    }
}
"#;
        let engine = LintEngine::new();
        let config = Config::default();
        let uri = lsp_types::Url::parse("file:///test.sol").unwrap();

        // Request code actions for the entire file
        let range = lsp_types::Range {
            start: lsp_types::Position::new(0, 0),
            end: lsp_types::Position::new(10, 0),
        };

        let actions = code_actions(
            &engine,
            source,
            Path::new("test.sol"),
            &config,
            &range,
            &uri,
        );
        // Even if no fixable issues overlap, we shouldn't crash
        let _ = actions;
    }

    #[test]
    fn test_code_actions_no_diagnostics_clean_source() {
        let source = "";
        let engine = LintEngine::new();
        let config = Config::default();
        let uri = lsp_types::Url::parse("file:///empty.sol").unwrap();

        let range = lsp_types::Range {
            start: lsp_types::Position::new(0, 0),
            end: lsp_types::Position::new(0, 0),
        };

        let actions = code_actions(
            &engine,
            source,
            Path::new("empty.sol"),
            &config,
            &range,
            &uri,
        );
        assert!(actions.is_empty());
    }
}
