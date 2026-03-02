//! Rule: style/func-order
//!
//! Enforce function ordering within contracts per the Solidity style guide:
//! constructor, receive, fallback, external, public, internal, private.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::{ContractKind, FunctionKind, ItemKind, Visibility};
use solgrid_parser::with_parsed_ast_sequential;

static META: RuleMeta = RuleMeta {
    id: "style/func-order",
    name: "func-order",
    category: RuleCategory::Style,
    default_severity: Severity::Info,
    description: "functions should be ordered: constructor, receive, fallback, external, public, internal, private",
    fix_availability: FixAvailability::Available(FixSafety::Suggestion),
};

pub struct FuncOrderRule;

fn func_priority(kind: FunctionKind, visibility: Option<Visibility>) -> u8 {
    match kind {
        FunctionKind::Constructor => 0,
        FunctionKind::Receive => 1,
        FunctionKind::Fallback => 2,
        FunctionKind::Function | FunctionKind::Modifier => match visibility {
            Some(Visibility::External) => 3,
            Some(Visibility::Public) => 4,
            Some(Visibility::Internal) => 5,
            Some(Visibility::Private) => 6,
            None => 5, // default is internal
        },
    }
}

impl Rule for FuncOrderRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let filename = ctx.path.to_string_lossy().to_string();
        let result = with_parsed_ast_sequential(ctx.source, &filename, |source_unit| {
            let mut diagnostics = Vec::new();

            for item in source_unit.items.iter() {
                if let ItemKind::Contract(contract) = &item.kind {
                    // Skip interfaces and libraries — they have different conventions
                    if matches!(
                        contract.kind,
                        ContractKind::Interface | ContractKind::Library
                    ) {
                        continue;
                    }

                    let mut max_priority = 0u8;
                    for body_item in contract.body.iter() {
                        if let ItemKind::Function(func) = &body_item.kind {
                            // Skip modifiers for ordering purposes
                            if func.kind == FunctionKind::Modifier {
                                continue;
                            }

                            let priority = func_priority(func.kind, func.header.visibility());
                            if priority < max_priority {
                                let range = solgrid_ast::span_to_range(body_item.span);
                                let name = func
                                    .header
                                    .name
                                    .map(|n| n.as_str().to_string())
                                    .unwrap_or_else(|| func.kind.to_str().to_string());
                                diagnostics.push(Diagnostic::new(
                                    META.id,
                                    format!(
                                        "function `{name}` is out of order (expected: constructor, receive, fallback, external, public, internal, private)"
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
            }
            diagnostics
        });

        result.unwrap_or_default()
    }
}
