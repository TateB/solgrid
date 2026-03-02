//! Rule: best-practices/no-unused-state
//!
//! Detect unused state variables.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::{ItemKind, Visibility};
use solgrid_parser::with_parsed_ast_sequential;

static META: RuleMeta = RuleMeta {
    id: "best-practices/no-unused-state",
    name: "no-unused-state",
    category: RuleCategory::BestPractices,
    default_severity: Severity::Warning,
    description: "state variable is declared but never used",
    fix_availability: FixAvailability::None,
};

pub struct NoUnusedStateRule;

impl Rule for NoUnusedStateRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let filename = ctx.path.to_string_lossy().to_string();
        let result = with_parsed_ast_sequential(ctx.source, &filename, |source_unit| {
            let mut diagnostics = Vec::new();

            for item in source_unit.items.iter() {
                if let ItemKind::Contract(contract) = &item.kind {
                    // Collect state variable declarations (owned strings for lifetimes)
                    let mut state_vars: Vec<(String, std::ops::Range<usize>)> = Vec::new();

                    for body_item in contract.body.iter() {
                        if let ItemKind::Variable(var) = &body_item.kind {
                            // Skip public variables (they have auto-generated getters)
                            if matches!(var.visibility, Some(Visibility::Public)) {
                                continue;
                            }

                            if let Some(name_ident) = var.name {
                                let name = name_ident.as_str().to_string();
                                // Skip variables starting with _ (intentionally unused)
                                if name.starts_with('_') {
                                    continue;
                                }
                                let range = solgrid_ast::span_to_range(name_ident.span);
                                state_vars.push((name, range));
                            }
                        }
                    }

                    // Check if each state variable is used elsewhere in the source
                    for (name, decl_range) in &state_vars {
                        if !is_var_used_elsewhere(ctx.source, name, decl_range) {
                            diagnostics.push(Diagnostic::new(
                                META.id,
                                format!("state variable `{name}` is declared but never used"),
                                META.default_severity,
                                decl_range.clone(),
                            ));
                        }
                    }
                }
            }

            diagnostics
        });

        result.unwrap_or_default()
    }
}

/// Check if a variable name appears in the source text outside of its declaration range.
fn is_var_used_elsewhere(source: &str, name: &str, decl_range: &std::ops::Range<usize>) -> bool {
    let mut search_from = 0;
    while let Some(pos) = source[search_from..].find(name) {
        let abs_pos = search_from + pos;

        // Skip if this occurrence is the declaration itself
        if abs_pos >= decl_range.start && abs_pos < decl_range.end {
            search_from = abs_pos + name.len();
            continue;
        }

        // Check word boundaries
        let before_ok = abs_pos == 0
            || !source.as_bytes()[abs_pos - 1].is_ascii_alphanumeric()
                && source.as_bytes()[abs_pos - 1] != b'_';

        let after_pos = abs_pos + name.len();
        let after_ok = after_pos >= source.len()
            || !source.as_bytes()[after_pos].is_ascii_alphanumeric()
                && source.as_bytes()[after_pos] != b'_';

        if before_ok && after_ok {
            return true;
        }

        search_from = abs_pos + name.len();
    }
    false
}
