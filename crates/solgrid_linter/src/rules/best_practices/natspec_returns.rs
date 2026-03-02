//! Rule: best-practices/natspec-returns
//!
//! Require NatSpec `@return` for every return value.

use crate::context::LintContext;
use crate::rule::Rule;
use crate::rules::best_practices::use_natspec::{count_natspec_returns, extract_natspec};
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::{FunctionKind, ItemKind, Visibility};
use solgrid_parser::with_parsed_ast_sequential;

static META: RuleMeta = RuleMeta {
    id: "best-practices/natspec-returns",
    name: "natspec-returns",
    category: RuleCategory::BestPractices,
    default_severity: Severity::Warning,
    description: "NatSpec @return must exist for every return value",
    fix_availability: FixAvailability::None,
};

pub struct NatspecReturnsRule;

impl Rule for NatspecReturnsRule {
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

                            // Skip functions with no return values
                            let return_count = match &func.header.returns {
                                Some(returns) => returns.len(),
                                None => continue,
                            };

                            let span_start = solgrid_ast::span_to_range(body_item.span).start;
                            let natspec = match extract_natspec(ctx.source, span_start) {
                                Some(n) => n,
                                None => continue, // No NatSpec at all — handled by use-natspec rule
                            };

                            let documented_returns = count_natspec_returns(&natspec);

                            if documented_returns < return_count {
                                let func_name = func
                                    .header
                                    .name
                                    .map(|n| n.as_str().to_string())
                                    .unwrap_or_else(|| func.kind.to_str().to_string());
                                let range = solgrid_ast::span_to_range(body_item.span);
                                diagnostics.push(Diagnostic::new(
                                    META.id,
                                    format!(
                                        "function `{func_name}` has {return_count} return value(s) but only {documented_returns} @return tag(s)"
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
