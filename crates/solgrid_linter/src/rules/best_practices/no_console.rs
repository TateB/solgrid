//! Rule: best-practices/no-console
//!
//! Remove `console.log` / `console2.log` statements.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;

static META: RuleMeta = RuleMeta {
    id: "best-practices/no-console",
    name: "no-console",
    category: RuleCategory::BestPractices,
    default_severity: Severity::Warning,
    description: "remove console log statements before deployment",
    fix_availability: FixAvailability::None,
};

pub struct NoConsoleRule;

impl Rule for NoConsoleRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let patterns = [
            "console.log",
            "console2.log",
            "console.logInt",
            "console.logString",
            "console.logBool",
            "console.logAddress",
            "console.logBytes",
        ];

        for pattern in &patterns {
            let mut search_from = 0;
            while let Some(pos) = ctx.source[search_from..].find(pattern) {
                let abs_pos = search_from + pos;
                diagnostics.push(Diagnostic::new(
                    META.id,
                    format!("remove `{pattern}` statement before deployment"),
                    META.default_severity,
                    abs_pos..abs_pos + pattern.len(),
                ));
                search_from = abs_pos + pattern.len();
            }
        }

        diagnostics.sort_by_key(|d| d.span.start);
        diagnostics
    }
}
