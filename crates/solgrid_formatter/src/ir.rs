//! Chunk-based format intermediate representation.
//!
//! Inspired by Wadler-Lindig / Prettier-style document IR.
//! The formatter converts the Solar AST into a tree of `FormatChunk`s,
//! which the printer then renders to a string with line fitting.

/// A chunk of formatted output.
#[derive(Debug, Clone)]
pub enum FormatChunk {
    /// Literal text (no newlines).
    Text(String),
    /// A soft line break: renders as a space when the enclosing Group fits
    /// on one line, or as a newline + indent when it breaks.
    Line,
    /// A soft line break that renders as empty string (no space) when flat.
    Softline,
    /// A hard line break: always renders as a newline.
    HardLine,
    /// A group of chunks that the printer tries to fit on a single line.
    /// If the flat width exceeds the remaining line budget, the group breaks.
    Group(Vec<FormatChunk>),
    /// Increase the indent level for the enclosed chunks.
    Indent(Vec<FormatChunk>),
    /// A sequence of chunks (no grouping semantics).
    Concat(Vec<FormatChunk>),
    /// A comment to be emitted verbatim.
    Comment(CommentKind, String),
    /// Content that differs based on whether the enclosing group is flat or broken.
    /// (flat_content, break_content)
    IfFlat(Box<FormatChunk>, Box<FormatChunk>),
}

/// The kind of a source comment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommentKind {
    /// `// ...`
    Line,
    /// `/// ...` (NatSpec doc comment)
    DocLine,
    /// `/* ... */`
    Block,
}

// Builder helpers for constructing IR trees ergonomically.

/// Create a `Text` chunk.
pub fn text(s: impl Into<String>) -> FormatChunk {
    FormatChunk::Text(s.into())
}

/// Create a `Line` chunk (soft line break — space when flat, newline when broken).
pub fn line() -> FormatChunk {
    FormatChunk::Line
}

/// Create a `Softline` chunk (empty when flat, newline when broken).
pub fn softline() -> FormatChunk {
    FormatChunk::Softline
}

/// Create a `HardLine` chunk (always newline).
pub fn hardline() -> FormatChunk {
    FormatChunk::HardLine
}

/// Create a `Group` chunk that tries to fit its children on one line.
pub fn group(children: Vec<FormatChunk>) -> FormatChunk {
    FormatChunk::Group(children)
}

/// Create an `Indent` chunk that increases the indent level.
pub fn indent(children: Vec<FormatChunk>) -> FormatChunk {
    FormatChunk::Indent(children)
}

/// Create a `Concat` chunk — a flat sequence with no grouping.
pub fn concat(children: Vec<FormatChunk>) -> FormatChunk {
    FormatChunk::Concat(children)
}

/// Create an `IfFlat` chunk.
pub fn if_flat(flat: FormatChunk, broken: FormatChunk) -> FormatChunk {
    FormatChunk::IfFlat(Box::new(flat), Box::new(broken))
}

/// Join chunks with a separator.
pub fn join(chunks: Vec<FormatChunk>, separator: FormatChunk) -> FormatChunk {
    let mut result = Vec::new();
    for (i, chunk) in chunks.into_iter().enumerate() {
        if i > 0 {
            result.push(separator.clone());
        }
        result.push(chunk);
    }
    FormatChunk::Concat(result)
}

/// Create a space text chunk.
pub fn space() -> FormatChunk {
    FormatChunk::Text(" ".into())
}
