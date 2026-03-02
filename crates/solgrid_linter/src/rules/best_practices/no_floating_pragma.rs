//! Rule: best-practices/no-floating-pragma
//!
//! Disallow floating pragma versions (e.g. `^0.8.0`).

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;

static META: RuleMeta = RuleMeta {
    id: "best-practices/no-floating-pragma",
    name: "no-floating-pragma",
    category: RuleCategory::BestPractices,
    default_severity: Severity::Warning,
    description: "use a fixed compiler version instead of a floating pragma",
    fix_availability: FixAvailability::None,
};

pub struct NoFloatingPragmaRule;

impl Rule for NoFloatingPragmaRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        let mut search_from = 0;
        while let Some(pos) = ctx.source[search_from..].find("pragma solidity") {
            let abs_pos = search_from + pos;
            // Find the end of the pragma statement
            let rest = &ctx.source[abs_pos..];
            let end = rest.find(';').unwrap_or(rest.len());
            let pragma_text = &rest[..end];

            // Check for floating version indicators
            if pragma_text.contains('^')
                || pragma_text.contains('~')
                || pragma_text.contains(">=")
                || pragma_text.contains('>')
            {
                diagnostics.push(Diagnostic::new(
                    META.id,
                    "use a fixed compiler version instead of a floating pragma",
                    META.default_severity,
                    abs_pos..abs_pos + end,
                ));
            }

            search_from = abs_pos + end;
        }

        diagnostics
    }
}
