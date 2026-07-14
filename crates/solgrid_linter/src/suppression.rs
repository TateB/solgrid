//! Inline suppression comment parsing.
//!
//! Supports:
//! - `// solgrid-disable-next-line [rule-id[, rule-id...]]`
//! - `// solgrid-disable-line [rule-id[, rule-id...]]`
//! - `// solgrid-disable [rule-id[, rule-id...]]` /
//!   `// solgrid-enable [rule-id[, rule-id...]]`

use crate::source_utils::{scan_source_regions, RegionKind};
use std::collections::{HashMap, HashSet};

/// Parsed suppression directives for a file.
pub struct Suppressions {
    /// Lines where specific rules are suppressed.
    suppressed_lines: HashMap<usize, HashSet<String>>,
    /// Lines where all rules are suppressed.
    blanket_suppressed_lines: HashSet<usize>,
}

impl Suppressions {
    /// Check if a rule is suppressed at a given line.
    pub fn is_suppressed(&self, rule_id: &str, line: usize) -> bool {
        if self.blanket_suppressed_lines.contains(&line) {
            return true;
        }
        if let Some(rules) = self.suppressed_lines.get(&line) {
            if rules.contains(rule_id) {
                return true;
            }
            // Check category match (e.g. "security" matches "security/tx-origin")
            if let Some(category) = rule_id.split('/').next() {
                if rules.contains(category) {
                    return true;
                }
            }
        }
        false
    }
}

/// Parse suppression comments from source code.
pub fn parse_suppressions(source: &str) -> Suppressions {
    let mut suppressed_lines: HashMap<usize, HashSet<String>> = HashMap::new();
    let mut blanket_suppressed_lines: HashSet<usize> = HashSet::new();
    let mut disable_ranges: Vec<(Option<String>, usize)> = Vec::new(); // (rule, start_line)
    let mut line_num = 1usize;
    let mut cursor = 0usize;

    for region in scan_source_regions(source) {
        let start = region.range.start.min(source.len());
        let end = region.range.end.min(source.len());
        line_num += source[cursor..start]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count();

        if region.kind == RegionKind::LineComment {
            let comment = source[start + 2..end].trim();
            parse_comment_directive(
                comment,
                line_num,
                &mut suppressed_lines,
                &mut blanket_suppressed_lines,
                &mut disable_ranges,
            );
        }

        line_num += source[start..end]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count();
        cursor = end;
    }

    let last_line = source.lines().count().max(1);
    for (rule, start) in disable_ranges {
        for line in start..=last_line {
            if let Some(id) = &rule {
                suppressed_lines.entry(line).or_default().insert(id.clone());
            } else {
                blanket_suppressed_lines.insert(line);
            }
        }
    }

    Suppressions {
        suppressed_lines,
        blanket_suppressed_lines,
    }
}

fn parse_comment_directive(
    comment: &str,
    line_num: usize,
    suppressed_lines: &mut HashMap<usize, HashSet<String>>,
    blanket_suppressed_lines: &mut HashSet<usize>,
    disable_ranges: &mut Vec<(Option<String>, usize)>,
) {
    let prefixes = [
        "solgrid-disable-next-line",
        "solgrid-disable-line",
        "solgrid-disable",
        "solgrid-enable",
        // Compatibility with solhint
        "solhint-disable-next-line",
        "solhint-disable-line",
    ];

    for prefix in &prefixes {
        if let Some(rest) = comment.strip_prefix(prefix) {
            let rule_ids = parse_rule_ids(rest);

            if prefix.ends_with("next-line") {
                let target_line = line_num + 1;
                if let Some(ids) = rule_ids {
                    suppressed_lines.entry(target_line).or_default().extend(ids);
                } else {
                    blanket_suppressed_lines.insert(target_line);
                }
            } else if prefix.ends_with("disable-line") {
                if let Some(ids) = rule_ids {
                    suppressed_lines.entry(line_num).or_default().extend(ids);
                } else {
                    blanket_suppressed_lines.insert(line_num);
                }
            } else if prefix.ends_with("disable") {
                if let Some(ids) = rule_ids {
                    disable_ranges.extend(ids.into_iter().map(|id| (Some(id), line_num)));
                } else {
                    disable_ranges.push((None, line_num));
                }
            } else if prefix.ends_with("enable") {
                if let Some(ids) = rule_ids {
                    for id in ids {
                        close_disable_range(
                            Some(&id),
                            line_num,
                            suppressed_lines,
                            blanket_suppressed_lines,
                            disable_ranges,
                        );
                    }
                } else {
                    close_disable_range(
                        None,
                        line_num,
                        suppressed_lines,
                        blanket_suppressed_lines,
                        disable_ranges,
                    );
                }
            }
            break;
        }
    }
}

fn parse_rule_ids(rest: &str) -> Option<HashSet<String>> {
    let ids: HashSet<_> = rest
        .split(',')
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
        .collect();
    (!ids.is_empty()).then_some(ids)
}

fn close_disable_range(
    rule_id: Option<&str>,
    line_num: usize,
    suppressed_lines: &mut HashMap<usize, HashSet<String>>,
    blanket_suppressed_lines: &mut HashSet<usize>,
    disable_ranges: &mut Vec<(Option<String>, usize)>,
) {
    let Some(pos) = disable_ranges
        .iter()
        .rposition(|(rule, _)| rule.as_deref() == rule_id)
    else {
        return;
    };

    let (rule, start) = disable_ranges.remove(pos);
    for line in start..=line_num {
        if let Some(id) = &rule {
            suppressed_lines.entry(line).or_default().insert(id.clone());
        } else {
            blanket_suppressed_lines.insert(line);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse_suppressions;

    #[test]
    fn comma_separated_next_line_ids_are_trimmed_and_empty_entries_ignored() {
        let suppressions = parse_suppressions(
            "// solgrid-disable-next-line security/tx-origin, , best-practices/no-empty-blocks,,\ncode\n",
        );

        assert!(suppressions.is_suppressed("security/tx-origin", 2));
        assert!(suppressions.is_suppressed("best-practices/no-empty-blocks", 2));
        assert!(!suppressions.is_suppressed("security/reentrancy", 2));
    }

    #[test]
    fn comma_separated_disable_line_suppresses_each_rule() {
        let suppressions = parse_suppressions(
            "code; // solgrid-disable-line security/tx-origin, best-practices/no-empty-blocks\n",
        );

        assert!(suppressions.is_suppressed("security/tx-origin", 1));
        assert!(suppressions.is_suppressed("best-practices/no-empty-blocks", 1));
        assert!(!suppressions.is_suppressed("security/reentrancy", 1));
    }

    #[test]
    fn comma_separated_disable_enable_group_suppresses_each_rule() {
        let suppressions = parse_suppressions(
            "// solgrid-disable security/tx-origin, best-practices/no-empty-blocks\n\
             code\n\
             // solgrid-enable best-practices/no-empty-blocks, security/tx-origin\n\
             code\n",
        );

        assert!(suppressions.is_suppressed("security/tx-origin", 2));
        assert!(suppressions.is_suppressed("best-practices/no-empty-blocks", 2));
        assert!(!suppressions.is_suppressed("security/tx-origin", 4));
        assert!(!suppressions.is_suppressed("best-practices/no-empty-blocks", 4));
    }

    #[test]
    fn single_id_and_blanket_directives_keep_their_existing_behavior() {
        let suppressions = parse_suppressions(
            "// solgrid-disable-next-line security/tx-origin\n\
             code\n\
             // solgrid-disable-next-line , ,\n\
             code\n",
        );

        assert!(suppressions.is_suppressed("security/tx-origin", 2));
        assert!(!suppressions.is_suppressed("security/reentrancy", 2));
        assert!(suppressions.is_suppressed("any/rule", 4));
    }

    #[test]
    fn unclosed_disable_group_applies_through_end_of_file() {
        let suppressions = parse_suppressions(
            "// solgrid-disable security/tx-origin, best-practices/no-empty-blocks\n\
             code\n\
             code\n",
        );

        assert!(suppressions.is_suppressed("security/tx-origin", 3));
        assert!(suppressions.is_suppressed("best-practices/no-empty-blocks", 3));
        assert!(!suppressions.is_suppressed("security/reentrancy", 3));
    }

    #[test]
    fn url_in_string_does_not_hide_trailing_directive() {
        let suppressions = parse_suppressions(
            r#"string memory url = "https://example.test/path"; // solgrid-disable-line security/tx-origin
"#,
        );

        assert!(suppressions.is_suppressed("security/tx-origin", 1));
        assert!(!suppressions.is_suppressed("security/reentrancy", 1));
    }

    #[test]
    fn escaped_quote_and_comment_marker_in_string_do_not_hide_trailing_directive() {
        let suppressions = parse_suppressions(
            r#"string memory text = "escaped \" // still a string"; // solgrid-disable-line security/tx-origin
"#,
        );

        assert!(suppressions.is_suppressed("security/tx-origin", 1));
    }

    #[test]
    fn comment_marker_in_block_comment_does_not_hide_trailing_directive() {
        let suppressions = parse_suppressions(
            "/* example: https://example.test */ code; // solgrid-disable-line security/tx-origin\n",
        );

        assert!(suppressions.is_suppressed("security/tx-origin", 1));
    }

    #[test]
    fn directive_shaped_text_inside_unclosed_string_is_not_a_comment() {
        let suppressions = parse_suppressions("string memory text = \"// solgrid-disable-line\n");

        assert!(!suppressions.is_suppressed("security/tx-origin", 1));
    }
}
