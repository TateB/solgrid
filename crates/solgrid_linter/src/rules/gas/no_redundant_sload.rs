//! Rule: gas/no-redundant-sload
//!
//! Cache state variable reads in a local variable when the same state variable
//! is read multiple times in a function. Each read of a state variable costs
//! an SLOAD (~2100 gas for cold, ~100 for warm), while reading a local
//! variable costs only 3 gas.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::ItemKind;
use solgrid_parser::with_parsed_ast_sequential;

static META: RuleMeta = RuleMeta {
    id: "gas/no-redundant-sload",
    name: "no-redundant-sload",
    category: RuleCategory::Gas,
    default_severity: Severity::Info,
    description: "state variable read multiple times in function; cache in local variable",
    fix_availability: FixAvailability::None,
};

pub struct NoRedundantSloadRule;

impl Rule for NoRedundantSloadRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let filename = ctx.path.to_string_lossy().to_string();
        let result = with_parsed_ast_sequential(ctx.source, &filename, |source_unit| {
            let mut diagnostics = Vec::new();
            for item in source_unit.items.iter() {
                if let ItemKind::Contract(contract) = &item.kind {
                    // Collect state variable names
                    let mut state_vars: Vec<String> = Vec::new();
                    for body_item in contract.body.iter() {
                        if let ItemKind::Variable(var) = &body_item.kind {
                            if let Some(name_ident) = var.name {
                                state_vars.push(name_ident.as_str().to_string());
                            }
                        }
                    }

                    if state_vars.is_empty() {
                        continue;
                    }

                    // Check each function for multiple reads of the same state variable
                    for body_item in contract.body.iter() {
                        if let ItemKind::Function(func) = &body_item.kind {
                            if let Some(body) = &func.body {
                                let body_range = solgrid_ast::span_to_range(body.span);
                                let body_text = &ctx.source[body_range.clone()];

                                for var_name in &state_vars {
                                    let read_count = count_word_occurrences(body_text, var_name);
                                    if read_count >= 2 {
                                        // Find the first occurrence for the diagnostic span
                                        if let Some(first_pos) =
                                            find_word_position(body_text, var_name)
                                        {
                                            let abs_pos = body_range.start + first_pos;
                                            diagnostics.push(Diagnostic::new(
                                                META.id,
                                                format!(
                                                    "state variable `{var_name}` is read {read_count} times in this function; cache it in a local variable to save gas"
                                                ),
                                                META.default_severity,
                                                abs_pos..abs_pos + var_name.len(),
                                            ));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            diagnostics
        });
        result.unwrap_or_default()
    }
}

/// Count word-boundary-aware occurrences of a word in text.
fn count_word_occurrences(text: &str, word: &str) -> usize {
    let mut count = 0;
    let mut search_from = 0;
    while let Some(pos) = text[search_from..].find(word) {
        let abs_pos = search_from + pos;

        let before_ok = abs_pos == 0
            || !text.as_bytes()[abs_pos - 1].is_ascii_alphanumeric()
                && text.as_bytes()[abs_pos - 1] != b'_';

        let after_pos = abs_pos + word.len();
        let after_ok = after_pos >= text.len()
            || !text.as_bytes()[after_pos].is_ascii_alphanumeric()
                && text.as_bytes()[after_pos] != b'_';

        if before_ok && after_ok {
            count += 1;
        }
        search_from = abs_pos + word.len();
    }
    count
}

/// Find the first word-boundary-aware occurrence of a word.
fn find_word_position(text: &str, word: &str) -> Option<usize> {
    let mut search_from = 0;
    while let Some(pos) = text[search_from..].find(word) {
        let abs_pos = search_from + pos;

        let before_ok = abs_pos == 0
            || !text.as_bytes()[abs_pos - 1].is_ascii_alphanumeric()
                && text.as_bytes()[abs_pos - 1] != b'_';

        let after_pos = abs_pos + word.len();
        let after_ok = after_pos >= text.len()
            || !text.as_bytes()[after_pos].is_ascii_alphanumeric()
                && text.as_bytes()[after_pos] != b'_';

        if before_ok && after_ok {
            return Some(abs_pos);
        }
        search_from = abs_pos + word.len();
    }
    None
}
