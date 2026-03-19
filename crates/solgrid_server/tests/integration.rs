//! Integration tests for the solgrid LSP server modules.

use solgrid_config::Config;
use solgrid_linter::LintEngine;
use solgrid_server::{actions, completion, convert, diagnostics, format, hover, resolve};
use std::path::Path;
use tower_lsp_server::ls_types;

fn noop_uri() -> ls_types::Uri {
    "file:///test.sol".parse().unwrap()
}

fn noop_source(_path: &Path) -> Option<String> {
    None
}

fn noop_resolver() -> resolve::ImportResolver {
    resolve::ImportResolver::new(None)
}

/// Helper: create a standard test source with known issues.
fn source_with_issues() -> &'static str {
    r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function bad() public {
        require(tx.origin == msg.sender);
    }
}
"#
}

/// Helper: create a clean, minimal source.
fn clean_source() -> &'static str {
    r#"// SPDX-License-Identifier: MIT
pragma solidity 0.8.20;

contract Clean {
    uint256 public value;
}
"#
}

// =============================================================================
// Convert module integration tests
// =============================================================================

#[test]
fn test_offset_to_position_multiline() {
    let source = "pragma solidity ^0.8.0;\n\ncontract Test {\n    uint x;\n}\n";
    // "contract" starts at offset 25 (after "..;\n\n")
    let pos = convert::offset_to_position(source, 25);
    assert_eq!(pos, ls_types::Position::new(2, 0));

    // "uint" starts at offset 45 (line 3, col 4)
    let pos = convert::offset_to_position(source, 45);
    assert_eq!(pos, ls_types::Position::new(3, 4));
}

#[test]
fn test_offset_position_roundtrip_all_line_starts() {
    let source = "line0\nline1\nline2\nline3\n";
    let line_starts = [0, 6, 12, 18];
    for (line, &offset) in line_starts.iter().enumerate() {
        let pos = convert::offset_to_position(source, offset);
        assert_eq!(pos.line, line as u32, "line mismatch at offset {offset}");
        assert_eq!(pos.character, 0, "character mismatch at offset {offset}");
        let back = convert::position_to_offset(source, pos);
        assert_eq!(back, offset, "roundtrip failed at offset {offset}");
    }
}

#[test]
fn test_span_to_range_single_line() {
    let source = "hello world\ngoodbye world";
    let range = convert::span_to_range(source, &(6..11));
    assert_eq!(range.start, ls_types::Position::new(0, 6));
    assert_eq!(range.end, ls_types::Position::new(0, 11));
}

#[test]
fn test_span_to_range_multiline() {
    let source = "hello\nworld";
    let range = convert::span_to_range(source, &(0..11));
    assert_eq!(range.start, ls_types::Position::new(0, 0));
    assert_eq!(range.end, ls_types::Position::new(1, 5));
}

// =============================================================================
// Diagnostics module integration tests
// =============================================================================

#[test]
fn test_lint_detects_tx_origin() {
    let engine = LintEngine::new();
    let config = Config::default();
    let diags = diagnostics::lint_to_lsp_diagnostics(
        &engine,
        source_with_issues(),
        Path::new("test.sol"),
        &config,
    );

    let tx_origin_diags: Vec<_> = diags
        .iter()
        .filter(|d| {
            matches!(&d.code, Some(ls_types::NumberOrString::String(id)) if id == "security/tx-origin")
        })
        .collect();

    assert!(!tx_origin_diags.is_empty(), "should detect tx.origin usage");
}

#[test]
fn test_diagnostics_have_correct_structure() {
    let engine = LintEngine::new();
    let config = Config::default();
    let diags = diagnostics::lint_to_lsp_diagnostics(
        &engine,
        source_with_issues(),
        Path::new("test.sol"),
        &config,
    );

    for diag in &diags {
        assert_eq!(diag.source, Some("solgrid".into()));
        assert!(diag.severity.is_some());
        assert!(diag.code.is_some());
        // Range should be valid
        assert!(diag.range.start.line <= diag.range.end.line);
    }
}

// =============================================================================
// Code actions integration tests
// =============================================================================

#[test]
fn test_code_actions_full_file_range() {
    let engine = LintEngine::new();
    let config = Config::default();
    let uri = "file:///test.sol".parse::<ls_types::Uri>().unwrap();

    let full_range = ls_types::Range {
        start: ls_types::Position::new(0, 0),
        end: ls_types::Position::new(100, 0),
    };

    let result = actions::code_actions(
        &engine,
        source_with_issues(),
        Path::new("test.sol"),
        &config,
        &full_range,
        &uri,
    );

    // Code actions should be valid — each must have a non-empty title
    for action in &result {
        match action {
            ls_types::CodeActionOrCommand::CodeAction(ca) => {
                assert!(!ca.title.is_empty(), "code action should have a title");
            }
            ls_types::CodeActionOrCommand::Command(cmd) => {
                assert!(!cmd.title.is_empty(), "command should have a title");
            }
        }
    }
}

#[test]
fn test_safe_fix_edits() {
    let engine = LintEngine::new();
    let config = Config::default();

    let edits = actions::safe_fix_edits(
        &engine,
        source_with_issues(),
        Path::new("test.sol"),
        &config,
    );

    // The edits should be valid LSP text edits
    for edit in &edits {
        assert!(edit.range.start.line <= edit.range.end.line);
    }
}

// =============================================================================
// Format module integration tests
// =============================================================================

#[test]
fn test_format_preserves_valid_solidity() {
    let source = clean_source();
    let config = solgrid_config::FormatConfig::default();
    let edits = format::format_document(source, &config);

    // Formatting should either produce no edits (already formatted)
    // or valid edits that don't crash
    for edit in &edits {
        assert!(edit.range.start.line <= edit.range.end.line);
    }
}

#[test]
fn test_format_range_subset() {
    let source = clean_source();
    let config = solgrid_config::FormatConfig::default();

    // Format only the first line
    let range = ls_types::Range {
        start: ls_types::Position::new(0, 0),
        end: ls_types::Position::new(1, 0),
    };

    let edits = format::format_range(source, &range, &config);
    // Edits should have valid ranges
    for edit in &edits {
        assert!(edit.range.start.line <= edit.range.end.line);
    }
}

// =============================================================================
// Hover module integration tests
// =============================================================================

#[test]
fn test_hover_shows_rule_docs_for_known_rule() {
    let engine = LintEngine::new();

    let diags = vec![ls_types::Diagnostic {
        range: ls_types::Range {
            start: ls_types::Position::new(4, 8),
            end: ls_types::Position::new(4, 40),
        },
        severity: Some(ls_types::DiagnosticSeverity::ERROR),
        code: Some(ls_types::NumberOrString::String(
            "security/tx-origin".into(),
        )),
        source: Some("solgrid".into()),
        message: "Avoid using tx.origin".into(),
        ..Default::default()
    }];

    let hover = hover::hover_at_position(
        &engine,
        &diags,
        &ls_types::Position::new(4, 20),
        "",
        &noop_uri(),
        &noop_source,
        &noop_resolver(),
    );
    assert!(hover.is_some());

    let hover = hover.unwrap();
    match &hover.contents {
        ls_types::HoverContents::Markup(markup) => {
            assert_eq!(markup.kind, ls_types::MarkupKind::Markdown);
            assert!(markup.value.contains("tx-origin"));
            assert!(markup.value.contains("security"));
            assert!(markup.value.contains("solgrid-disable-next-line"));
        }
        _ => panic!("expected markup content"),
    }
}

#[test]
fn test_hover_returns_none_for_non_diagnostic_position() {
    let engine = LintEngine::new();

    let diags = vec![ls_types::Diagnostic {
        range: ls_types::Range {
            start: ls_types::Position::new(4, 8),
            end: ls_types::Position::new(4, 40),
        },
        severity: Some(ls_types::DiagnosticSeverity::ERROR),
        code: Some(ls_types::NumberOrString::String(
            "security/tx-origin".into(),
        )),
        source: Some("solgrid".into()),
        message: "Avoid using tx.origin".into(),
        ..Default::default()
    }];

    // Position on a different line — should return None
    let hover = hover::hover_at_position(
        &engine,
        &diags,
        &ls_types::Position::new(0, 0),
        "",
        &noop_uri(),
        &noop_source,
        &noop_resolver(),
    );
    assert!(hover.is_none());
}

// =============================================================================
// Completion module integration tests
// =============================================================================

#[test]
fn test_completion_after_disable_directive() {
    let engine = LintEngine::new();
    let source = "// solgrid-disable-next-line \ncontract Test {}\n";
    let position = ls_types::Position::new(0, 29);

    let items = completion::suppression_completions(&engine, source, &position);
    assert!(!items.is_empty(), "should suggest rule IDs after directive");

    // Should contain known rules
    let rule_labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
    assert!(rule_labels.contains(&"security/tx-origin"));
    assert!(rule_labels.contains(&"naming/contract-name-capwords"));
}

#[test]
fn test_completion_typing_solgrid_comment() {
    let engine = LintEngine::new();
    let source = "// sol\ncontract Test {}\n";
    let position = ls_types::Position::new(0, 6);

    let items = completion::suppression_completions(&engine, source, &position);
    assert!(
        !items.is_empty(),
        "should suggest directives when typing // sol"
    );

    let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
    assert!(labels.contains(&"solgrid-disable-next-line"));
    assert!(labels.contains(&"solgrid-disable"));
    assert!(labels.contains(&"solgrid-enable"));
}

#[test]
fn test_completion_not_in_code() {
    let engine = LintEngine::new();
    let source = "contract Test { uint x; }\n";
    let position = ls_types::Position::new(0, 10);

    let items = completion::suppression_completions(&engine, source, &position);
    assert!(items.is_empty(), "should not suggest completions in code");
}

// =============================================================================
// End-to-end scenario tests
// =============================================================================

#[test]
fn test_full_lint_format_cycle() {
    let engine = LintEngine::new();
    let config = Config::default();

    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function bad() public {
        require(tx.origin == msg.sender);
    }
}
"#;

    // Step 1: Lint and get diagnostics
    let diags =
        diagnostics::lint_to_lsp_diagnostics(&engine, source, Path::new("test.sol"), &config);
    assert!(!diags.is_empty());

    // Step 2: Get code actions
    let uri = "file:///test.sol".parse::<ls_types::Uri>().unwrap();
    let full_range = ls_types::Range {
        start: ls_types::Position::new(0, 0),
        end: ls_types::Position::new(100, 0),
    };
    let code_actions = actions::code_actions(
        &engine,
        source,
        Path::new("test.sol"),
        &config,
        &full_range,
        &uri,
    );
    for action in &code_actions {
        match action {
            ls_types::CodeActionOrCommand::CodeAction(ca) => {
                assert!(!ca.title.is_empty(), "code action should have a title");
            }
            ls_types::CodeActionOrCommand::Command(cmd) => {
                assert!(!cmd.title.is_empty(), "command should have a title");
            }
        }
    }

    // Step 3: Format — should produce valid edits
    let format_edits = format::format_document(source, &config.format);
    for edit in &format_edits {
        assert!(edit.range.start.line <= edit.range.end.line);
        assert!(!edit.new_text.is_empty(), "format edit should have content");
    }

    // Step 4: Hover over a diagnostic — should find the tx.origin diagnostic
    let tx_diag = diags.iter().find(|d| {
        matches!(&d.code, Some(ls_types::NumberOrString::String(id)) if id == "security/tx-origin")
    });
    assert!(tx_diag.is_some(), "should have found tx-origin diagnostic");
    let tx_diag = tx_diag.unwrap();
    let hover_pos =
        ls_types::Position::new(tx_diag.range.start.line, tx_diag.range.start.character + 1);
    let hover = hover::hover_at_position(
        &engine,
        &diags,
        &hover_pos,
        "",
        &noop_uri(),
        &noop_source,
        &noop_resolver(),
    );
    assert!(
        hover.is_some(),
        "should find hover info for tx-origin diagnostic"
    );
}
