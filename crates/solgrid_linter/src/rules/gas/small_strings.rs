//! Rule: gas/small-strings
//!
//! Short strings (< 32 bytes) in `require`/`revert` save gas. Long error
//! strings are stored as additional contract bytecode and increase deployment
//! and runtime costs.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;

static META: RuleMeta = RuleMeta {
    id: "gas/small-strings",
    name: "small-strings",
    category: RuleCategory::Gas,
    default_severity: Severity::Info,
    description: "require/revert string longer than 32 bytes wastes gas",
    fix_availability: FixAvailability::None,
};

pub struct SmallStringsRule;

impl Rule for SmallStringsRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for keyword in &["require(", "revert("] {
            let mut search_from = 0;
            while let Some(pos) = ctx.source[search_from..].find(keyword) {
                let abs_pos = search_from + pos;

                // Check word boundary
                if abs_pos > 0 {
                    let prev = ctx.source.as_bytes()[abs_pos - 1];
                    if prev.is_ascii_alphanumeric() || prev == b'_' || prev == b'.' {
                        search_from = abs_pos + keyword.len();
                        continue;
                    }
                }

                // Extract the arguments
                let args_start = abs_pos + keyword.len();
                if let Some(args_text) = extract_call_args(ctx.source, args_start) {
                    // Find string literals in the args
                    if let Some(string_info) = find_long_string(args_text) {
                        let string_abs = args_start + string_info.0;
                        diagnostics.push(Diagnostic::new(
                            META.id,
                            format!(
                                "require/revert string is {} bytes; strings longer than 32 bytes waste gas — consider using custom errors",
                                string_info.1
                            ),
                            META.default_severity,
                            string_abs..string_abs + string_info.2,
                        ));
                    }
                }

                search_from = abs_pos + keyword.len();
            }
        }

        diagnostics
    }
}

/// Extract call arguments text (everything between the parentheses).
fn extract_call_args(source: &str, start: usize) -> Option<&str> {
    if start >= source.len() {
        return None;
    }
    let rest = &source[start..];
    let mut depth = 1i32; // We're already past the opening paren
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

/// Find a string literal longer than 32 bytes in the arguments.
/// Returns (offset_in_args, byte_length, total_literal_length_including_quotes).
fn find_long_string(args: &str) -> Option<(usize, usize, usize)> {
    let mut search_from = 0;
    while let Some(pos) = args[search_from..].find('"') {
        let abs_pos = search_from + pos;
        let after_quote = abs_pos + 1;
        // Find the closing quote
        if let Some(end_pos) = args[after_quote..].find('"') {
            let string_content = &args[after_quote..after_quote + end_pos];
            let byte_len = string_content.len();
            if byte_len > 32 {
                return Some((abs_pos, byte_len, end_pos + 2)); // +2 for quotes
            }
            search_from = after_quote + end_pos + 1;
        } else {
            break;
        }
    }
    None
}
