//! Rule: naming/error-name-capwords
//!
//! Custom errors must use CapWords (PascalCase).

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::ItemKind;
use solgrid_parser::with_parsed_ast_sequential;

static META: RuleMeta = RuleMeta {
    id: "naming/error-name-capwords",
    name: "error-name-capwords",
    category: RuleCategory::Naming,
    default_severity: Severity::Warning,
    description: "custom error name should use CapWords (PascalCase)",
    fix_availability: FixAvailability::None,
};

pub struct ErrorNameCapwordsRule;

impl Rule for ErrorNameCapwordsRule {
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
                        if let ItemKind::Error(er) = &body_item.kind {
                            let name = er.name.as_str();
                            if !solgrid_ast::is_pascal_case(name) {
                                let range = solgrid_ast::span_to_range(er.name.span);
                                diagnostics.push(Diagnostic::new(
                                    META.id,
                                    format!(
                                        "custom error name `{name}` should use CapWords (PascalCase)"
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
