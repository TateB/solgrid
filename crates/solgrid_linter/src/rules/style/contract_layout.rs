//! Rule: style/contract-layout
//!
//! Enforce ordering within contracts:
//! type declarations, state variables, events, errors, modifiers, functions.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::{FunctionKind, ItemKind};
use solgrid_parser::with_parsed_ast_sequential;

static META: RuleMeta = RuleMeta {
    id: "style/contract-layout",
    name: "contract-layout",
    category: RuleCategory::Style,
    default_severity: Severity::Info,
    description: "contract members should be ordered: type declarations, state variables, events, errors, modifiers, functions",
    fix_availability: FixAvailability::Available(FixSafety::Suggestion),
};

pub struct ContractLayoutRule;

fn body_item_priority(kind: &ItemKind<'_>) -> u8 {
    match kind {
        ItemKind::Udvt(_) | ItemKind::Struct(_) | ItemKind::Enum(_) => 0, // type declarations
        ItemKind::Variable(_) => 1,                                       // state variables
        ItemKind::Event(_) => 2,                                          // events
        ItemKind::Error(_) => 3,                                          // custom errors
        ItemKind::Function(f) if f.kind == FunctionKind::Modifier => 4,   // modifiers
        ItemKind::Function(_) => 5,                                       // functions
        ItemKind::Using(_) => 0,                                          // using-for with types
        _ => 6,
    }
}

fn priority_label(priority: u8) -> &'static str {
    match priority {
        0 => "type declaration",
        1 => "state variable",
        2 => "event",
        3 => "error",
        4 => "modifier",
        5 => "function",
        _ => "declaration",
    }
}

impl Rule for ContractLayoutRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let filename = ctx.path.to_string_lossy().to_string();
        let result = with_parsed_ast_sequential(ctx.source, &filename, |source_unit| {
            let mut diagnostics = Vec::new();

            for item in source_unit.items.iter() {
                if let ItemKind::Contract(contract) = &item.kind {
                    let body_items: Vec<_> = contract.body.iter().collect();
                    if body_items.is_empty() {
                        continue;
                    }

                    // First pass: detect violations and collect diagnostics
                    let mut max_priority = 0u8;
                    let mut violation_diags = Vec::new();
                    for body_item in body_items.iter() {
                        let priority = body_item_priority(&body_item.kind);
                        if priority < max_priority {
                            let range = solgrid_ast::item_name_range(body_item);
                            let label = priority_label(priority);
                            violation_diags.push(Diagnostic::new(
                                META.id,
                                format!(
                                    "{label} should appear before higher-priority members (expected order: types, state variables, events, errors, modifiers, functions)"
                                ),
                                META.default_severity,
                                range,
                            ));
                        } else {
                            max_priority = priority;
                        }
                    }

                    if violation_diags.is_empty() {
                        continue;
                    }

                    // Build the fix: chunk-based reordering
                    let contract_range = solgrid_ast::span_to_range(item.span);
                    let contract_text = &ctx.source[contract_range.clone()];
                    if let (Some(brace_open), Some(brace_close)) =
                        (contract_text.find('{'), contract_text.rfind('}'))
                    {
                        let body_start = contract_range.start + brace_open + 1;
                        let body_end = contract_range.start + brace_close;

                        // Build chunks: each chunk is (priority, original_index, text from prev_end to item_end)
                        let mut chunks: Vec<(u8, usize, String)> = Vec::new();
                        for (idx, body_item) in body_items.iter().enumerate() {
                            let item_range = solgrid_ast::span_to_range(body_item.span);
                            let priority = body_item_priority(&body_item.kind);
                            let prev_end = if idx == 0 {
                                body_start
                            } else {
                                solgrid_ast::span_to_range(body_items[idx - 1].span).end
                            };
                            let chunk = ctx.source[prev_end..item_range.end].to_string();
                            chunks.push((priority, idx, chunk));
                        }

                        // Trailing text after last item (whitespace before closing brace)
                        let last_end =
                            solgrid_ast::span_to_range(body_items.last().unwrap().span).end;
                        let trailing = &ctx.source[last_end..body_end];

                        // Sort by (priority, original_index) for stable ordering
                        chunks.sort_by_key(|&(p, i, _)| (p, i));

                        let replacement: String = chunks
                            .iter()
                            .map(|(_, _, text)| text.as_str())
                            .collect::<String>()
                            + trailing;

                        let fix = Fix::suggestion(
                            "Reorder contract members",
                            vec![TextEdit::replace(body_start..body_end, replacement)],
                        );

                        // Attach fix to the first diagnostic only
                        violation_diags[0] = violation_diags[0].clone().with_fix(fix);
                    }

                    diagnostics.extend(violation_diags);
                }
            }
            diagnostics
        });

        result.unwrap_or_default()
    }
}
