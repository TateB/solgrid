//! Rule: style/no-trailing-whitespace
//!
//! Disallow trailing whitespace at the end of lines.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;

static META: RuleMeta = RuleMeta {
    id: "style/no-trailing-whitespace",
    name: "no-trailing-whitespace",
    category: RuleCategory::Style,
    default_severity: Severity::Info,
    description: "no trailing whitespace at end of lines",
    fix_availability: FixAvailability::Available(FixSafety::Safe),
};

pub struct NoTrailingWhitespaceRule;

impl Rule for NoTrailingWhitespaceRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let mut offset = 0;

        for line in ctx.source.split('\n') {
            let trimmed = line.trim_end_matches([' ', '\t']);
            if trimmed.len() < line.len() {
                let ws_start = offset + trimmed.len();
                let ws_end = offset + line.len();
                diagnostics.push(
                    Diagnostic::new(
                        META.id,
                        "trailing whitespace",
                        META.default_severity,
                        ws_start..ws_end,
                    )
                    .with_fix(Fix::safe(
                        "Remove trailing whitespace",
                        vec![TextEdit::delete(ws_start..ws_end)],
                    )),
                );
            }
            // +1 for the '\n' delimiter
            offset += line.len() + 1;
        }

        diagnostics
    }
}
