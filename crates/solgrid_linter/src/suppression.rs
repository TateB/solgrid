//! Inline suppression comment parsing.
//!
//! Supports:
//! - `// solgrid-disable-next-line [rule-id]`
//! - `// solgrid-disable-line [rule-id]`
//! - `// solgrid-disable [rule-id]` / `// solgrid-enable [rule-id]`

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

    for (line_idx, line) in source.lines().enumerate() {
        let line_num = line_idx + 1;
        let trimmed = line.trim();

        // Check for inline comments
        if let Some(comment_start) = trimmed.find("//") {
            let comment = trimmed[comment_start + 2..].trim();
            parse_comment_directive(
                comment,
                line_num,
                &mut suppressed_lines,
                &mut blanket_suppressed_lines,
                &mut disable_ranges,
            );
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
            let rule_id = rest.trim().to_string();
            let rule_id = if rule_id.is_empty() {
                None
            } else {
                Some(rule_id)
            };

            if prefix.ends_with("next-line") {
                let target_line = line_num + 1;
                if let Some(id) = rule_id {
                    suppressed_lines
                        .entry(target_line)
                        .or_default()
                        .insert(id);
                } else {
                    blanket_suppressed_lines.insert(target_line);
                }
            } else if prefix.ends_with("disable-line") {
                if let Some(id) = rule_id {
                    suppressed_lines
                        .entry(line_num)
                        .or_default()
                        .insert(id);
                } else {
                    blanket_suppressed_lines.insert(line_num);
                }
            } else if prefix.ends_with("disable") {
                disable_ranges.push((rule_id, line_num));
            } else if prefix.ends_with("enable") {
                // Find matching disable and suppress all lines in range
                if let Some(pos) = disable_ranges.iter().rposition(|(r, _)| *r == rule_id) {
                    let (rule, start) = disable_ranges.remove(pos);
                    for l in start..=line_num {
                        if let Some(id) = &rule {
                            suppressed_lines.entry(l).or_default().insert(id.clone());
                        } else {
                            blanket_suppressed_lines.insert(l);
                        }
                    }
                }
            }
            break;
        }
    }
}
