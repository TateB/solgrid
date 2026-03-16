//! Comment extraction from Solidity source text.
//!
//! Solar's AST includes doc comments (NatSpec) on nodes, but regular `//` and
//! `/* */` comments are not part of the AST. This module scans the raw source
//! to extract all comments with their byte positions, so the formatter can
//! reattach them to the appropriate AST nodes.

use crate::ir::CommentKind;
use std::ops::Range;

/// A comment extracted from source text.
#[derive(Debug, Clone)]
pub struct Comment {
    /// The kind of comment (line or block).
    pub kind: CommentKind,
    /// Byte range in the original source.
    pub range: Range<usize>,
    /// The comment content (without `//` or `/* */` delimiters).
    pub content: String,
    /// Whether this comment has been consumed by the formatter.
    pub consumed: bool,
}

/// Extract all comments from a Solidity source string.
pub fn extract_comments(source: &str) -> Vec<Comment> {
    let mut comments = Vec::new();
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        // Skip string literals
        if bytes[i] == b'"' || bytes[i] == b'\'' {
            let quote = bytes[i];
            i += 1;
            while i < len && bytes[i] != quote {
                if bytes[i] == b'\\' {
                    i += 1; // skip escaped char
                }
                i += 1;
            }
            if i < len {
                i += 1; // skip closing quote
            }
            continue;
        }

        if bytes[i] == b'/' && i + 1 < len {
            if bytes[i + 1] == b'/' {
                // Line comment — detect NatSpec `///` vs regular `//`
                let start = i;
                let is_doc = i + 2 < len && bytes[i + 2] == b'/';
                if is_doc {
                    i += 3; // skip ///
                } else {
                    i += 2; // skip //
                }
                let content_start = i;
                while i < len && bytes[i] != b'\n' {
                    i += 1;
                }
                let content = source[content_start..i].to_string();
                comments.push(Comment {
                    kind: if is_doc {
                        CommentKind::DocLine
                    } else {
                        CommentKind::Line
                    },
                    range: start..i,
                    content,
                    consumed: false,
                });
                continue;
            } else if bytes[i + 1] == b'*' {
                // Block comment
                let start = i;
                i += 2; // skip /*
                let content_start = i;
                while i + 1 < len && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                let content_end = i;
                if i + 1 < len {
                    i += 2; // skip */
                }
                let content = source[content_start..content_end].to_string();
                comments.push(Comment {
                    kind: CommentKind::Block,
                    range: start..i,
                    content,
                    consumed: false,
                });
                continue;
            }
        }

        i += 1;
    }

    comments
}

/// A store of comments that can be consumed by the formatter as it walks the AST.
#[derive(Debug)]
pub struct CommentStore {
    comments: Vec<Comment>,
}

impl CommentStore {
    pub fn new(source: &str) -> Self {
        Self {
            comments: extract_comments(source),
        }
    }

    /// Take all unconsumed comments whose range starts before `pos`.
    /// These are "leading" comments for the node at `pos`.
    pub fn take_leading(&mut self, pos: usize) -> Vec<Comment> {
        let mut result = Vec::new();
        for c in &mut self.comments {
            if !c.consumed && c.range.start < pos {
                c.consumed = true;
                result.push(c.clone());
            }
        }
        result
    }

    /// Take all unconsumed comments on the same line as `pos` that start after `pos`.
    /// These are "trailing" comments for the node ending at `pos`.
    pub fn take_trailing(&mut self, source: &str, pos: usize) -> Vec<Comment> {
        let mut result = Vec::new();
        let line_end = source[pos..].find('\n').map_or(source.len(), |i| pos + i);

        for c in &mut self.comments {
            if !c.consumed && c.range.start >= pos && c.range.start < line_end {
                c.consumed = true;
                result.push(c.clone());
            }
        }
        result
    }

    /// Take all remaining unconsumed comments.
    pub fn take_remaining(&mut self) -> Vec<Comment> {
        let mut result = Vec::new();
        for c in &mut self.comments {
            if !c.consumed {
                c.consumed = true;
                result.push(c.clone());
            }
        }
        result
    }

    /// Take unconsumed comments within a given byte range.
    pub fn take_within(&mut self, range: Range<usize>) -> Vec<Comment> {
        let mut result = Vec::new();
        for c in &mut self.comments {
            if !c.consumed && c.range.start >= range.start && c.range.end <= range.end {
                c.consumed = true;
                result.push(c.clone());
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_line_comment() {
        let source = "uint256 x; // a comment\nuint256 y;";
        let comments = extract_comments(source);
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].kind, CommentKind::Line);
        assert_eq!(comments[0].content, " a comment");
    }

    #[test]
    fn test_extract_block_comment() {
        let source = "uint256 x; /* block */ uint256 y;";
        let comments = extract_comments(source);
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].kind, CommentKind::Block);
        assert_eq!(comments[0].content, " block ");
    }

    #[test]
    fn test_skip_string_literals() {
        let source = r#"string s = "// not a comment"; // real comment"#;
        let comments = extract_comments(source);
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].content, " real comment");
    }

    #[test]
    fn test_multiple_comments() {
        let source = "// first\n// second\n/* third */";
        let comments = extract_comments(source);
        assert_eq!(comments.len(), 3);
    }

    #[test]
    fn test_comment_store_leading() {
        let source = "// leading\nuint256 x;";
        let mut store = CommentStore::new(source);
        let leading = store.take_leading(11); // position of "uint256"
        assert_eq!(leading.len(), 1);
        assert_eq!(leading[0].content, " leading");
    }

    #[test]
    fn test_comment_store_trailing() {
        let source = "uint256 x; // trailing\n";
        let mut store = CommentStore::new(source);
        let trailing = store.take_trailing(source, 10); // position after "uint256 x;"
        assert_eq!(trailing.len(), 1);
        assert_eq!(trailing[0].content, " trailing");
    }
}
