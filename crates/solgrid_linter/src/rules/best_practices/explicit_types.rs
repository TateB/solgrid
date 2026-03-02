//! Rule: best-practices/explicit-types
//!
//! Use `uint256` instead of `uint`, `int256` instead of `int`.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;

static META: RuleMeta = RuleMeta {
    id: "best-practices/explicit-types",
    name: "explicit-types",
    category: RuleCategory::BestPractices,
    default_severity: Severity::Warning,
    description: "use explicit type sizes (e.g. `uint256` instead of `uint`)",
    fix_availability: FixAvailability::Available(FixSafety::Safe),
};

pub struct ExplicitTypesRule;

impl Rule for ExplicitTypesRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // Search for standalone `uint` or `int` that aren't followed by digits
        for (pattern, replacement) in [("uint", "uint256"), ("int", "int256")] {
            let mut search_from = 0;
            while let Some(pos) = ctx.source[search_from..].find(pattern) {
                let abs_pos = search_from + pos;

                // Check it's not part of a larger type like uint256, uint8, etc.
                let after_pos = abs_pos + pattern.len();
                if after_pos < ctx.source.len() {
                    let next_char = ctx.source.as_bytes()[after_pos];
                    if next_char.is_ascii_digit()
                        || next_char.is_ascii_alphanumeric()
                        || next_char == b'_'
                    {
                        search_from = after_pos;
                        continue;
                    }
                }

                // Check it's not part of a larger identifier before
                if abs_pos > 0 {
                    let prev = ctx.source.as_bytes()[abs_pos - 1];
                    if prev.is_ascii_alphanumeric() || prev == b'_' {
                        search_from = after_pos;
                        continue;
                    }
                }

                diagnostics.push(
                    Diagnostic::new(
                        META.id,
                        format!("use `{replacement}` instead of `{pattern}`"),
                        META.default_severity,
                        abs_pos..after_pos,
                    )
                    .with_fix(Fix::safe(
                        format!("Replace `{pattern}` with `{replacement}`"),
                        vec![TextEdit::replace(abs_pos..after_pos, replacement)],
                    )),
                );
                search_from = after_pos;
            }
        }

        diagnostics.sort_by_key(|d| d.span.start);
        diagnostics
    }
}
