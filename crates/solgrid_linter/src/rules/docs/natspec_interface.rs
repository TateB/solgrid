//! Rule: docs/natspec-interface
//!
//! Public interfaces must have NatSpec on all functions.

use crate::context::LintContext;
use crate::rule::Rule;
use crate::rules::best_practices::use_natspec::extract_natspec;
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::{ContractKind, ItemKind};
use solgrid_parser::with_parsed_ast_sequential;

static META: RuleMeta = RuleMeta {
    id: "docs/natspec-interface",
    name: "natspec-interface",
    category: RuleCategory::Docs,
    default_severity: Severity::Info,
    description: "interface functions must have NatSpec documentation",
    fix_availability: FixAvailability::None,
};

pub struct NatspecInterfaceRule;

impl Rule for NatspecInterfaceRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let filename = ctx.path.to_string_lossy().to_string();
        let result = with_parsed_ast_sequential(ctx.source, &filename, |source_unit| {
            let mut diagnostics = Vec::new();

            for item in source_unit.items.iter() {
                if let ItemKind::Contract(contract) = &item.kind {
                    if contract.kind != ContractKind::Interface {
                        continue;
                    }

                    let iface_name = contract.name.as_str().to_string();

                    for body_item in contract.body.iter() {
                        if let ItemKind::Function(func) = &body_item.kind {
                            let func_name = func
                                .header
                                .name
                                .map(|n| n.as_str().to_string())
                                .unwrap_or_else(|| func.kind.to_str().to_string());

                            let span_start = solgrid_ast::span_to_range(body_item.span).start;
                            if extract_natspec(ctx.source, span_start).is_none() {
                                let range = solgrid_ast::item_name_range(body_item);
                                diagnostics.push(Diagnostic::new(
                                    META.id,
                                    format!(
                                        "interface `{iface_name}` function `{func_name}` is missing NatSpec documentation"
                                    ),
                                    META.default_severity,
                                    range,
                                ));
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
