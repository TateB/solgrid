//! Rule: naming/library-name-capwords
//!
//! Libraries must use CapWords (PascalCase).

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::{ContractKind, ItemKind};
use solgrid_parser::with_parsed_ast_sequential;

static META: RuleMeta = RuleMeta {
    id: "naming/library-name-capwords",
    name: "library-name-capwords",
    category: RuleCategory::Naming,
    default_severity: Severity::Warning,
    description: "library name should use CapWords (PascalCase)",
    fix_availability: FixAvailability::None,
};

pub struct LibraryNameCapwordsRule;

impl Rule for LibraryNameCapwordsRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let filename = ctx.path.to_string_lossy().to_string();
        let result = with_parsed_ast_sequential(ctx.source, &filename, |source_unit| {
            let mut diagnostics = Vec::new();
            for item in source_unit.items.iter() {
                if let ItemKind::Contract(contract) = &item.kind {
                    if contract.kind == ContractKind::Library {
                        let name = contract.name.as_str();
                        if !solgrid_ast::is_pascal_case(name) {
                            let range = solgrid_ast::span_to_range(contract.name.span);
                            diagnostics.push(Diagnostic::new(
                                META.id,
                                format!(
                                    "library name `{name}` should use CapWords (PascalCase)"
                                ),
                                META.default_severity,
                                range,
                            ));
                        }
                    }
                }
            }
            diagnostics
        });

        result.unwrap_or_default()
    }
}
