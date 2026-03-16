//! Shared source-text utilities for linter rules.
//!
//! Provides a state-machine scanner that classifies byte offsets as code,
//! comment, or string literal. Used by text-search-based rules to avoid
//! false positives on patterns found inside comments or strings.

use std::ops::Range;

/// The kind of source region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionKind {
    LineComment,
    BlockComment,
    DoubleString,
    SingleString,
}

/// A contiguous non-code region in the source text.
#[derive(Debug, Clone)]
pub struct SourceRegion {
    pub range: Range<usize>,
    pub kind: RegionKind,
}

/// Scan source code and return all non-code regions (comments and strings).
///
/// Processes the source left-to-right with a state machine that correctly
/// handles escape sequences, `//` inside strings, `/*` inside strings, etc.
/// The returned regions are sorted by start offset and non-overlapping.
pub fn scan_source_regions(source: &str) -> Vec<SourceRegion> {
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut regions = Vec::new();
    let mut i = 0;

    while i < len {
        match bytes[i] {
            b'/' if i + 1 < len && bytes[i + 1] == b'/' => {
                // Line comment: starts at `//`, ends at `\n` or EOF
                let start = i;
                i += 2;
                while i < len && bytes[i] != b'\n' {
                    i += 1;
                }
                regions.push(SourceRegion {
                    range: start..i,
                    kind: RegionKind::LineComment,
                });
                // Don't skip the `\n` — it's code (or EOF)
            }
            b'/' if i + 1 < len && bytes[i + 1] == b'*' => {
                // Block comment: starts at `/*`, ends at `*/` or EOF
                let start = i;
                i += 2;
                while i + 1 < len {
                    if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                        i += 2;
                        break;
                    }
                    i += 1;
                }
                // If we exited the loop without finding `*/`, advance past EOF
                if i < len && !(i >= 2 && bytes[i - 2] == b'*' && bytes[i - 1] == b'/') {
                    i = len;
                }
                regions.push(SourceRegion {
                    range: start..i,
                    kind: RegionKind::BlockComment,
                });
            }
            b'"' => {
                // Double-quoted string
                let start = i;
                i += 1;
                while i < len {
                    if bytes[i] == b'\\' {
                        i += 2; // skip escaped character
                    } else if bytes[i] == b'"' {
                        i += 1;
                        break;
                    } else {
                        i += 1;
                    }
                }
                regions.push(SourceRegion {
                    range: start..i,
                    kind: RegionKind::DoubleString,
                });
            }
            b'\'' => {
                // Single-quoted string
                let start = i;
                i += 1;
                while i < len {
                    if bytes[i] == b'\\' {
                        i += 2; // skip escaped character
                    } else if bytes[i] == b'\'' {
                        i += 1;
                        break;
                    } else {
                        i += 1;
                    }
                }
                regions.push(SourceRegion {
                    range: start..i,
                    kind: RegionKind::SingleString,
                });
            }
            _ => {
                i += 1;
            }
        }
    }

    regions
}

/// Check whether a byte offset falls inside any non-code region.
/// Uses binary search over the sorted, non-overlapping region list.
pub fn is_in_non_code_region(regions: &[SourceRegion], pos: usize) -> bool {
    regions
        .binary_search_by(|region| {
            if pos < region.range.start {
                std::cmp::Ordering::Greater
            } else if pos >= region.range.end {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

/// Check whether `pos` (a byte offset into `source`) is inside a comment
/// or string literal.
///
/// This is a convenience wrapper that scans the full source each time.
/// For repeated queries on the same source, prefer caching the result of
/// [`scan_source_regions`] and calling [`is_in_non_code_region`] directly.
pub fn is_in_comment_or_string(source: &str, pos: usize) -> bool {
    let regions = scan_source_regions(source);
    is_in_non_code_region(&regions, pos)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Original tests (preserved) ----

    #[test]
    fn test_not_in_comment() {
        let source = "uint256 x = tx.origin;";
        assert!(!is_in_comment_or_string(
            source,
            source.find("tx.origin").unwrap()
        ));
    }

    #[test]
    fn test_in_line_comment() {
        let source = "uint256 x; // tx.origin check";
        assert!(is_in_comment_or_string(
            source,
            source.find("tx.origin").unwrap()
        ));
    }

    #[test]
    fn test_in_block_comment() {
        let source = "/* tx.origin */ uint256 x;";
        assert!(is_in_comment_or_string(
            source,
            source.find("tx.origin").unwrap()
        ));
    }

    #[test]
    fn test_in_double_quoted_string() {
        let source = r#"string memory s = "tx.origin";"#;
        assert!(is_in_comment_or_string(
            source,
            source.find("tx.origin").unwrap()
        ));
    }

    #[test]
    fn test_in_single_quoted_string() {
        let source = "string memory s = 'tx.origin';";
        assert!(is_in_comment_or_string(
            source,
            source.find("tx.origin").unwrap()
        ));
    }

    #[test]
    fn test_after_block_comment() {
        let source = "/* comment */ tx.origin;";
        assert!(!is_in_comment_or_string(
            source,
            source.find("tx.origin").unwrap()
        ));
    }

    #[test]
    fn test_after_string() {
        let source = r#""hello" tx.origin;"#;
        assert!(!is_in_comment_or_string(
            source,
            source.find("tx.origin").unwrap()
        ));
    }

    #[test]
    fn test_multiline_before_line_comment() {
        let source = "uint256 x;\n// comment\ntx.origin;";
        // tx.origin is on its own line, not in a comment
        assert!(!is_in_comment_or_string(
            source,
            source.rfind("tx.origin").unwrap()
        ));
    }

    // ---- Edge case tests the old heuristic got wrong ----

    #[test]
    fn test_url_in_string_not_comment() {
        // The `//` inside the string should NOT be treated as a line comment
        let source = r#"string memory url = "http://example.com"; tx.origin;"#;
        assert!(!is_in_comment_or_string(
            source,
            source.find("tx.origin").unwrap()
        ));
    }

    #[test]
    fn test_escaped_double_quote() {
        // Escaped quote should not end the string
        let source = r#"string memory s = "he said \"hello\""; tx.origin;"#;
        assert!(!is_in_comment_or_string(
            source,
            source.find("tx.origin").unwrap()
        ));
    }

    #[test]
    fn test_escaped_single_quote() {
        let source = r"string memory s = 'it\'s'; tx.origin;";
        assert!(!is_in_comment_or_string(
            source,
            source.find("tx.origin").unwrap()
        ));
    }

    #[test]
    fn test_block_comment_markers_in_string() {
        // `/*` and `*/` inside a string should not start/end a block comment
        let source = r#"string memory s = "/* not a comment */"; tx.origin;"#;
        assert!(!is_in_comment_or_string(
            source,
            source.find("tx.origin").unwrap()
        ));
    }

    #[test]
    fn test_double_slash_in_string() {
        // `//` inside a string followed by code on the next line
        let source = "string memory s = \"http://foo\";\ntx.origin;";
        assert!(!is_in_comment_or_string(
            source,
            source.find("tx.origin").unwrap()
        ));
    }

    #[test]
    fn test_quote_in_block_comment() {
        // A `"` inside a block comment should not start a string
        let source = "/* he said \" */ tx.origin;";
        assert!(!is_in_comment_or_string(
            source,
            source.find("tx.origin").unwrap()
        ));
    }

    #[test]
    fn test_empty_strings() {
        let source = r#"""'' tx.origin;"#;
        assert!(!is_in_comment_or_string(
            source,
            source.find("tx.origin").unwrap()
        ));
    }

    #[test]
    fn test_unterminated_string_at_eof() {
        // If a string is never closed, the rest of the file is "inside" it
        let source = r#"string s = "unterminated tx.origin"#;
        assert!(is_in_comment_or_string(
            source,
            source.find("tx.origin").unwrap()
        ));
    }

    #[test]
    fn test_unterminated_block_comment() {
        let source = "/* unclosed block comment tx.origin";
        assert!(is_in_comment_or_string(
            source,
            source.find("tx.origin").unwrap()
        ));
    }

    #[test]
    fn test_multiline_block_comment() {
        let source = "/*\n * multi-line\n * tx.origin\n */ uint256 x;";
        assert!(is_in_comment_or_string(
            source,
            source.find("tx.origin").unwrap()
        ));
    }

    #[test]
    fn test_code_after_multiline_block_comment() {
        let source = "/*\n * multi-line\n */ tx.origin;";
        assert!(!is_in_comment_or_string(
            source,
            source.find("tx.origin").unwrap()
        ));
    }

    // ---- Scanner unit tests ----

    #[test]
    fn test_scan_empty_source() {
        assert!(scan_source_regions("").is_empty());
    }

    #[test]
    fn test_scan_no_special_regions() {
        let regions = scan_source_regions("uint256 x = 42;");
        assert!(regions.is_empty());
    }

    #[test]
    fn test_scan_line_comment_region() {
        let source = "code // comment\nmore code";
        let regions = scan_source_regions(source);
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].kind, RegionKind::LineComment);
        assert_eq!(&source[regions[0].range.clone()], "// comment");
    }

    #[test]
    fn test_scan_block_comment_region() {
        let source = "code /* block */ more";
        let regions = scan_source_regions(source);
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].kind, RegionKind::BlockComment);
        assert_eq!(&source[regions[0].range.clone()], "/* block */");
    }

    #[test]
    fn test_scan_double_string_region() {
        let source = r#"x = "hello" + y"#;
        let regions = scan_source_regions(source);
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].kind, RegionKind::DoubleString);
        assert_eq!(&source[regions[0].range.clone()], "\"hello\"");
    }

    #[test]
    fn test_scan_single_string_region() {
        let source = "x = 'hello' + y";
        let regions = scan_source_regions(source);
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].kind, RegionKind::SingleString);
        assert_eq!(&source[regions[0].range.clone()], "'hello'");
    }

    #[test]
    fn test_scan_escaped_quote_in_string() {
        let source = r#"x = "say \"hi\"" + y"#;
        let regions = scan_source_regions(source);
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].kind, RegionKind::DoubleString);
        assert_eq!(&source[regions[0].range.clone()], r#""say \"hi\"""#);
    }

    #[test]
    fn test_scan_mixed_regions() {
        let source = r#"uint x; // line comment
/* block */ string s = "hello";
'world'
"#;
        let regions = scan_source_regions(source);
        assert_eq!(regions.len(), 4);
        assert_eq!(regions[0].kind, RegionKind::LineComment);
        assert_eq!(regions[1].kind, RegionKind::BlockComment);
        assert_eq!(regions[2].kind, RegionKind::DoubleString);
        assert_eq!(regions[3].kind, RegionKind::SingleString);
    }
}
