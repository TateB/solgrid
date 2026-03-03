//! Rule: security/tx-origin
//!
//! Disallow `tx.origin` for authorization.
//! `tx.origin` returns the original sender of the transaction, which can
//! be exploited in phishing attacks.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;

static META: RuleMeta = RuleMeta {
    id: "security/tx-origin",
    name: "tx-origin",
    category: RuleCategory::Security,
    default_severity: Severity::Error,
    description: "use of `tx.origin` for authorization",
    fix_availability: FixAvailability::None,
};

pub struct TxOriginRule;

impl Rule for TxOriginRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        // Search for tx.origin in source
        let mut search_from = 0;
        while let Some(pos) = ctx.source[search_from..].find("tx.origin") {
            let abs_pos = search_from + pos;
            // Make sure it's not inside a comment or string
            if !is_in_comment_or_string(ctx.source, abs_pos) {
                diagnostics.push(Diagnostic::new(
                    META.id,
                    "avoid using `tx.origin` for authorization; use `msg.sender` instead",
                    META.default_severity,
                    abs_pos..abs_pos + 9,
                ));
            }
            search_from = abs_pos + 9;
        }
        diagnostics
    }
}

fn is_in_comment_or_string(source: &str, pos: usize) -> bool {
    let before = &source[..pos];
    // Check if inside a line comment
    if let Some(last_newline) = before.rfind('\n') {
        let line = &before[last_newline..];
        if line.contains("//") {
            let comment_pos = before.rfind("//").unwrap();
            if comment_pos > last_newline {
                return true;
            }
        }
    } else if before.contains("//") {
        return true;
    }
    // Check if inside a block comment
    let block_opens = before.matches("/*").count();
    let block_closes = before.matches("*/").count();
    if block_opens > block_closes {
        return true;
    }
    // Check if inside a string literal (simple heuristic)
    let double_quotes = before.matches('"').count();
    if double_quotes % 2 != 0 {
        return true;
    }
    false
}
