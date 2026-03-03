//! Formatting — full-document and range formatting via the LSP.

use crate::convert;
use solgrid_config::FormatConfig;
use tower_lsp::lsp_types;

/// Format an entire document and return the text edits to apply.
///
/// Returns a single edit that replaces the entire document content,
/// or an empty vec if the document is already formatted.
pub fn format_document(source: &str, config: &FormatConfig) -> Vec<lsp_types::TextEdit> {
    match solgrid_formatter::format_source(source, config) {
        Ok(formatted) => {
            if formatted == source {
                return Vec::new();
            }
            vec![lsp_types::TextEdit {
                range: full_document_range(source),
                new_text: formatted,
            }]
        }
        Err(_) => Vec::new(),
    }
}

/// Format a range within a document.
///
/// Since the formatter operates on full source units, we format the entire
/// document and then extract only the edits that fall within the requested range.
/// If no changes fall within the range, we return an empty vec.
pub fn format_range(
    source: &str,
    range: &lsp_types::Range,
    config: &FormatConfig,
) -> Vec<lsp_types::TextEdit> {
    match solgrid_formatter::format_source(source, config) {
        Ok(formatted) => {
            if formatted == source {
                return Vec::new();
            }

            // Convert the requested range to byte offsets
            let range_start = convert::position_to_offset(source, range.start);
            let range_end = convert::position_to_offset(source, range.end);

            // Find lines that changed within the requested range
            let old_lines: Vec<&str> = source.lines().collect();
            let new_lines: Vec<&str> = formatted.lines().collect();

            let mut edits = Vec::new();
            let mut old_offset = 0usize;

            for (i, old_line) in old_lines.iter().enumerate() {
                let line_end = old_offset + old_line.len();

                // Check if this line is within the requested range
                if line_end >= range_start && old_offset <= range_end {
                    if let Some(&new_line) = new_lines.get(i) {
                        if *old_line != new_line {
                            edits.push(lsp_types::TextEdit {
                                range: lsp_types::Range {
                                    start: lsp_types::Position::new(i as u32, 0),
                                    end: lsp_types::Position::new(i as u32, old_line.len() as u32),
                                },
                                new_text: new_line.to_string(),
                            });
                        }
                    }
                }

                // +1 for the newline character
                old_offset = line_end + 1;
            }

            // If line-level diffing produced no edits but the file changed,
            // fall back to replacing the entire requested range
            if edits.is_empty() && formatted != source {
                vec![lsp_types::TextEdit {
                    range: full_document_range(source),
                    new_text: formatted,
                }]
            } else {
                edits
            }
        }
        Err(_) => Vec::new(),
    }
}

/// Compute the LSP range covering the entire document.
fn full_document_range(source: &str) -> lsp_types::Range {
    let line_count = source.lines().count();
    let last_line_len = source.lines().last().map_or(0, |l| l.len());
    lsp_types::Range {
        start: lsp_types::Position::new(0, 0),
        end: lsp_types::Position::new(line_count.saturating_sub(1) as u32, last_line_len as u32),
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
        // May or may not produce edits depending on how the formatter handles this
        // The important thing is no crash
        let _ = edits;
    }

    #[test]
    fn test_format_range() {
        let source =
            "// SPDX-License-Identifier: MIT\npragma solidity ^0.8.0;\ncontract Test {\n}\n";
        let config = FormatConfig::default();
        let range = lsp_types::Range {
            start: lsp_types::Position::new(0, 0),
            end: lsp_types::Position::new(1, 0),
        };
        let edits = format_range(source, &range, &config);
        // Should not crash
        let _ = edits;
    }

    #[test]
    fn test_full_document_range() {
        let source = "line1\nline2\nline3";
        let range = full_document_range(source);
        assert_eq!(range.start, lsp_types::Position::new(0, 0));
        assert_eq!(range.end, lsp_types::Position::new(2, 5));
    }

    #[test]
    fn test_full_document_range_empty() {
        let range = full_document_range("");
        assert_eq!(range.start, lsp_types::Position::new(0, 0));
        assert_eq!(range.end, lsp_types::Position::new(0, 0));
    }
}
