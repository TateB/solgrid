//! Shared NatSpec parsing helpers.

use std::ops::Range;

/// NatSpec comment style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NatSpecStyle {
    TripleSlash,
    Block,
}

/// A NatSpec block attached to a declaration.
#[derive(Debug, Clone)]
pub struct NatSpecBlock {
    pub style: NatSpecStyle,
    pub range: Range<usize>,
    pub indent: String,
    /// The block rendered as triple-slash lines, one entry per line, without a
    /// trailing newline.
    pub lines: Vec<String>,
}

impl NatSpecBlock {
    /// Render the block as text without a trailing newline.
    pub fn render(&self) -> String {
        self.lines.join("\n")
    }

    /// Return the block lines with comment markers stripped.
    pub fn stripped_lines(&self) -> Vec<String> {
        self.lines
            .iter()
            .map(|line| strip_line_marker(line).to_string())
            .collect()
    }
}

/// Find a NatSpec block attached to the declaration that starts at `item_start`.
pub fn find_attached_natspec(source: &str, item_start: usize) -> Option<NatSpecBlock> {
    let mut cursor = line_start(source, item_start);
    while cursor > 0 {
        let (prev_start, prev_end) = previous_line_bounds(source, cursor)?;
        let line = &source[prev_start..prev_end];
        if line.trim().is_empty() {
            cursor = prev_start;
            continue;
        }

        if line.trim_start().starts_with("///") {
            let mut start = prev_start;
            let end = prev_end;
            while start > 0 {
                let Some((line_start, line_end)) = previous_line_bounds(source, start) else {
                    break;
                };
                let prev_line = &source[line_start..line_end];
                if prev_line.trim_start().starts_with("///") {
                    start = line_start;
                } else {
                    break;
                }
            }

            let lines = source[start..end]
                .lines()
                .map(ToString::to_string)
                .collect::<Vec<_>>();
            let indent = line[..line.find("///").unwrap_or(0)].to_string();
            return Some(NatSpecBlock {
                style: NatSpecStyle::TripleSlash,
                range: start..end,
                indent,
                lines,
            });
        }

        if line.trim_end().ends_with("*/") {
            let mut start = prev_start;
            let end = prev_end;
            loop {
                let current_line = &source[start..line_end(source, start)];
                let trimmed = current_line.trim_start();
                if trimmed.starts_with("/**") {
                    let indent = current_line[..current_line.find("/**").unwrap_or(0)].to_string();
                    let raw_lines = source[start..end]
                        .lines()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>();
                    return Some(NatSpecBlock {
                        style: NatSpecStyle::Block,
                        range: start..end,
                        indent: indent.clone(),
                        lines: convert_block_lines(&raw_lines, &indent),
                    });
                }
                if trimmed.starts_with("/*") {
                    return None;
                }
                if start == 0 {
                    break;
                }
                let Some((line_start, _)) = previous_line_bounds(source, start) else {
                    break;
                };
                start = line_start;
            }
        }

        break;
    }

    None
}

/// Return the start byte offset of the line containing `pos`.
pub fn line_start(source: &str, pos: usize) -> usize {
    source[..pos].rfind('\n').map(|idx| idx + 1).unwrap_or(0)
}

/// Return the end byte offset of the line containing `pos`, excluding the newline.
pub fn line_end(source: &str, pos: usize) -> usize {
    source[pos..]
        .find('\n')
        .map(|idx| pos + idx)
        .unwrap_or(source.len())
}

/// Return the bounds of the line preceding `cursor`, where `cursor` is the
/// start of the current line.
pub fn previous_line_bounds(source: &str, cursor: usize) -> Option<(usize, usize)> {
    if cursor == 0 {
        return None;
    }

    let prev_end = cursor.saturating_sub(1);
    let prev_start = source[..prev_end]
        .rfind('\n')
        .map(|idx| idx + 1)
        .unwrap_or(0);
    Some((prev_start, prev_end))
}

/// Strip NatSpec comment markers from a rendered triple-slash line.
pub fn strip_line_marker(line: &str) -> &str {
    let trimmed = line.trim_start();
    if let Some(rest) = trimmed.strip_prefix("///") {
        if let Some(rest) = rest.strip_prefix(' ') {
            rest
        } else {
            rest
        }
    } else {
        trimmed
    }
}

/// Render plain NatSpec contents back to triple-slash lines.
pub fn render_triple_slash_block(indent: &str, contents: &[String]) -> String {
    contents
        .iter()
        .map(|content| {
            if content.is_empty() {
                format!("{indent}///")
            } else {
                format!("{indent}/// {content}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn convert_block_lines(lines: &[String], indent: &str) -> Vec<String> {
    let mut result = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let is_first = index == 0;
        let is_last = index + 1 == lines.len();

        let trimmed = line.trim_start();
        let mut work = trimmed;

        if is_first {
            if let Some(rest) = work.strip_prefix("/**") {
                work = rest;
            }
            if let Some(rest) = work.strip_prefix(' ') {
                work = rest;
            }
        }

        if is_last {
            if let Some(rest) = work.strip_suffix("*/") {
                work = rest.trim_end();
            } else if let Some(pos) = work.rfind("*/") {
                work = work[..pos].trim_end();
            }
        }

        if !is_first {
            if let Some(rest) = work.strip_prefix("* ") {
                work = rest;
            } else if let Some(rest) = work.strip_prefix('*') {
                work = rest;
            }
        }

        let content = work.trim_end();
        if content.is_empty() {
            if !(is_first || is_last) {
                result.push(format!("{indent}///"));
            }
        } else {
            result.push(format!("{indent}/// {content}"));
        }
    }
    result
}
