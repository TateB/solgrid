//! Rule: naming/enum-name-capwords
//!
//! Enums must use CapWords (PascalCase).

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::ItemKind;
use solgrid_parser::with_parsed_ast_sequential;

static META: RuleMeta = RuleMeta {
    id: "naming/enum-name-capwords",
    name: "enum-name-capwords",
    category: RuleCategory::Naming,
    default_severity: Severity::Warning,
    description: "enum name should use CapWords (PascalCase)",
    fix_availability: FixAvailability::None,
};

pub struct EnumNameCapwordsRule;

impl Rule for EnumNameCapwordsRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let filename = ctx.path.to_string_lossy().to_string();
        let result = with_parsed_ast_sequential(ctx.source, &filename, |source_unit| {
            let mut diagnostics = Vec::new();

            for item in source_unit.items.iter() {
                // Check top-level enums
                if let ItemKind::Enum(e) = &item.kind {
                    let name = e.name.as_str();
                    if !solgrid_ast::is_pascal_case(name) {
                        let range = solgrid_ast::span_to_range(e.name.span);
                        diagnostics.push(Diagnostic::new(
                            META.id,
                            format!("enum name `{name}` should use CapWords (PascalCase)"),
                            META.default_severity,
                            range,
                        ));
                    }
                }

                // Check enums inside contracts
                if let ItemKind::Contract(contract) = &item.kind {
                    for body_item in contract.body.iter() {
                        if let ItemKind::Enum(e) = &body_item.kind {
                            let name = e.name.as_str();
                            if !solgrid_ast::is_pascal_case(name) {
                                let range = solgrid_ast::span_to_range(e.name.span);
                                diagnostics.push(Diagnostic::new(
                                    META.id,
                                    format!(
                                        "enum name `{name}` should use CapWords (PascalCase)"
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
