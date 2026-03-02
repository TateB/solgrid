//! Rule: gas/unchecked-increment
//!
//! Loop counter increments/decrements can be wrapped in `unchecked { }` blocks
//! for gas savings since overflow is already prevented by the loop condition
//! (Solidity >= 0.8.0).

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;

static META: RuleMeta = RuleMeta {
    id: "gas/unchecked-increment",
    name: "unchecked-increment",
    category: RuleCategory::Gas,
    default_severity: Severity::Info,
    description: "loop counter increment can use `unchecked` block to save gas",
    fix_availability: FixAvailability::None,
};

pub struct UncheckedIncrementRule;

impl Rule for UncheckedIncrementRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // Find `for` loops with i++/++i/i--/--i in the update expression
        let mut search_from = 0;
        while let Some(for_pos) = ctx.source[search_from..].find("for") {
            let abs_for = search_from + for_pos;

            // Check word boundary
            if abs_for > 0 {
                let prev = ctx.source.as_bytes()[abs_for - 1];
                if prev.is_ascii_alphanumeric() || prev == b'_' {
                    search_from = abs_for + 3;
                    continue;
                }
            }

            // Find the opening parenthesis
            let after_for = &ctx.source[abs_for + 3..];
            if let Some(paren_pos) = after_for.find('(') {
                let paren_start = abs_for + 3 + paren_pos;
                if let Some(paren_end) = find_matching_paren(ctx.source, paren_start) {
                    let header = &ctx.source[paren_start + 1..paren_end];

                    // Split by `;` to get the update expression (third part)
                    let parts: Vec<&str> = header.splitn(3, ';').collect();
                    if parts.len() == 3 {
                        let update = parts[2].trim();

                        // Check if the update is a simple increment/decrement
                        let is_increment = update.ends_with("++")
                            || update.starts_with("++")
                            || update.ends_with("--")
                            || update.starts_with("--")
                            || update.contains("+= 1")
                            || update.contains("-= 1");

                        if is_increment {
                            // Check if already in an unchecked block
                            let before_for = &ctx.source[..abs_for];
                            let is_in_unchecked = before_for.trim_end().ends_with('{')
                                && before_for.contains("unchecked");

                            if !is_in_unchecked {
                                // Find the update expression position
                                let second_semi = paren_start
                                    + 1
                                    + header
                                        .match_indices(';')
                                        .nth(1)
                                        .map(|(i, _)| i)
                                        .unwrap_or(0);
                                let update_start = second_semi + 1;
                                let update_text = ctx.source[update_start..paren_end].trim_start();
                                let update_abs = paren_end - update_text.len();

                                diagnostics.push(Diagnostic::new(
                                    META.id,
                                    "loop counter increment can be wrapped in `unchecked { }` to save gas (Solidity >= 0.8.0)",
                                    META.default_severity,
                                    update_abs..paren_end,
                                ));
                            }
                        }
                    }
                }
            }

            search_from = abs_for + 3;
        }
        diagnostics
    }
}

fn find_matching_paren(source: &str, open_pos: usize) -> Option<usize> {
    let mut depth = 0;
    for (i, ch) in source[open_pos..].char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open_pos + i);
                }
            }
            _ => {}
        }
    }
    None
}
