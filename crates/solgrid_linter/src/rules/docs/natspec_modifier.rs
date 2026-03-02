//! Rule: docs/natspec-modifier
//!
//! Modifiers must have NatSpec documentation.

use crate::context::LintContext;
use crate::rule::Rule;
use crate::rules::best_practices::use_natspec::extract_natspec;
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::{FunctionKind, ItemKind};
use solgrid_parser::with_parsed_ast_sequential;

static META: RuleMeta = RuleMeta {
    id: "docs/natspec-modifier",
    name: "natspec-modifier",
    category: RuleCategory::Docs,
    default_severity: Severity::Info,
    description: "modifiers must have NatSpec documentation",
    fix_availability: FixAvailability::None,
};

pub struct NatspecModifierRule;

impl Rule for NatspecModifierRule {
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
                        if let ItemKind::Function(func) = &body_item.kind {
                            if func.kind != FunctionKind::Modifier {
                                continue;
                            }

                            let name = func
                                .header
                                .name
                                .map(|n| n.as_str().to_string())
                                .unwrap_or_else(|| "unnamed".to_string());

                            let span_start = solgrid_ast::span_to_range(body_item.span).start;
                            if extract_natspec(ctx.source, span_start).is_none() {
                                let range = solgrid_ast::span_to_range(body_item.span);
                                diagnostics.push(Diagnostic::new(
                                    META.id,
                                    format!(
                                        "modifier `{name}` is missing NatSpec documentation"
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
