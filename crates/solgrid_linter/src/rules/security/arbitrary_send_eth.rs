//! Rule: security/arbitrary-send-eth
//!
//! Flag ETH-sending patterns (`.send(`, `.transfer(`, `.call{value`) that may
//! send Ether to a user-controlled address. This is a heuristic warning — these
//! patterns are sometimes intentional, but can be dangerous when the destination
//! address is attacker-controlled.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;

static META: RuleMeta = RuleMeta {
    id: "security/arbitrary-send-eth",
    name: "arbitrary-send-eth",
    category: RuleCategory::Security,
    default_severity: Severity::Warning,
    description: "ETH send to potentially user-controlled address",
    fix_availability: FixAvailability::None,
};

pub struct ArbitrarySendEthRule;

impl Rule for ArbitrarySendEthRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // Flag .send( — address.send(amount)
        find_eth_send_pattern(
            ctx.source,
            ".send(",
            "use of `.send()` to transfer ETH; ensure the recipient address is trusted",
            &mut diagnostics,
        );

        // Flag .transfer( — address.transfer(amount)
        // Only flag single-argument .transfer( (ETH transfer), not two-argument (ERC20)
        find_eth_transfer_pattern(ctx.source, &mut diagnostics);

        // Flag .call{value — address.call{value: ...}(...)
        find_eth_send_pattern(
            ctx.source,
            ".call{value",
            "use of `.call{value}` to transfer ETH; ensure the recipient address is trusted",
            &mut diagnostics,
        );

        diagnostics.sort_by_key(|d| d.span.start);
        diagnostics
    }
}

/// Search for a pattern and emit a diagnostic for each occurrence not inside
/// a comment or string.
fn find_eth_send_pattern(
    source: &str,
    pattern: &str,
    message: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut search_from = 0;
    while let Some(pos) = source[search_from..].find(pattern) {
        let abs_pos = search_from + pos;
        if !is_in_comment_or_string(source, abs_pos) {
            diagnostics.push(Diagnostic::new(
                META.id,
                message,
                META.default_severity,
                abs_pos..abs_pos + pattern.len(),
            ));
        }
        search_from = abs_pos + pattern.len();
    }
}

/// Flag `.transfer(` only when it is a single-argument call (ETH transfer).
/// Two-argument `.transfer(to, amount)` is an ERC20 call handled by the
/// unchecked-transfer rule.
fn find_eth_transfer_pattern(source: &str, diagnostics: &mut Vec<Diagnostic>) {
    let pattern = ".transfer(";
    let mut search_from = 0;
    while let Some(pos) = source[search_from..].find(pattern) {
        let abs_pos = search_from + pos;
        search_from = abs_pos + pattern.len();

        if is_in_comment_or_string(source, abs_pos) {
            continue;
        }

        // Only flag single-argument calls (no comma before closing paren).
        let after = &source[abs_pos + pattern.len()..];
        if !has_comma_before_close_paren(after) {
            diagnostics.push(Diagnostic::new(
                META.id,
                "use of `.transfer()` to send ETH; ensure the recipient address is trusted",
                META.default_severity,
                abs_pos..abs_pos + pattern.len(),
            ));
        }
    }
}

/// Check whether the text after an opening `(` contains a comma before the
/// matching `)`.
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
    if !double_quotes.is_multiple_of(2) {
        return true;
    }
    false
}
