//! Rule: naming/contract-name-capwords
//!
//! Contracts must use PascalCase (CapWords).

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;
use solgrid_parser::with_parsed_ast_sequential;

static META: RuleMeta = RuleMeta {
    id: "naming/contract-name-capwords",
    name: "contract-name-capwords",
    category: RuleCategory::Naming,
    default_severity: Severity::Warning,
    description: "contract name should use CapWords (PascalCase)",
    fix_availability: FixAvailability::None,
};

pub struct ContractNameCapwordsRule;

impl Rule for ContractNameCapwordsRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let filename = ctx.path.to_string_lossy().to_string();
        let result = with_parsed_ast_sequential(ctx.source, &filename, |source_unit| {
            let mut diagnostics = Vec::new();
            for item in source_unit.items.iter() {
                if let solgrid_parser::solar_ast::ItemKind::Contract(contract) = &item.kind {
                    let name = contract.name.as_str();
                    if !solgrid_ast::is_pascal_case(name) {
                        let range = solgrid_ast::span_to_range(contract.name.span);
                        diagnostics.push(Diagnostic::new(
                            META.id,
                            format!("contract name `{name}` should use CapWords (PascalCase)"),
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
