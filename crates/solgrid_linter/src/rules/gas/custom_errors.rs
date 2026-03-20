//! Rule: gas/custom-errors
//!
//! Custom errors are cheaper than `require` with string literals because
//! strings are stored as contract bytecode, increasing deployment and
//! runtime gas costs.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;

static META: RuleMeta = RuleMeta {
    id: "gas/custom-errors",
    name: "custom-errors",
    category: RuleCategory::Gas,
    default_severity: Severity::Info,
    description: "custom errors are cheaper than `require` with string literals",
    fix_availability: FixAvailability::None,
};

pub struct GasCustomErrorsRule;

impl Rule for GasCustomErrorsRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        if ctx
            .config
            .lint
            .is_rule_enabled("best-practices/custom-errors", RuleCategory::BestPractices)
        {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();

        let mut search_from = 0;
        while let Some(pos) = ctx.source[search_from..].find("require(") {
            let abs_pos = search_from + pos;

            // Check word boundary
            if abs_pos > 0 {
                let prev = ctx.source.as_bytes()[abs_pos - 1];
                if prev.is_ascii_alphanumeric() || prev == b'_' || prev == b'.' {
                    search_from = abs_pos + 8;
                    continue;
                }
            }

            // Check if there's a string literal argument
            let args_start = abs_pos + 8;
            if let Some(args_text) = extract_call_args(ctx.source, args_start) {
                if args_text.contains('"') {
                    diagnostics.push(Diagnostic::new(
                        META.id,
                        "custom errors are cheaper than `require()` with string literals — saves deployment and runtime gas",
                        META.default_severity,
                        abs_pos..abs_pos + 8,
                    ));
                }
            }
            search_from = abs_pos + 8;
        }

        diagnostics
    }
}

fn extract_call_args(source: &str, start: usize) -> Option<&str> {
    if start >= source.len() {
        return None;
    }
    let rest = &source[start..];
    let mut depth = 1i32;
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
