//! Rule: naming/type-name-capwords
//!
//! User-defined value types must use CapWords (PascalCase).

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::ItemKind;
use solgrid_parser::with_parsed_ast_sequential;

static META: RuleMeta = RuleMeta {
    id: "naming/type-name-capwords",
    name: "type-name-capwords",
    category: RuleCategory::Naming,
    default_severity: Severity::Warning,
    description: "user-defined value type name should use CapWords (PascalCase)",
    fix_availability: FixAvailability::None,
};

pub struct TypeNameCapwordsRule;

impl Rule for TypeNameCapwordsRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let filename = ctx.path.to_string_lossy().to_string();
        let result = with_parsed_ast_sequential(ctx.source, &filename, |source_unit| {
            let mut diagnostics = Vec::new();

            // Check top-level UDVT definitions
            for item in source_unit.items.iter() {
                if let ItemKind::Udvt(udvt) = &item.kind {
                    let name = udvt.name.as_str();
                    if !solgrid_ast::is_pascal_case(name) {
                        let range = solgrid_ast::span_to_range(udvt.name.span);
                        diagnostics.push(Diagnostic::new(
                            META.id,
                            format!("user-defined type `{name}` should use CapWords (PascalCase)"),
                            META.default_severity,
                            range,
                        ));
                    }
                }

                // Check UDVT definitions inside contracts
                if let ItemKind::Contract(contract) = &item.kind {
                    for body_item in contract.body.iter() {
                        if let ItemKind::Udvt(udvt) = &body_item.kind {
                            let name = udvt.name.as_str();
                            if !solgrid_ast::is_pascal_case(name) {
                                let range = solgrid_ast::span_to_range(udvt.name.span);
                                diagnostics.push(Diagnostic::new(
                                    META.id,
                                    format!(
                                        "user-defined type `{name}` should use CapWords (PascalCase)"
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
