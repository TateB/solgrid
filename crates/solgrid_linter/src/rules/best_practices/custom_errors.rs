//! Rule: best-practices/custom-errors
//!
//! Prefer custom errors over require/revert with string literals.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;

static META: RuleMeta = RuleMeta {
    id: "best-practices/custom-errors",
    name: "custom-errors",
    category: RuleCategory::BestPractices,
    default_severity: Severity::Warning,
    description: "use custom errors instead of `require()` with string literals",
    fix_availability: FixAvailability::None,
};

pub struct CustomErrorsRule;

impl Rule for CustomErrorsRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        let mut search_from = 0;
        while let Some(pos) = ctx.source[search_from..].find("require(") {
            let abs_pos = search_from + pos;
            // Boundary check
            if abs_pos > 0 {
                let prev = ctx.source.as_bytes()[abs_pos - 1];
                if prev.is_ascii_alphanumeric() || prev == b'_' || prev == b'.' {
                    search_from = abs_pos + 8;
                    continue;
                }
            }
            // Check if there's a string literal argument (reason string)
            if let Some(args_text) = extract_call_args(ctx.source, abs_pos + 8) {
                if args_text.contains('"') {
                    diagnostics.push(Diagnostic::new(
                        META.id,
                        "use custom errors instead of `require()` with a string literal",
                        META.default_severity,
                        abs_pos..abs_pos + 8,
                    ));
                }
            }
            search_from = abs_pos + 8;
        }

        // Also check revert("string")
        search_from = 0;
        while let Some(pos) = ctx.source[search_from..].find("revert(") {
            let abs_pos = search_from + pos;
            if abs_pos > 0 {
                let prev = ctx.source.as_bytes()[abs_pos - 1];
                if prev.is_ascii_alphanumeric() || prev == b'_' || prev == b'.' {
                    search_from = abs_pos + 7;
                    continue;
                }
            }
            if let Some(args_text) = extract_call_args(ctx.source, abs_pos + 7) {
                if args_text.contains('"') {
                    diagnostics.push(Diagnostic::new(
                        META.id,
                        "use custom errors instead of `revert()` with a string literal",
                        META.default_severity,
                        abs_pos..abs_pos + 7,
                    ));
                }
            }
            search_from = abs_pos + 7;
        }

        diagnostics
    }
}

fn extract_call_args(source: &str, start: usize) -> Option<&str> {
    let rest = &source[start..];
    let mut depth = 1;
    for (i, ch) in rest.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&rest[..i]);
                }
            }
            _ => {}
        }
    }
    None
}
