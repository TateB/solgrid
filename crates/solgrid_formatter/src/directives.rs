//! Formatter directives — `// solgrid-fmt: off/on` and compatibility.
//!
//! Regions between `off` and `on` directives are passed through verbatim
//! without formatting.

use std::ops::Range;

/// A directive that controls formatting behavior.
#[derive(Debug, Clone)]
pub enum Directive {
    /// Disable formatting from this point.
    Off(usize),
    /// Re-enable formatting from this point.
    On(usize),
    /// Disable formatting for the next line only.
    DisableNextLine(usize),
}

/// Parse formatter directives from source text.
pub fn parse_directives(source: &str) -> Vec<Directive> {
    let mut directives = Vec::new();

    for (line_idx, line) in source.lines().enumerate() {
        let trimmed = line.trim();

        // solgrid-fmt: off / on
        if let Some(rest) = trimmed.strip_prefix("//") {
            let rest = rest.trim();
            if rest.eq_ignore_ascii_case("solgrid-fmt: off")
                || rest.eq_ignore_ascii_case("solgrid-fmt:off")
            {
                directives.push(Directive::Off(line_idx));
            } else if rest.eq_ignore_ascii_case("solgrid-fmt: on")
                || rest.eq_ignore_ascii_case("solgrid-fmt:on")
            {
                directives.push(Directive::On(line_idx));
            }
            // forgefmt compatibility
            else if rest.eq_ignore_ascii_case("forgefmt: disable-start") {
                directives.push(Directive::Off(line_idx));
            } else if rest.eq_ignore_ascii_case("forgefmt: disable-end") {
                directives.push(Directive::On(line_idx));
            } else if rest.eq_ignore_ascii_case("forgefmt: disable-next-line") {
                directives.push(Directive::DisableNextLine(line_idx));
            }
        }
    }

    directives
}

/// Compute disabled byte ranges from directives.
/// Returns a list of byte ranges where formatting should be skipped.
pub fn compute_disabled_ranges(source: &str, directives: &[Directive]) -> Vec<Range<usize>> {
    let line_offsets = compute_line_offsets(source);
    let mut ranges = Vec::new();

    let mut off_start: Option<usize> = None;

    for directive in directives {
        match directive {
            Directive::Off(line_idx) => {
                if off_start.is_none() {
                    off_start = Some(line_offset(&line_offsets, *line_idx));
                }
            }
            Directive::On(line_idx) => {
                if let Some(start) = off_start.take() {
                    let end = line_end_offset(source, &line_offsets, *line_idx);
                    ranges.push(start..end);
                }
            }
            Directive::DisableNextLine(line_idx) => {
                let next_line = line_idx + 1;
                if next_line < line_offsets.len() {
                    let start = line_offset(&line_offsets, next_line);
                    let end = line_end_offset(source, &line_offsets, next_line);
                    ranges.push(start..end);
                }
            }
        }
    }

    // If there's an unclosed Off, extend to end of file.
    if let Some(start) = off_start {
        ranges.push(start..source.len());
    }

    ranges
}

/// Check if a byte position falls within any disabled range.
pub fn is_disabled(pos: usize, disabled_ranges: &[Range<usize>]) -> bool {
    disabled_ranges.iter().any(|r| r.contains(&pos))
}

fn compute_line_offsets(source: &str) -> Vec<usize> {
    let mut offsets = vec![0];
    for (i, b) in source.bytes().enumerate() {
        if b == b'\n' && i < source.len() {
            offsets.push(i + 1);
        }
    }
    offsets
}

fn line_offset(offsets: &[usize], line: usize) -> usize {
    offsets.get(line).copied().unwrap_or(0)
}

fn line_end_offset(source: &str, offsets: &[usize], line: usize) -> usize {
    offsets.get(line + 1).copied().unwrap_or(source.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_off_on() {
        let source = "line1\n// solgrid-fmt: off\nline3\n// solgrid-fmt: on\nline5";
        let directives = parse_directives(source);
        assert_eq!(directives.len(), 2);
        assert!(matches!(directives[0], Directive::Off(1)));
        assert!(matches!(directives[1], Directive::On(3)));
    }

    #[test]
    fn test_forgefmt_compat() {
        let source = "// forgefmt: disable-start\ncode\n// forgefmt: disable-end\n";
        let directives = parse_directives(source);
        assert_eq!(directives.len(), 2);
        assert!(matches!(directives[0], Directive::Off(0)));
        assert!(matches!(directives[1], Directive::On(2)));
    }

    #[test]
    fn test_disable_next_line() {
        let source = "line1\n// forgefmt: disable-next-line\nline3\nline4";
        let directives = parse_directives(source);
        assert_eq!(directives.len(), 1);
        assert!(matches!(directives[0], Directive::DisableNextLine(1)));
    }

    #[test]
    fn test_disabled_ranges() {
        let source = "line1\n// solgrid-fmt: off\nline3\n// solgrid-fmt: on\nline5";
        let directives = parse_directives(source);
        let ranges = compute_disabled_ranges(source, &directives);
        assert_eq!(ranges.len(), 1);
        // The disabled range should cover from "// solgrid-fmt: off" to end of "// solgrid-fmt: on" line
        assert!(is_disabled(6, &ranges)); // start of off line
        assert!(!is_disabled(0, &ranges)); // line1 is not disabled
    }

    #[test]
    fn test_unclosed_off() {
        let source = "line1\n// solgrid-fmt: off\nline3\nline4";
        let directives = parse_directives(source);
        let ranges = compute_disabled_ranges(source, &directives);
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].end, source.len());
    }
}
