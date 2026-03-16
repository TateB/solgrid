//! Rule: security/unchecked-transfer
//!
//! Flag ERC20 `.transfer()` and `.transferFrom()` calls whose return value
//! is not checked. Many ERC20 tokens return a boolean that must be verified;
//! ignoring it can silently fail transfers.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;

static META: RuleMeta = RuleMeta {
    id: "security/unchecked-transfer",
    name: "unchecked-transfer",
    category: RuleCategory::Security,
    default_severity: Severity::Warning,
    description: "ERC20 transfer return value should be checked",
    fix_availability: FixAvailability::None,
};

pub struct UncheckedTransferRule;

impl Rule for UncheckedTransferRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // Flag .transferFrom( — almost always ERC20
        find_unchecked_calls(ctx, ".transferFrom(", &mut diagnostics);

        // Flag .transfer( only when it looks like ERC20 (has a comma indicating
        // two arguments: `token.transfer(to, amount)`)
        // We skip ETH-style `.transfer(amount)` which has no comma before the closing paren.
        find_unchecked_transfer_two_arg(ctx, &mut diagnostics);

        diagnostics.sort_by_key(|d| d.span.start);
        diagnostics
    }
}

/// Search for `pattern` (e.g. `.transferFrom(`) as a standalone expression
/// statement — i.e. not wrapped in `require(`, `assert(`, or assigned to a
/// variable with `=`.
fn find_unchecked_calls(ctx: &LintContext<'_>, pattern: &str, diagnostics: &mut Vec<Diagnostic>) {
    let source = ctx.source;
    let mut search_from = 0;
    while let Some(pos) = source[search_from..].find(pattern) {
        let abs_pos = search_from + pos;
        if !ctx.is_in_comment_or_string(abs_pos) && is_unchecked_usage(source, abs_pos) {
            let method = pattern.trim_start_matches('.').trim_end_matches('(');
            diagnostics.push(Diagnostic::new(
                META.id,
                format!(
                    "return value of `.{method}()` is not checked; use `require` or check the bool return"
                ),
                META.default_severity,
                abs_pos..abs_pos + pattern.len(),
            ));
        }
        search_from = abs_pos + pattern.len();
    }
}

/// Flag `.transfer(` that has two arguments (comma present) — likely ERC20,
/// not ETH transfer.
fn find_unchecked_transfer_two_arg(ctx: &LintContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    let source = ctx.source;
    let pattern = ".transfer(";
    let mut search_from = 0;
    while let Some(pos) = source[search_from..].find(pattern) {
        let abs_pos = search_from + pos;
        search_from = abs_pos + pattern.len();

        if ctx.is_in_comment_or_string(abs_pos) {
            continue;
        }

        // Check if this is a two-argument call by looking for a comma before
        // the matching closing paren.
        let after = &source[abs_pos + pattern.len()..];
        if has_comma_before_close_paren(after) && is_unchecked_usage(source, abs_pos) {
            diagnostics.push(Diagnostic::new(
                META.id,
                "return value of `.transfer()` is not checked; use `require` or check the bool return",
                META.default_severity,
                abs_pos..abs_pos + pattern.len(),
            ));
        }
    }
}

/// Check whether the text after the opening `(` contains a comma before the
/// matching `)`. This is a simple heuristic that handles one level of nesting.
fn has_comma_before_close_paren(s: &str) -> bool {
    let mut depth = 1u32;
    for ch in s.chars() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return false;
                }
            }
            ',' if depth == 1 => return true,
            _ => {}
        }
    }
    false
}

/// Heuristic: a call is "unchecked" if the line containing it does not have
/// `require(`, `assert(`, or `=` before the dot-call on the same line.
fn is_unchecked_usage(source: &str, pos: usize) -> bool {
    let before = &source[..pos];
    let line_start = before.rfind('\n').map(|p| p + 1).unwrap_or(0);
    let line_prefix = &source[line_start..pos];
    let trimmed = line_prefix.trim();

    // If the line contains require/assert wrapping, it's checked
    if trimmed.contains("require(") || trimmed.contains("assert(") {
        return false;
    }

    // If there's an assignment (bool success = ...), it's checked
    if trimmed.contains('=') {
        return false;
    }

    // If wrapped in an if-condition, it's likely checked
    if trimmed.starts_with("if") && trimmed.contains('(') {
        return false;
    }

    true
}
