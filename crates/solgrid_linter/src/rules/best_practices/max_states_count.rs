//! Rule: best-practices/max-states-count
//!
//! Flag contracts with too many state variables.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::ItemKind;
use solgrid_parser::with_parsed_ast_sequential;

const DEFAULT_MAX_STATES: usize = 15;

static META: RuleMeta = RuleMeta {
    id: "best-practices/max-states-count",
    name: "max-states-count",
    category: RuleCategory::BestPractices,
    default_severity: Severity::Warning,
    description: "contract has too many state variables",
    fix_availability: FixAvailability::None,
};

pub struct MaxStatesCountRule;

impl Rule for MaxStatesCountRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let filename = ctx.path.to_string_lossy().to_string();
        let result = with_parsed_ast_sequential(ctx.source, &filename, |source_unit| {
            let mut diagnostics = Vec::new();
            for item in source_unit.items.iter() {
                if let ItemKind::Contract(contract) = &item.kind {
                    let state_count = contract
                        .body
                        .iter()
                        .filter(|i| matches!(i.kind, ItemKind::Variable(_)))
                        .count();
                    if state_count > DEFAULT_MAX_STATES {
                        let name = contract.name.as_str();
                        let range = solgrid_ast::span_to_range(contract.name.span);
                        diagnostics.push(Diagnostic::new(
                            META.id,
                            format!(
                                "contract `{name}` has {state_count} state variables (maximum is {DEFAULT_MAX_STATES})"
                            ),
                            META.default_severity,
                            range,
                        ));
                    }
                }
            }
            diagnostics
        });

        result.unwrap_or_default()
    }
}
