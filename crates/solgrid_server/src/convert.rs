//! Type conversion between solgrid diagnostics and LSP types.

use solgrid_diagnostics::{Diagnostic, FixSafety, Severity};
use tower_lsp::lsp_types;

/// Convert a byte offset to an LSP Position (0-based line and character).
///
/// The LSP spec uses UTF-16 character offsets, but for Solidity (which is
/// predominantly ASCII), byte offsets and UTF-16 offsets are typically identical.
/// We handle multi-byte UTF-8 characters correctly by counting UTF-16 code units.
pub fn offset_to_position(source: &str, offset: usize) -> lsp_types::Position {
    let offset = offset.min(source.len());
    let mut line = 0u32;
    let mut character = 0u32;

    for (i, ch) in source.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            character = 0;
        } else {
            character += ch.len_utf16() as u32;
        }
    }

    lsp_types::Position { line, character }
}

/// Convert an LSP Position back to a byte offset in the source.
pub fn position_to_offset(source: &str, position: lsp_types::Position) -> usize {
    let mut current_line = 0u32;
    let mut current_char = 0u32;

    for (i, ch) in source.char_indices() {
        if current_line == position.line && current_char == position.character {
            return i;
        }
        if ch == '\n' {
            if current_line == position.line {
                // Position is past end of line — return the newline position
                return i;
            }
            current_line += 1;
            current_char = 0;
        } else {
            current_char += ch.len_utf16() as u32;
        }
    }

    source.len()
}

/// Convert a byte range to an LSP Range.
pub fn span_to_range(source: &str, span: &std::ops::Range<usize>) -> lsp_types::Range {
    lsp_types::Range {
        start: offset_to_position(source, span.start),
        end: offset_to_position(source, span.end),
    }
}

/// Convert a solgrid Severity to an LSP DiagnosticSeverity.
pub fn severity_to_lsp(severity: Severity) -> lsp_types::DiagnosticSeverity {
    match severity {
        Severity::Error => lsp_types::DiagnosticSeverity::ERROR,
        Severity::Warning => lsp_types::DiagnosticSeverity::WARNING,
        Severity::Info => lsp_types::DiagnosticSeverity::INFORMATION,
    }
}

/// Convert a solgrid Diagnostic to an LSP Diagnostic.
pub fn diagnostic_to_lsp(source_text: &str, diag: &Diagnostic) -> lsp_types::Diagnostic {
    lsp_types::Diagnostic {
        range: span_to_range(source_text, &diag.span),
        severity: Some(severity_to_lsp(diag.severity)),
        code: Some(lsp_types::NumberOrString::String(diag.rule_id.clone())),
        code_description: None,
        source: Some("solgrid".into()),
        message: diag.message.clone(),
        related_information: None,
        tags: None,
        data: None,
    }
}

/// Convert a FixSafety to a CodeActionKind.
pub fn fix_safety_to_action_kind(safety: FixSafety) -> lsp_types::CodeActionKind {
    match safety {
        FixSafety::Safe => lsp_types::CodeActionKind::QUICKFIX,
        FixSafety::Suggestion => lsp_types::CodeActionKind::REFACTOR,
        FixSafety::Dangerous => lsp_types::CodeActionKind::REFACTOR_REWRITE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_offset_to_position_simple() {
        let source = "line one\nline two\nline three";
        // Start of file
        assert_eq!(
            offset_to_position(source, 0),
            lsp_types::Position {
                line: 0,
                character: 0
            }
        );
        // Start of "line two"
        assert_eq!(
            offset_to_position(source, 9),
            lsp_types::Position {
                line: 1,
                character: 0
            }
        );
        // "two" in "line two"
        assert_eq!(
            offset_to_position(source, 14),
            lsp_types::Position {
                line: 1,
                character: 5
            }
        );
    }

    #[test]
    fn test_position_to_offset_simple() {
        let source = "line one\nline two\nline three";
        assert_eq!(
            position_to_offset(source, lsp_types::Position::new(0, 0)),
            0
        );
        assert_eq!(
            position_to_offset(source, lsp_types::Position::new(1, 0)),
            9
        );
        assert_eq!(
            position_to_offset(source, lsp_types::Position::new(1, 5)),
            14
        );
    }

    #[test]
    fn test_offset_position_roundtrip() {
        let source = "pragma solidity ^0.8.0;\n\ncontract Test {\n    uint x;\n}\n";
        for offset in [0, 5, 23, 24, 25, 40, 50] {
            if offset <= source.len() {
                let pos = offset_to_position(source, offset);
                let back = position_to_offset(source, pos);
                assert_eq!(back, offset, "roundtrip failed at offset {offset}");
            }
        }
    }

    #[test]
    fn test_span_to_range() {
        let source = "line one\nline two";
        let range = span_to_range(source, &(9..17));
        assert_eq!(range.start, lsp_types::Position::new(1, 0));
        assert_eq!(range.end, lsp_types::Position::new(1, 8));
    }

    #[test]
    fn test_severity_to_lsp() {
        assert_eq!(
            severity_to_lsp(Severity::Error),
            lsp_types::DiagnosticSeverity::ERROR
        );
        assert_eq!(
            severity_to_lsp(Severity::Warning),
            lsp_types::DiagnosticSeverity::WARNING
        );
        assert_eq!(
            severity_to_lsp(Severity::Info),
            lsp_types::DiagnosticSeverity::INFORMATION
        );
    }

    #[test]
    fn test_diagnostic_to_lsp() {
        let source = "line one\nline two";
        let diag = Diagnostic::new("test/rule", "test message", Severity::Warning, 9..17);
        let lsp_diag = diagnostic_to_lsp(source, &diag);

        assert_eq!(lsp_diag.range.start, lsp_types::Position::new(1, 0));
        assert_eq!(lsp_diag.range.end, lsp_types::Position::new(1, 8));
        assert_eq!(
            lsp_diag.severity,
            Some(lsp_types::DiagnosticSeverity::WARNING)
        );
        assert_eq!(
            lsp_diag.code,
            Some(lsp_types::NumberOrString::String("test/rule".into()))
        );
        assert_eq!(lsp_diag.source, Some("solgrid".into()));
        assert_eq!(lsp_diag.message, "test message");
    }
}
