//! Rule: gas/use-immutable
//!
//! State variables assigned only in the constructor should be declared as
//! `immutable`. Immutable variables are stored in contract bytecode rather
//! than storage, avoiding SLOAD gas costs.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::{FunctionKind, ItemKind};
use solgrid_parser::with_parsed_ast_sequential;

static META: RuleMeta = RuleMeta {
    id: "gas/use-immutable",
    name: "use-immutable",
    category: RuleCategory::Gas,
    default_severity: Severity::Info,
    description: "state variable assigned only in constructor should be `immutable`",
    fix_availability: FixAvailability::Available(FixSafety::Suggestion),
};

pub struct UseImmutableRule;

impl Rule for UseImmutableRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let filename = ctx.path.to_string_lossy().to_string();
        let result = with_parsed_ast_sequential(ctx.source, &filename, |source_unit| {
            let mut diagnostics = Vec::new();
            for item in source_unit.items.iter() {
                if let ItemKind::Contract(contract) = &item.kind {
                    // Collect state variables that are candidates
                    let mut candidates: Vec<(
                        String,
                        std::ops::Range<usize>,
                        std::ops::Range<usize>,
                    )> = Vec::new();

                    for body_item in contract.body.iter() {
                        if let ItemKind::Variable(var) = &body_item.kind {
                            let var_range = solgrid_ast::span_to_range(body_item.span);
                            let var_text = &ctx.source[var_range.clone()];

                            // Skip if already constant or immutable
                            if var_text.contains("constant") || var_text.contains("immutable") {
                                continue;
                            }

                            // Skip if has an initializer (use-constant handles those)
                            if var.initializer.is_some() {
                                continue;
                            }

                            // Get variable name
                            if let Some(name_ident) = var.name {
                                let var_name = name_ident.as_str().to_string();
                                let name_range = solgrid_ast::span_to_range(name_ident.span);
                                candidates.push((var_name, name_range, var_range));
                            }
                        }
                    }

                    if candidates.is_empty() {
                        continue;
                    }

                    // Find constructor body text
                    let mut constructor_text: Option<String> = None;
                    let mut function_texts: Vec<String> = Vec::new();

                    for body_item in contract.body.iter() {
                        if let ItemKind::Function(func) = &body_item.kind {
                            if let Some(body) = &func.body {
                                let body_range = solgrid_ast::span_to_range(body.span);
                                let body_text = ctx.source[body_range].to_string();

                                if matches!(func.kind, FunctionKind::Constructor) {
                                    constructor_text = Some(body_text);
                                } else {
                                    function_texts.push(body_text);
                                }
                            }
                        }
                    }

                    // Check each candidate
                    let constructor = constructor_text.as_deref().unwrap_or("");
                    for (var_name, name_range, _var_range) in &candidates {
                        let assigned_in_constructor = is_assigned_in_text(constructor, var_name);
                        let assigned_elsewhere = function_texts
                            .iter()
                            .any(|ft| is_assigned_in_text(ft, var_name));

                        if assigned_in_constructor && !assigned_elsewhere {
                            diagnostics.push(
                                Diagnostic::new(
                                    META.id,
                                    format!(
                                        "state variable `{var_name}` is only assigned in the constructor; declare it as `immutable` to save gas"
                                    ),
                                    META.default_severity,
                                    name_range.clone(),
                                ),
                            );
                        }
                    }
                }
            }
            diagnostics
        });
        result.unwrap_or_default()
    }
}

fn is_assigned_in_text(text: &str, var_name: &str) -> bool {
    let assign_pattern = format!("{var_name} =");
    let mut search_from = 0;
    while let Some(pos) = text[search_from..].find(&assign_pattern) {
        let abs_pos = search_from + pos;

        // Check word boundary before
        if abs_pos > 0 {
            let prev = text.as_bytes()[abs_pos - 1];
            if prev.is_ascii_alphanumeric() || prev == b'_' || prev == b'.' {
                search_from = abs_pos + assign_pattern.len();
                continue;
            }
        }

        // Check it's not `==`
        let after_pattern = abs_pos + assign_pattern.len();
        if after_pattern < text.len() && text.as_bytes()[after_pattern] == b'=' {
            search_from = abs_pos + assign_pattern.len();
            continue;
        }

        return true;
    }
    false
}
