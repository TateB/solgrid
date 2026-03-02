//! Rule: gas/struct-packing
//!
//! Reorder struct fields for optimal storage packing. Solidity packs struct
//! fields into 32-byte storage slots; reordering fields by size can reduce
//! the number of slots used.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::ItemKind;
use solgrid_parser::with_parsed_ast_sequential;

use super::type_byte_size;

static META: RuleMeta = RuleMeta {
    id: "gas/struct-packing",
    name: "struct-packing",
    category: RuleCategory::Gas,
    default_severity: Severity::Info,
    description: "struct fields can be reordered for more efficient storage packing",
    fix_availability: FixAvailability::None,
};

pub struct StructPackingRule;

impl Rule for StructPackingRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let filename = ctx.path.to_string_lossy().to_string();
        let result = with_parsed_ast_sequential(ctx.source, &filename, |source_unit| {
            let mut diagnostics = Vec::new();
            for item in source_unit.items.iter() {
                if let ItemKind::Contract(contract) = &item.kind {
                    for body_item in contract.body.iter() {
                        if let ItemKind::Struct(struct_def) = &body_item.kind {
                            check_struct_packing(
                                ctx.source,
                                struct_def,
                                body_item,
                                &mut diagnostics,
                            );
                        }
                    }
                }
                // Also check top-level structs
                if let ItemKind::Struct(struct_def) = &item.kind {
                    check_struct_packing(ctx.source, struct_def, item, &mut diagnostics);
                }
            }
            diagnostics
        });
        result.unwrap_or_default()
    }
}

fn check_struct_packing(
    source: &str,
    struct_def: &solgrid_parser::solar_ast::ItemStruct<'_>,
    _item: &solgrid_parser::solar_ast::Item<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Collect field sizes
    let mut field_sizes: Vec<usize> = Vec::new();
    let mut all_known = true;

    for field in struct_def.fields.iter() {
        let type_range = solgrid_ast::span_to_range(field.ty.span);
        let type_text = source[type_range].trim();
        if let Some(size) = type_byte_size(type_text) {
            field_sizes.push(size);
        } else {
            all_known = false;
            // Assume 32 bytes for unknown types (structs, mappings, etc.)
            field_sizes.push(32);
        }
    }

    if field_sizes.len() < 2 {
        return;
    }

    // Calculate current slot usage
    let current_slots = calculate_slots(&field_sizes);

    // Calculate optimal slot usage (sort by size, smaller first for better packing)
    let mut sorted_sizes = field_sizes.clone();
    sorted_sizes.sort();
    let optimal_slots = calculate_slots(&sorted_sizes);

    if optimal_slots < current_slots && all_known {
        let struct_name = struct_def.name.as_str();
        let name_range = solgrid_ast::span_to_range(struct_def.name.span);

        diagnostics.push(Diagnostic::new(
            META.id,
            format!(
                "struct `{struct_name}` uses {current_slots} storage slots but could use {optimal_slots} with optimal field ordering"
            ),
            META.default_severity,
            name_range,
        ));
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
            // Finish current partial slot if any
            if current_slot_used > 0 {
                slots += 1;
                current_slot_used = 0;
            }
            // This type takes a full slot
            slots += 1;
        } else if current_slot_used + size > 32 {
            // Doesn't fit in current slot, finish it and start a new one
            slots += 1;
            current_slot_used = size;
        } else {
            current_slot_used += size;
        }
    }

    // Count the final partial slot if any
    if current_slot_used > 0 {
        slots += 1;
    }

    slots
}
