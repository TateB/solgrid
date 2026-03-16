//! Wadler-Lindig style line-fitting printer.
//!
//! Converts `FormatChunk` IR into a final output string, choosing between
//! flat mode (single line) and break mode (multi-line) for each `Group`.

use crate::ir::{CommentKind, FormatChunk};
use solgrid_config::FormatConfig;

/// The current printing mode for a group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Print everything on one line (Line → space, Softline → empty).
    Flat,
    /// Print with line breaks (Line → newline + indent).
    Break,
}

/// A command on the printer's work stack.
#[derive(Debug)]
struct PrintCommand {
    indent: usize,
    mode: Mode,
    chunk: FormatChunk,
}

/// Print a document IR to a string.
pub fn print_chunks(chunk: &FormatChunk, config: &FormatConfig) -> String {
    let indent_str = if config.use_tabs {
        "\t".to_string()
    } else {
        " ".repeat(config.tab_width)
    };

    let mut output = String::new();
    let mut pos: usize = 0; // column position on the current line

    // Work stack — processed in LIFO order.
    let mut stack: Vec<PrintCommand> = vec![PrintCommand {
        indent: 0,
        mode: Mode::Break,
        chunk: chunk.clone(),
    }];

    while let Some(cmd) = stack.pop() {
        match cmd.chunk {
            FormatChunk::Text(ref s) => {
                output.push_str(s);
                pos += s.len();
            }
            FormatChunk::Line => match cmd.mode {
                Mode::Flat => {
                    output.push(' ');
                    pos += 1;
                }
                Mode::Break => {
                    output.push('\n');
                    let indent_text = indent_str.repeat(cmd.indent);
                    output.push_str(&indent_text);
                    pos = cmd.indent * config.tab_width;
                }
            },
            FormatChunk::Softline => match cmd.mode {
                Mode::Flat => {
                    // No output in flat mode.
                }
                Mode::Break => {
                    output.push('\n');
                    let indent_text = indent_str.repeat(cmd.indent);
                    output.push_str(&indent_text);
                    pos = cmd.indent * config.tab_width;
                }
            },
            FormatChunk::HardLine => {
                output.push('\n');
                let indent_text = indent_str.repeat(cmd.indent);
                output.push_str(&indent_text);
                pos = cmd.indent * config.tab_width;
            }
            FormatChunk::Group(children) => {
                // Try to fit the group on one line.
                let flat_width = measure_flat_width(&children);
                let fits = flat_width != usize::MAX
                    && pos.saturating_add(flat_width) <= config.line_length;
                let mode = if fits { Mode::Flat } else { Mode::Break };
                // Push children in reverse order (LIFO stack).
                for child in children.into_iter().rev() {
                    stack.push(PrintCommand {
                        indent: cmd.indent,
                        mode,
                        chunk: child,
                    });
                }
            }
            FormatChunk::Indent(children) => {
                for child in children.into_iter().rev() {
                    stack.push(PrintCommand {
                        indent: cmd.indent + 1,
                        mode: cmd.mode,
                        chunk: child,
                    });
                }
            }
            FormatChunk::Concat(children) => {
                for child in children.into_iter().rev() {
                    stack.push(PrintCommand {
                        indent: cmd.indent,
                        mode: cmd.mode,
                        chunk: child,
                    });
                }
            }
            FormatChunk::Comment(kind, ref content) => match kind {
                CommentKind::Line => {
                    output.push_str("//");
                    if !content.is_empty() && !content.starts_with(' ') {
                        output.push(' ');
                    }
                    output.push_str(content);
                    pos = 0; // Will be followed by a newline typically
                }
                CommentKind::DocLine => {
                    output.push_str("///");
                    output.push_str(content);
                    pos = 0;
                }
                CommentKind::Block => {
                    output.push_str("/*");
                    output.push_str(content);
                    output.push_str("*/");
                    pos += 4 + content.len();
                }
            },
            FormatChunk::IfFlat(flat, broken) => {
                let chosen = match cmd.mode {
                    Mode::Flat => *flat,
                    Mode::Break => *broken,
                };
                stack.push(PrintCommand {
                    indent: cmd.indent,
                    mode: cmd.mode,
                    chunk: chosen,
                });
            }
        }
    }

    output
}

/// Measure the flat width of a chunk tree (as if everything is on one line).
/// Returns usize::MAX if it contains a HardLine (can never be flat).
fn measure_flat_width(chunks: &[FormatChunk]) -> usize {
    let mut width: usize = 0;
    for chunk in chunks {
        match chunk {
            FormatChunk::Text(s) => width = width.saturating_add(s.len()),
            FormatChunk::Line => width = width.saturating_add(1), // space
            FormatChunk::Softline => {}                           // empty in flat mode
            FormatChunk::HardLine => return usize::MAX,
            FormatChunk::Group(children) | FormatChunk::Concat(children) => {
                let w = measure_flat_width(children);
                if w == usize::MAX {
                    return usize::MAX;
                }
                width = width.saturating_add(w);
            }
            FormatChunk::Indent(children) => {
                let w = measure_flat_width(children);
                if w == usize::MAX {
                    return usize::MAX;
                }
                width = width.saturating_add(w);
            }
            FormatChunk::Comment(kind, content) => {
                let w = match kind {
                    CommentKind::Line => {
                        2 + if content.is_empty() || content.starts_with(' ') {
                            0
                        } else {
                            1
                        } + content.len()
                    }
                    CommentKind::DocLine => 3 + content.len(),
                    CommentKind::Block => 4 + content.len(),
                };
                width = width.saturating_add(w);
            }
            FormatChunk::IfFlat(flat, _) => {
                let w = measure_flat_width(&[*flat.clone()]);
                if w == usize::MAX {
                    return usize::MAX;
                }
                width = width.saturating_add(w);
            }
        }
    }
    width
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::*;

    fn default_config() -> FormatConfig {
        FormatConfig {
            line_length: 40,
            ..FormatConfig::default()
        }
    }

    #[test]
    fn test_simple_text() {
        let doc = text("hello world");
        let result = print_chunks(&doc, &default_config());
        assert_eq!(result, "hello world");
    }

    #[test]
    fn test_group_fits_on_line() {
        let doc = group(vec![text("a"), line(), text("b"), line(), text("c")]);
        let result = print_chunks(&doc, &default_config());
        assert_eq!(result, "a b c");
    }

    #[test]
    fn test_group_breaks() {
        let doc = group(vec![
            text("this is a long text that"),
            line(),
            text("will not fit on one line at all"),
        ]);
        let result = print_chunks(&doc, &default_config());
        assert_eq!(
            result,
            "this is a long text that\nwill not fit on one line at all"
        );
    }

    #[test]
    fn test_indent() {
        let doc = concat(vec![
            text("if (x) {"),
            indent(vec![hardline(), text("y = 1;")]),
            hardline(),
            text("}"),
        ]);
        let result = print_chunks(&doc, &default_config());
        assert_eq!(result, "if (x) {\n    y = 1;\n}");
    }

    #[test]
    fn test_nested_group_with_indent() {
        let doc = group(vec![
            text("function foo("),
            indent(vec![
                softline(),
                text("uint256 a,"),
                line(),
                text("uint256 b,"),
                line(),
                text("uint256 c"),
            ]),
            softline(),
            text(")"),
        ]);
        let result = print_chunks(&doc, &default_config());
        // Should break since "function foo(uint256 a, uint256 b, uint256 c)" is too long
        assert_eq!(
            result,
            "function foo(\n    uint256 a,\n    uint256 b,\n    uint256 c\n)"
        );
    }

    #[test]
    fn test_hardline_forces_break() {
        let doc = group(vec![text("a"), hardline(), text("b")]);
        let result = print_chunks(&doc, &default_config());
        assert_eq!(result, "a\nb");
    }

    #[test]
    fn test_if_flat() {
        let doc = group(vec![
            text("("),
            if_flat(text(" "), concat(vec![])),
            text("x"),
            if_flat(text(" "), concat(vec![])),
            text(")"),
        ]);
        let result = print_chunks(&doc, &default_config());
        assert_eq!(result, "( x )");
    }

    #[test]
    fn test_line_comment() {
        let doc = concat(vec![
            text("x = 1;"),
            text(" "),
            FormatChunk::Comment(CommentKind::Line, " TODO".into()),
        ]);
        let result = print_chunks(&doc, &default_config());
        assert_eq!(result, "x = 1; // TODO");
    }

    #[test]
    fn test_block_comment() {
        let doc = concat(vec![
            FormatChunk::Comment(CommentKind::Block, " multi-line ".into()),
            hardline(),
            text("code"),
        ]);
        let result = print_chunks(&doc, &default_config());
        assert_eq!(result, "/* multi-line */\ncode");
    }

    #[test]
    fn test_use_tabs() {
        let config = FormatConfig {
            line_length: 40,
            use_tabs: true,
            ..FormatConfig::default()
        };
        let doc = concat(vec![
            text("if (x) {"),
            indent(vec![hardline(), text("y = 1;")]),
            hardline(),
            text("}"),
        ]);
        let result = print_chunks(&doc, &config);
        assert_eq!(result, "if (x) {\n\ty = 1;\n}");
    }
}
