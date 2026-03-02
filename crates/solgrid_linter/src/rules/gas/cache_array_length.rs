//! Rule: gas/cache-array-length
//!
//! Cache `array.length` in a local variable before the loop to avoid
//! repeated SLOAD operations on each iteration.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;

static META: RuleMeta = RuleMeta {
    id: "gas/cache-array-length",
    name: "cache-array-length",
    category: RuleCategory::Gas,
    default_severity: Severity::Info,
    description: "cache `array.length` outside of loop to save gas",
    fix_availability: FixAvailability::None,
};

pub struct CacheArrayLengthRule;

impl Rule for CacheArrayLengthRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // Find `for` loops that reference `.length` in their condition
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
                // Find the matching closing parenthesis
                if let Some(paren_end) = find_matching_paren(ctx.source, paren_start) {
                    let condition_text = &ctx.source[paren_start..paren_end + 1];

                    // Check if the condition contains `.length`
                    if let Some(length_pos) = condition_text.find(".length") {
                        let abs_length = paren_start + length_pos;
                        // Make sure it's not `.length(` (a function call)
                        let after_length = abs_length + 7;
                        if after_length < ctx.source.len() {
                            let next_ch = ctx.source.as_bytes()[after_length];
                            if next_ch == b'(' {
                                search_from = abs_for + 3;
                                continue;
                            }
                        }

                        diagnostics.push(Diagnostic::new(
                            META.id,
                            "cache `array.length` outside of loop to avoid repeated storage reads",
                            META.default_severity,
                            abs_length..abs_length + 7,
                        ));
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
