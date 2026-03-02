//! Rule: security/divide-before-multiply
//!
//! Detect potential precision loss from performing division before
//! multiplication. In Solidity, integer division truncates, so dividing
//! before multiplying can silently lose precision. For example,
//! `(a / b) * c` may yield `0` when `a < b`, whereas `(a * c) / b` would
//! preserve more precision.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;

static META: RuleMeta = RuleMeta {
    id: "security/divide-before-multiply",
    name: "divide-before-multiply",
    category: RuleCategory::Security,
    default_severity: Severity::Warning,
    description: "division before multiplication may cause precision loss",
    fix_availability: FixAvailability::None,
};

pub struct DivideBeforeMultiplyRule;

impl Rule for DivideBeforeMultiplyRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for (line_idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();

            // Skip comment lines
            if trimmed.starts_with("//") || trimmed.starts_with('*') || trimmed.starts_with("/*") {
                continue;
            }

            // Strip inline comments from the line before analysis
            let code = if let Some(comment_pos) = trimmed.find("//") {
                &trimmed[..comment_pos]
            } else {
                trimmed
            };

            // Look for ` / ` followed by ` * ` on the same line (with spaces
            // around operators to reduce false positives from path-like strings,
            // pointer dereferences, and comments).
            if let Some(div_pos) = code.find(" / ") {
                let after_div = &code[div_pos + 3..];
                if after_div.contains(" * ") {
                    // Calculate the byte offset of the division operator in the
                    // original source.
                    let line_start = ctx
                        .source
                        .lines()
                        .take(line_idx)
                        .map(|l| l.len() + 1) // +1 for the newline
                        .sum::<usize>();

                    // Find the ` / ` position within the original line
                    let original_line = &ctx.source[line_start..];
                    if let Some(orig_div_pos) = original_line.find(" / ") {
                        let abs_pos = line_start + orig_div_pos + 1; // +1 to point at `/`
                        diagnostics.push(Diagnostic::new(
                            META.id,
                            "division before multiplication may cause precision loss; consider reordering to multiply first",
                            META.default_severity,
                            abs_pos..abs_pos + 1,
                        ));
                    }
                }
            }
        }

        diagnostics
    }
}
