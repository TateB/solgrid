//! Rule: security/low-level-calls
//!
//! Flag usage of `.call()`, `.delegatecall()`, `.staticcall()`.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;

static META: RuleMeta = RuleMeta {
    id: "security/low-level-calls",
    name: "low-level-calls",
    category: RuleCategory::Security,
    default_severity: Severity::Warning,
    description: "avoid low-level calls",
    fix_availability: FixAvailability::None,
};

pub struct LowLevelCallsRule;

impl Rule for LowLevelCallsRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let patterns = [".call(", ".call{", ".delegatecall(", ".staticcall("];

        for pattern in &patterns {
            let mut search_from = 0;
            while let Some(pos) = ctx.source[search_from..].find(pattern) {
                let abs_pos = search_from + pos;
                search_from = abs_pos + pattern.len();
                if ctx.is_in_comment_or_string(abs_pos) {
                    continue;
                }
                let method = pattern.trim_start_matches('.').trim_end_matches(['(', '{']);
                diagnostics.push(Diagnostic::new(
                    META.id,
                    format!("avoid using low-level `.{method}()`"),
                    META.default_severity,
                    abs_pos..abs_pos + pattern.len(),
                ));
            }
        }

        diagnostics.sort_by_key(|d| d.span.start);
        diagnostics
    }
}
