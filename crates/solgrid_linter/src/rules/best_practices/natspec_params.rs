//! Rule: best-practices/natspec-params
//!
//! Require NatSpec `@param` for every function parameter.

use crate::context::LintContext;
use crate::rule::Rule;
use crate::rules::best_practices::natspec_helpers::{extract_natspec, parse_natspec_params};
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::{FunctionKind, ItemKind, Visibility};
use solgrid_parser::with_parsed_ast_sequential;

static META: RuleMeta = RuleMeta {
    id: "best-practices/natspec-params",
    name: "natspec-params",
    category: RuleCategory::BestPractices,
    default_severity: Severity::Warning,
    description: "NatSpec @param must exist for every function parameter",
    fix_availability: FixAvailability::None,
};

pub struct NatspecParamsRule;

impl Rule for NatspecParamsRule {
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
                            // Skip constructors, fallback, receive
                            if matches!(
                                func.kind,
                                FunctionKind::Constructor
                                    | FunctionKind::Fallback
                                    | FunctionKind::Receive
                            ) {
                                continue;
                            }

                            // Only check public and external functions
                            let is_public_or_external = matches!(
                                func.header.visibility(),
                                Some(Visibility::Public) | Some(Visibility::External)
                            );

                            if !is_public_or_external {
                                continue;
                            }

                            // Skip functions with no parameters
                            if func.header.parameters.is_empty() {
                                continue;
                            }

                            let span_start = solgrid_ast::span_to_range(body_item.span).start;
                            let natspec = match extract_natspec(ctx.source, span_start) {
                                Some(n) => n,
                                None => continue, // No NatSpec at all — handled by use-natspec rule
                            };

                            let documented_params = parse_natspec_params(&natspec);

                            // Check each function parameter has a matching @param
                            for param in func.header.parameters.iter() {
                                if let Some(name) = param.name {
                                    let param_name = name.as_str();
                                    if !documented_params.iter().any(|p| p == param_name) {
                                        let range = solgrid_ast::item_name_range(body_item);
                                        let func_name = func
                                            .header
                                            .name
                                            .map(|n| n.as_str().to_string())
                                            .unwrap_or_else(|| func.kind.to_str().to_string());
                                        diagnostics.push(Diagnostic::new(
                                            META.id,
                                            format!(
                                                "function `{func_name}` is missing NatSpec @param for `{param_name}`"
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
            }
            diagnostics
        });

        result.unwrap_or_default()
    }
}
