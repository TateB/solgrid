//! Rule: security/not-rely-on-block-hash
//!
//! Avoid relying on `blockhash()` for randomness.  The block hash can be
//! influenced by miners and should not be used as a source of entropy for
//! security-critical logic.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;

static META: RuleMeta = RuleMeta {
    id: "security/not-rely-on-block-hash",
    name: "not-rely-on-block-hash",
    category: RuleCategory::Security,
    default_severity: Severity::Warning,
    description: "avoid using `blockhash()` for randomness",
    fix_availability: FixAvailability::None,
};

pub struct NotRelyOnBlockHashRule;

impl Rule for NotRelyOnBlockHashRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let pattern = "blockhash(";
        let pattern_len = pattern.len();
        let keyword_len = 9; // "blockhash"
        let mut search_from = 0;
        while let Some(pos) = ctx.source[search_from..].find(pattern) {
            let abs_pos = search_from + pos;
            // Ensure it's not part of a larger identifier
            if abs_pos > 0 {
                let prev = ctx.source.as_bytes()[abs_pos - 1];
                if prev.is_ascii_alphanumeric() || prev == b'_' || prev == b'.' {
                    search_from = abs_pos + pattern_len;
                    continue;
                }
            }
            if ctx.is_in_comment_or_string(abs_pos) {
                search_from = abs_pos + pattern_len;
                continue;
            }
            diagnostics.push(Diagnostic::new(
                META.id,
                "do not rely on `blockhash()` for randomness — it can be influenced by miners",
                META.default_severity,
                abs_pos..abs_pos + keyword_len,
            ));
            search_from = abs_pos + pattern_len;
        }
        diagnostics
    }
}
