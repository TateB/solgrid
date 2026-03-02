//! Rule: gas/named-return-values
//!
//! Named return values avoid extra stack variable allocation and can save gas.
//! Instead of `returns (uint256)`, use `returns (uint256 amount)`.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::ItemKind;
use solgrid_parser::with_parsed_ast_sequential;

static META: RuleMeta = RuleMeta {
    id: "gas/named-return-values",
    name: "named-return-values",
    category: RuleCategory::Gas,
    default_severity: Severity::Info,
    description: "use named return values to save gas on stack variable allocation",
    fix_availability: FixAvailability::None,
};

pub struct NamedReturnValuesRule;

impl Rule for NamedReturnValuesRule {
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
                            // Skip if no returns
                            let returns = match &func.header.returns {
                                Some(returns) => returns,
                                None => continue,
                            };

                            if returns.is_empty() {
                                continue;
                            }

                            // Check for unnamed return parameters
                            for ret_param in returns.iter() {
                                if ret_param.name.is_none() {
                                    let ret_range = solgrid_ast::span_to_range(ret_param.span);
                                    diagnostics.push(Diagnostic::new(
                                        META.id,
                                        "use named return values to save gas on stack variable allocation",
                                        META.default_severity,
                                        ret_range,
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
