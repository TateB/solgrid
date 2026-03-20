//! Rule: style/max-line-length
//!
//! Enforce a maximum line length (default: 120 characters).

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;

static META: RuleMeta = RuleMeta {
    id: "style/max-line-length",
    name: "max-line-length",
    category: RuleCategory::Style,
    default_severity: Severity::Info,
    description: "line length must not exceed 120 characters",
    fix_availability: FixAvailability::None,
};

pub struct MaxLineLengthRule;

impl Rule for MaxLineLengthRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let max_length = ctx.config.lint.max_line_length();
        let mut diagnostics = Vec::new();
        let mut offset = 0;

        for (line_idx, line) in ctx.source.lines().enumerate() {
            let char_count = line.chars().count();
            if char_count > max_length {
                diagnostics.push(Diagnostic::new(
                    META.id,
                    format!(
                        "line {} exceeds maximum length ({} > {})",
                        line_idx + 1,
                        char_count,
                        max_length
                    ),
                    META.default_severity,
                    offset..offset + line.len(),
                ));
            }
            // Advance past the line content plus the line ending (\n or \r\n)
            offset += line.len();
            if ctx.source[offset..].starts_with("\r\n") {
                offset += 2;
            } else if ctx.source[offset..].starts_with('\n') {
                offset += 1;
            }
        }

        diagnostics
    }
}
