//! Rule: security/no-inline-assembly
//!
//! Flag inline assembly usage.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;

static META: RuleMeta = RuleMeta {
    id: "security/no-inline-assembly",
    name: "no-inline-assembly",
    category: RuleCategory::Security,
    default_severity: Severity::Warning,
    description: "avoid using inline assembly",
    fix_availability: FixAvailability::None,
};

pub struct NoInlineAssemblyRule;

impl Rule for NoInlineAssemblyRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        // Look for `assembly {` or `assembly "evmasm" {` patterns
        let mut search_from = 0;
        while let Some(pos) = ctx.source[search_from..].find("assembly") {
            let abs_pos = search_from + pos;
            // Ensure it's a keyword (not part of larger identifier)
            if abs_pos > 0 {
                let prev = ctx.source.as_bytes()[abs_pos - 1];
                if prev.is_ascii_alphanumeric() || prev == b'_' {
                    search_from = abs_pos + 8;
                    continue;
                }
            }
            // Check character after "assembly"
            let after = &ctx.source[abs_pos + 8..];
            let after_trimmed = after.trim_start();
            if after_trimmed.starts_with('{') || after_trimmed.starts_with('"') {
                diagnostics.push(Diagnostic::new(
                    META.id,
                    "avoid using inline assembly",
                    META.default_severity,
                    abs_pos..abs_pos + 8,
                ));
            }
            search_from = abs_pos + 8;
        }
        diagnostics
    }
}
