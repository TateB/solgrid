//! Rule: gas/tight-variable-packing
//!
//! Pack adjacent state variables to fit in fewer 32-byte storage slots.
//! Reordering state variables by size can reduce the total number of
//! storage slots used.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::ItemKind;
use solgrid_parser::with_parsed_ast_sequential;

use super::type_byte_size;

static META: RuleMeta = RuleMeta {
    id: "gas/tight-variable-packing",
    name: "tight-variable-packing",
    category: RuleCategory::Gas,
    default_severity: Severity::Info,
    description: "adjacent state variables can be packed into fewer storage slots",
    fix_availability: FixAvailability::None,
};

pub struct TightVariablePackingRule;

impl Rule for TightVariablePackingRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let filename = ctx.path.to_string_lossy().to_string();
        let result = with_parsed_ast_sequential(ctx.source, &filename, |source_unit| {
            let mut diagnostics = Vec::new();
            for item in source_unit.items.iter() {
                if let ItemKind::Contract(contract) = &item.kind {
                    // Collect state variable sizes
                    let mut var_sizes: Vec<usize> = Vec::new();
                    let mut all_known = true;

                    for body_item in contract.body.iter() {
                        if let ItemKind::Variable(var) = &body_item.kind {
                            let var_text_range = solgrid_ast::span_to_range(body_item.span);
                            let var_text = &ctx.source[var_text_range];

                            // Skip constants and immutables (not in storage)
                            if var_text.contains("constant") || var_text.contains("immutable") {
                                continue;
                            }

                            let type_range = solgrid_ast::span_to_range(var.ty.span);
                            let type_text = ctx.source[type_range].trim();

                            if let Some(size) = type_byte_size(type_text) {
                                var_sizes.push(size);
                            } else {
                                all_known = false;
                                var_sizes.push(32); // assume full slot for unknown types
                            }
                        }
                    }

                    if var_sizes.len() < 2 || !all_known {
                        continue;
                    }

                    // Calculate current slot usage
                    let current_slots = calculate_slots(&var_sizes);

                    // Calculate optimal slot usage
                    let mut sorted_sizes = var_sizes.clone();
                    sorted_sizes.sort();
                    let optimal_slots = calculate_slots(&sorted_sizes);

                    if optimal_slots < current_slots {
                        let contract_name = contract.name.as_str();
                        let name_range = solgrid_ast::span_to_range(contract.name.span);

                        diagnostics.push(Diagnostic::new(
                            META.id,
                            format!(
                                "contract `{contract_name}` state variables use {current_slots} storage slots but could use {optimal_slots} with optimal ordering"
                            ),
                            META.default_severity,
                            name_range,
                        ));
                    }
                }
            }
            diagnostics
        });
        result.unwrap_or_default()
    }
}

/// Calculate the number of 32-byte storage slots needed.
fn calculate_slots(sizes: &[usize]) -> usize {
    if sizes.is_empty() {
        return 0;
    }

    let mut slots = 0;
    let mut current_slot_used = 0;

    for &size in sizes {
        if size >= 32 {
            if current_slot_used > 0 {
                slots += 1;
                current_slot_used = 0;
            }
            slots += 1;
        } else if current_slot_used + size > 32 {
            slots += 1;
            current_slot_used = size;
        } else {
            current_slot_used += size;
        }
    }

    if current_slot_used > 0 {
        slots += 1;
    }

    slots
}
