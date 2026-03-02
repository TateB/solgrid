//! Rule: gas/indexed-events
//!
//! Index event parameters for cheaper filtering. Up to 3 parameters can be
//! indexed per event. Indexed value-type parameters (address, uint, bool, etc.)
//! enable efficient off-chain filtering without scanning all events.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;

static META: RuleMeta = RuleMeta {
    id: "gas/indexed-events",
    name: "indexed-events",
    category: RuleCategory::Gas,
    default_severity: Severity::Info,
    description: "event parameters should be `indexed` for cheaper filtering (up to 3)",
    fix_availability: FixAvailability::Available(FixSafety::Suggestion),
};

pub struct IndexedEventsRule;

impl Rule for IndexedEventsRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // Find event declarations
        let mut search_from = 0;
        while let Some(pos) = ctx.source[search_from..].find("event ") {
            let abs_pos = search_from + pos;

            // Check word boundary before
            if abs_pos > 0 {
                let prev = ctx.source.as_bytes()[abs_pos - 1];
                if prev.is_ascii_alphanumeric() || prev == b'_' {
                    search_from = abs_pos + 6;
                    continue;
                }
            }

            // Find the event parameters (everything between parentheses)
            let after_event = &ctx.source[abs_pos..];
            if let Some(paren_open) = after_event.find('(') {
                let paren_start = abs_pos + paren_open;
                if let Some(paren_end) = find_matching_paren(ctx.source, paren_start) {
                    let params_text = &ctx.source[paren_start + 1..paren_end];

                    // Count existing indexed params
                    let indexed_count = params_text.matches("indexed").count();

                    if indexed_count < 3 && !params_text.trim().is_empty() {
                        // Parse individual parameters
                        let params: Vec<&str> = params_text.split(',').collect();
                        let mut remaining_slots = 3 - indexed_count;

                        for param in &params {
                            let param_trimmed = param.trim();
                            if param_trimmed.is_empty() || remaining_slots == 0 {
                                continue;
                            }

                            // Skip already indexed
                            if param_trimmed.contains("indexed") {
                                continue;
                            }

                            // Get the type (first word)
                            let type_name = param_trimmed.split_whitespace().next().unwrap_or("");

                            // Only suggest indexing for value types
                            if is_indexable_type(type_name) {
                                let param_offset =
                                    (param.as_ptr() as usize) - (ctx.source.as_ptr() as usize);
                                let param_len = param.trim().len();
                                let trimmed_offset =
                                    param_offset + (param.len() - param.trim_start().len());

                                diagnostics.push(
                                    Diagnostic::new(
                                        META.id,
                                        format!(
                                            "event parameter `{param_trimmed}` should be `indexed` for cheaper off-chain filtering"
                                        ),
                                        META.default_severity,
                                        trimmed_offset..trimmed_offset + param_len,
                                    )
                                    .with_fix(Fix::suggestion(
                                        "Add `indexed` to event parameter".to_string(),
                                        vec![TextEdit::replace(
                                            trimmed_offset..trimmed_offset + param_len,
                                            add_indexed_to_param(param_trimmed),
                                        )],
                                    )),
                                );
                                remaining_slots -= 1;
                            }
                        }
                    }
                }
            }

            search_from = abs_pos + 6;
        }
        diagnostics
    }
}

/// Check if a Solidity type can be efficiently indexed in events.
fn is_indexable_type(type_name: &str) -> bool {
    let t = type_name.trim();
    t == "address"
        || t == "bool"
        || t.starts_with("uint")
        || (t.starts_with("int") && !t.starts_with("interface"))
        || (t.starts_with("bytes") && t.len() <= 7 && t != "bytes") // bytes1..bytes32, not bytes
}

/// Insert `indexed` keyword into a parameter declaration.
fn add_indexed_to_param(param: &str) -> String {
    // param is like "address from" or "uint256 amount"
    let parts: Vec<&str> = param.splitn(2, char::is_whitespace).collect();
    if parts.len() == 2 {
        format!("{} indexed {}", parts[0], parts[1].trim())
    } else {
        // Just type, no name
        format!("{param} indexed")
    }
}

fn find_matching_paren(source: &str, open_pos: usize) -> Option<usize> {
    let mut depth = 0;
    for (i, ch) in source[open_pos..].char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open_pos + i);
                }
            }
            _ => {}
        }
    }
    None
}
