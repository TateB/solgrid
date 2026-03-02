//! Rule: best-practices/reason-string
//!
//! Require reason strings in require/revert statements.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;

static META: RuleMeta = RuleMeta {
    id: "best-practices/reason-string",
    name: "reason-string",
    category: RuleCategory::BestPractices,
    default_severity: Severity::Warning,
    description: "provide a reason string for `require()` and `revert()`",
    fix_availability: FixAvailability::None,
};

pub struct ReasonStringRule;

impl Rule for ReasonStringRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // Check for require() without a reason string
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
            // Check if there's a comma (second argument = reason string)
            if let Some(args_text) = extract_call_args(ctx.source, abs_pos + 8) {
                if !args_text.contains(',') {
                    diagnostics.push(Diagnostic::new(
                        META.id,
                        "provide a reason string for `require()`",
                        META.default_severity,
                        abs_pos..abs_pos + 8,
                    ));
                }
            }
            search_from = abs_pos + 8;
        }

        // Check for empty revert() calls
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
            // Check if revert() has empty args
            let after = ctx.source[abs_pos + 7..].trim_start();
            if after.starts_with(')') {
                diagnostics.push(Diagnostic::new(
                    META.id,
                    "provide a reason string or custom error for `revert()`",
                    META.default_severity,
                    abs_pos..abs_pos + 7,
                ));
            }
            search_from = abs_pos + 7;
        }

        diagnostics
    }
}

/// Extract the text between balanced parentheses starting at `start`.
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
