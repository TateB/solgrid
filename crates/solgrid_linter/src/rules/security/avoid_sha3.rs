//! Rule: security/avoid-sha3
//!
//! Use `keccak256()` instead of deprecated `sha3()`.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;

static META: RuleMeta = RuleMeta {
    id: "security/avoid-sha3",
    name: "avoid-sha3",
    category: RuleCategory::Security,
    default_severity: Severity::Error,
    description: "use `keccak256()` instead of deprecated `sha3()`",
    fix_availability: FixAvailability::Available(FixSafety::Safe),
};

pub struct AvoidSha3Rule;

impl Rule for AvoidSha3Rule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let mut search_from = 0;
        while let Some(pos) = ctx.source[search_from..].find("sha3(") {
            let abs_pos = search_from + pos;
            // Verify it's not part of a larger identifier
            if abs_pos > 0 {
                let prev = ctx.source.as_bytes()[abs_pos - 1];
                if prev.is_ascii_alphanumeric() || prev == b'_' || prev == b'.' {
                    search_from = abs_pos + 4;
                    continue;
                }
            }
            diagnostics.push(
                Diagnostic::new(
                    META.id,
                    "use `keccak256()` instead of deprecated `sha3()`",
                    META.default_severity,
                    abs_pos..abs_pos + 4,
                )
                .with_fix(Fix::safe(
                    "Replace `sha3` with `keccak256`",
                    vec![TextEdit::replace(abs_pos..abs_pos + 4, "keccak256")],
                )),
            );
            search_from = abs_pos + 4;
        }
        diagnostics
    }
}
