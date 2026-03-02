//! Rule: naming/struct-name-capwords
//!
//! Structs must use CapWords (PascalCase).

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::ItemKind;
use solgrid_parser::with_parsed_ast_sequential;

static META: RuleMeta = RuleMeta {
    id: "naming/struct-name-capwords",
    name: "struct-name-capwords",
    category: RuleCategory::Naming,
    default_severity: Severity::Warning,
    description: "struct name should use CapWords (PascalCase)",
    fix_availability: FixAvailability::None,
};

pub struct StructNameCapwordsRule;

impl Rule for StructNameCapwordsRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let filename = ctx.path.to_string_lossy().to_string();
        let result = with_parsed_ast_sequential(ctx.source, &filename, |source_unit| {
            let mut diagnostics = Vec::new();

            for item in source_unit.items.iter() {
                // Check top-level structs
                if let ItemKind::Struct(s) = &item.kind {
                    let name = s.name.as_str();
                    if !solgrid_ast::is_pascal_case(name) {
                        let range = solgrid_ast::span_to_range(s.name.span);
                        diagnostics.push(Diagnostic::new(
                            META.id,
                            format!("struct name `{name}` should use CapWords (PascalCase)"),
                            META.default_severity,
                            range,
                        ));
                    }
                }

                // Check structs inside contracts
                if let ItemKind::Contract(contract) = &item.kind {
                    for body_item in contract.body.iter() {
                        if let ItemKind::Struct(s) = &body_item.kind {
                            let name = s.name.as_str();
                            if !solgrid_ast::is_pascal_case(name) {
                                let range = solgrid_ast::span_to_range(s.name.span);
                                diagnostics.push(Diagnostic::new(
                                    META.id,
                                    format!(
                                        "struct name `{name}` should use CapWords (PascalCase)"
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
