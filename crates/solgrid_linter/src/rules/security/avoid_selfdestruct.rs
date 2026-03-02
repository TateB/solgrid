//! Rule: security/avoid-selfdestruct
//!
//! Flag usage of `selfdestruct`, which is deprecated since Solidity 0.8.18+
//! and will eventually be removed from the EVM (EIP-6049).

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;

static META: RuleMeta = RuleMeta {
    id: "security/avoid-selfdestruct",
    name: "avoid-selfdestruct",
    category: RuleCategory::Security,
    default_severity: Severity::Error,
    description: "`selfdestruct` is deprecated (EIP-6049) and should be avoided",
    fix_availability: FixAvailability::None,
};

pub struct AvoidSelfdestructRule;

impl Rule for AvoidSelfdestructRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let mut search_from = 0;
        while let Some(pos) = ctx.source[search_from..].find("selfdestruct(") {
            let abs_pos = search_from + pos;
            // Ensure it's not part of a larger identifier
            if abs_pos > 0 {
                let prev = ctx.source.as_bytes()[abs_pos - 1];
                if prev.is_ascii_alphanumeric() || prev == b'_' || prev == b'.' {
                    search_from = abs_pos + 13;
                    continue;
                }
            }
            diagnostics.push(Diagnostic::new(
                META.id,
                "`selfdestruct` is deprecated (EIP-6049) and should not be used",
                META.default_severity,
                abs_pos..abs_pos + 12,
            ));
            search_from = abs_pos + 13;
        }
        diagnostics
    }
}
