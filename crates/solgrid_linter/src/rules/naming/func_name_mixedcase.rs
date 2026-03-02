//! Rule: naming/func-name-mixedcase
//!
//! Functions must use mixedCase (camelCase).

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::{FunctionKind, ItemKind};
use solgrid_parser::with_parsed_ast_sequential;

static META: RuleMeta = RuleMeta {
    id: "naming/func-name-mixedcase",
    name: "func-name-mixedcase",
    category: RuleCategory::Naming,
    default_severity: Severity::Warning,
    description: "function name should use mixedCase (camelCase)",
    fix_availability: FixAvailability::None,
};

pub struct FuncNameMixedcaseRule;

impl Rule for FuncNameMixedcaseRule {
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
                            // Only check regular functions (not constructors, fallback, receive)
                            if func.kind != FunctionKind::Function {
                                continue;
                            }
                            if let Some(name_ident) = func.header.name {
                                let name = name_ident.as_str();
                                if !solgrid_ast::is_camel_case(name) {
                                    let range =
                                        solgrid_ast::span_to_range(name_ident.span);
                                    diagnostics.push(Diagnostic::new(
                                        META.id,
                                        format!(
                                            "function name `{name}` should use mixedCase (camelCase)"
                                        ),
                                        META.default_severity,
                                        range,
                                    ));
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
