//! Rule: security/not-rely-on-time
//!
//! Avoid relying on `block.timestamp` or `now` for critical logic.
//! Block timestamps can be manipulated by miners within a small window and
//! should not be used for security-sensitive decisions.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;

static META: RuleMeta = RuleMeta {
    id: "security/not-rely-on-time",
    name: "not-rely-on-time",
    category: RuleCategory::Security,
    default_severity: Severity::Warning,
    description: "avoid using `block.timestamp` or `now` for critical logic",
    fix_availability: FixAvailability::None,
};

pub struct NotRelyOnTimeRule;

impl Rule for NotRelyOnTimeRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // Search for `block.timestamp`
        {
            let pattern = "block.timestamp";
            let pattern_len = pattern.len();
            let mut search_from = 0;
            while let Some(pos) = ctx.source[search_from..].find(pattern) {
                let abs_pos = search_from + pos;
                // Ensure `block` is not part of a larger identifier
                if abs_pos > 0 {
                    let prev = ctx.source.as_bytes()[abs_pos - 1];
                    if prev.is_ascii_alphanumeric() || prev == b'_' {
                        search_from = abs_pos + pattern_len;
                        continue;
                    }
                }
                // Ensure `timestamp` is not followed by identifier characters
                let end = abs_pos + pattern_len;
                if end < ctx.source.len() {
                    let next = ctx.source.as_bytes()[end];
                    if next.is_ascii_alphanumeric() || next == b'_' {
                        search_from = abs_pos + pattern_len;
                        continue;
                    }
                }
                diagnostics.push(Diagnostic::new(
                    META.id,
                    "avoid relying on `block.timestamp` for critical logic — it can be manipulated by miners",
                    META.default_severity,
                    abs_pos..abs_pos + pattern_len,
                ));
                search_from = abs_pos + pattern_len;
            }
        }

        // Search for standalone `now` keyword
        {
            let pattern = "now";
            let pattern_len = pattern.len();
            let mut search_from = 0;
            while let Some(pos) = ctx.source[search_from..].find(pattern) {
                let abs_pos = search_from + pos;
                // Ensure it's not part of a larger identifier — check previous char
                if abs_pos > 0 {
                    let prev = ctx.source.as_bytes()[abs_pos - 1];
                    if prev.is_ascii_alphanumeric() || prev == b'_' || prev == b'.' {
                        search_from = abs_pos + pattern_len;
                        continue;
                    }
                }
                // Ensure it's not part of a larger identifier — check next char
                let end = abs_pos + pattern_len;
                if end < ctx.source.len() {
                    let next = ctx.source.as_bytes()[end];
                    if next.is_ascii_alphanumeric() || next == b'_' {
                        search_from = abs_pos + pattern_len;
                        continue;
                    }
                }
                diagnostics.push(Diagnostic::new(
                    META.id,
                    "avoid relying on `now` (alias for `block.timestamp`) for critical logic",
                    META.default_severity,
                    abs_pos..abs_pos + pattern_len,
                ));
                search_from = abs_pos + pattern_len;
            }
        }

        diagnostics.sort_by_key(|d| d.span.start);
        diagnostics
    }
}
