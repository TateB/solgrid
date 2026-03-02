//! Rule: security/avoid-suicide
//!
//! Use `selfdestruct()` instead of deprecated `suicide()`.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;

static META: RuleMeta = RuleMeta {
    id: "security/avoid-suicide",
    name: "avoid-suicide",
    category: RuleCategory::Security,
    default_severity: Severity::Error,
    description: "use `selfdestruct()` instead of deprecated `suicide()`",
    fix_availability: FixAvailability::Available(FixSafety::Safe),
};

pub struct AvoidSuicideRule;

impl Rule for AvoidSuicideRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let mut search_from = 0;
        while let Some(pos) = ctx.source[search_from..].find("suicide(") {
            let abs_pos = search_from + pos;
            if abs_pos > 0 {
                let prev = ctx.source.as_bytes()[abs_pos - 1];
                if prev.is_ascii_alphanumeric() || prev == b'_' || prev == b'.' {
                    search_from = abs_pos + 7;
                    continue;
                }
            }
            diagnostics.push(
                Diagnostic::new(
                    META.id,
                    "use `selfdestruct()` instead of deprecated `suicide()`",
                    META.default_severity,
                    abs_pos..abs_pos + 7,
                )
                .with_fix(Fix::safe(
                    "Replace `suicide` with `selfdestruct`",
                    vec![TextEdit::replace(abs_pos..abs_pos + 7, "selfdestruct")],
                )),
            );
            search_from = abs_pos + 7;
        }
        diagnostics
    }
}
