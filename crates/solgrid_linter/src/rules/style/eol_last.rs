//! Rule: style/eol-last
//!
//! Require a newline at the end of the file.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;

static META: RuleMeta = RuleMeta {
    id: "style/eol-last",
    name: "eol-last",
    category: RuleCategory::Style,
    default_severity: Severity::Info,
    description: "file must end with a newline",
    fix_availability: FixAvailability::Available(FixSafety::Safe),
};

pub struct EolLastRule;

impl Rule for EolLastRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        if ctx.source.is_empty() || ctx.source.ends_with('\n') {
            return Vec::new();
        }

        let len = ctx.source.len();
        vec![Diagnostic::new(
            META.id,
            "file must end with a newline",
            META.default_severity,
            len..len,
        )
        .with_fix(Fix::safe(
            "Add newline at end of file",
            vec![TextEdit::insert(len, "\n")],
        ))]
    }
}
