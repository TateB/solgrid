//! Rule: gas/increment-by-one
//!
//! Use `++i` instead of `i += 1` or `i = i + 1` for gas efficiency.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;

static META: RuleMeta = RuleMeta {
    id: "gas/increment-by-one",
    name: "increment-by-one",
    category: RuleCategory::Gas,
    default_severity: Severity::Info,
    description: "use `++i` instead of `i += 1` for gas efficiency",
    fix_availability: FixAvailability::Available(FixSafety::Safe),
};

pub struct IncrementByOneRule;

impl Rule for IncrementByOneRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // Find `+= 1` patterns
        let mut search_from = 0;
        while let Some(pos) = ctx.source[search_from..].find("+= 1") {
            let abs_pos = search_from + pos;

            // Check that the `1` is followed by a non-digit (not `+= 10`, `+= 100`, etc.)
            let after = abs_pos + 4;
            if after < ctx.source.len() && ctx.source.as_bytes()[after].is_ascii_digit() {
                search_from = abs_pos + 4;
                continue;
            }

            // Find the variable name before `+= 1`
            if let Some(var_name) = extract_var_before(ctx.source, abs_pos) {
                let var_start = abs_pos - var_name.len() - count_spaces_before(ctx.source, abs_pos);
                diagnostics.push(
                    Diagnostic::new(
                        META.id,
                        format!(
                            "use `++{var_name}` instead of `{var_name} += 1` for gas efficiency"
                        ),
                        META.default_severity,
                        var_start..after,
                    )
                    .with_fix(Fix::safe(
                        format!("Replace with `++{var_name}`"),
                        vec![TextEdit::replace(var_start..after, format!("++{var_name}"))],
                    )),
                );
            }
            search_from = abs_pos + 4;
        }

        // Find `-= 1` patterns
        search_from = 0;
        while let Some(pos) = ctx.source[search_from..].find("-= 1") {
            let abs_pos = search_from + pos;

            let after = abs_pos + 4;
            if after < ctx.source.len() && ctx.source.as_bytes()[after].is_ascii_digit() {
                search_from = abs_pos + 4;
                continue;
            }

            if let Some(var_name) = extract_var_before(ctx.source, abs_pos) {
                let var_start = abs_pos - var_name.len() - count_spaces_before(ctx.source, abs_pos);
                diagnostics.push(
                    Diagnostic::new(
                        META.id,
                        format!(
                            "use `--{var_name}` instead of `{var_name} -= 1` for gas efficiency"
                        ),
                        META.default_severity,
                        var_start..after,
                    )
                    .with_fix(Fix::safe(
                        format!("Replace with `--{var_name}`"),
                        vec![TextEdit::replace(var_start..after, format!("--{var_name}"))],
                    )),
                );
            }
            search_from = abs_pos + 4;
        }

        diagnostics
    }
}

/// Extract the variable name immediately before `+= 1` or `-= 1`.
fn extract_var_before(source: &str, op_pos: usize) -> Option<String> {
    // Skip whitespace before the operator
    let before = &source[..op_pos];
    let trimmed = before.trim_end();
    if trimmed.is_empty() {
        return None;
    }

    // Walk backwards to find the variable name
    let end = trimmed.len();
    let mut start = end;
    let bytes = trimmed.as_bytes();
    while start > 0 {
        let ch = bytes[start - 1];
        if ch.is_ascii_alphanumeric() || ch == b'_' || ch == b'.' || ch == b'[' || ch == b']' {
            start -= 1;
        } else {
            break;
        }
    }

    if start < end {
        Some(trimmed[start..end].to_string())
    } else {
        None
    }
}

/// Count spaces before a position.
fn count_spaces_before(source: &str, pos: usize) -> usize {
    let before = &source[..pos];
    before.len() - before.trim_end().len()
}
