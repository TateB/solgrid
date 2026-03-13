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
                    let mut max_priority = 0u8;
                    for body_item in contract.body.iter() {
                        let priority = body_item_priority(&body_item.kind);
                        if priority < max_priority {
                            let range = solgrid_ast::item_name_range(body_item);
                            let label = priority_label(priority);
                            diagnostics.push(Diagnostic::new(
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
                }
            }
            diagnostics
        });

        result.unwrap_or_default()
    }
}
