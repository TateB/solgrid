//! Formatting — full-document and range formatting via the LSP.

use solgrid_config::FormatConfig;
use tower_lsp_server::ls_types;

/// Format an entire document and return the text edits to apply.
///
/// Returns a single edit that replaces the entire document content,
/// or an empty vec if the document is already formatted.
pub fn format_document(source: &str, config: &FormatConfig) -> Vec<ls_types::TextEdit> {
    match solgrid_formatter::format_source(source, config) {
        Ok(formatted) => {
            if formatted == source {
                return Vec::new();
            }
            vec![ls_types::TextEdit {
                range: full_document_range(source),
                new_text: formatted,
            }]
        }
        Err(_) => Vec::new(),
    }
}

/// Format a range within a document.
///
/// Since the formatter operates on full source units (it needs the complete
/// AST), we format the entire document and replace it wholesale. The LSP
/// client will apply the minimal diff. Attempting to do a line-by-line diff
/// here is unreliable when formatting changes the number of lines.
pub fn format_range(
    source: &str,
    _range: &ls_types::Range,
    config: &FormatConfig,
) -> Vec<ls_types::TextEdit> {
    // Format the whole document — the formatter needs full context.
    format_document(source, config)
}

/// Compute the LSP range covering the entire document.
fn full_document_range(source: &str) -> ls_types::Range {
    let line_count = source.lines().count();
    let last_line_len = source.lines().last().map_or(0, |l| l.len());
    ls_types::Range {
        start: ls_types::Position::new(0, 0),
        end: ls_types::Position::new(line_count.saturating_sub(1) as u32, last_line_len as u32),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solgrid_config::FormatConfig;

    #[test]
    fn test_format_document_already_formatted() {
        // Format a well-formatted source — should return no edits
        let source = "// SPDX-License-Identifier: MIT\npragma solidity ^0.8.0;\n\ncontract Test {\n    uint256 public x;\n}\n";
        let config = FormatConfig::default();
        // First format to get the canonical form
        let canonical =
            solgrid_formatter::format_source(source, &config).unwrap_or(source.to_string());
        // Formatting the canonical form should produce no edits
        let edits = format_document(&canonical, &config);
        assert!(
            edits.is_empty(),
            "formatting canonical source should produce no edits"
        );
    }

    #[test]
    fn test_format_document_with_changes() {
        let source =
            "// SPDX-License-Identifier: MIT\npragma solidity ^0.8.0;\ncontract    Test  {\n}\n";
        let config = FormatConfig::default();
        let edits = format_document(source, &config);
        // Source has extra spaces — formatting should produce edits
        assert!(
            !edits.is_empty(),
            "source with extra whitespace should produce format edits"
        );
        // The edit should contain the cleaned-up contract name
        assert!(
            edits[0].new_text.contains("contract Test {"),
            "formatted output should normalize whitespace in contract declaration"
        );
    }

    #[test]
    fn test_format_range() {
        let source =
            "// SPDX-License-Identifier: MIT\npragma solidity ^0.8.0;\ncontract Test {\n}\n";
        let config = FormatConfig::default();
        let range = ls_types::Range {
            start: ls_types::Position::new(0, 0),
            end: ls_types::Position::new(1, 0),
        };
        let edits = format_range(source, &range, &config);
        // Edits should have valid ranges
        for edit in &edits {
            assert!(edit.range.start.line <= edit.range.end.line);
        }
    }

    #[test]
    fn test_full_document_range() {
        let source = "line1\nline2\nline3";
        let range = full_document_range(source);
        assert_eq!(range.start, ls_types::Position::new(0, 0));
        assert_eq!(range.end, ls_types::Position::new(2, 5));
    }

    #[test]
    fn test_full_document_range_empty() {
        let range = full_document_range("");
        assert_eq!(range.start, ls_types::Position::new(0, 0));
        assert_eq!(range.end, ls_types::Position::new(0, 0));
    }
}
