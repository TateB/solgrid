//! Rule: naming/const-name-snakecase
//!
//! Constants must use UPPER_SNAKE_CASE.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::{ItemKind, VarMut};
use solgrid_parser::with_parsed_ast_sequential;

static META: RuleMeta = RuleMeta {
    id: "naming/const-name-snakecase",
    name: "const-name-snakecase",
    category: RuleCategory::Naming,
    default_severity: Severity::Warning,
    description: "constant name should use UPPER_SNAKE_CASE",
    fix_availability: FixAvailability::None,
};

pub struct ConstNameSnakecaseRule;

impl Rule for ConstNameSnakecaseRule {
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
                        if let ItemKind::Variable(var) = &body_item.kind {
                            // Check constants and immutables
                            let is_constant = var.mutability == Some(VarMut::Constant);
                            let is_immutable = var.mutability == Some(VarMut::Immutable);
                            if is_constant || is_immutable {
                                if let Some(name_ident) = var.name {
                                    let name = name_ident.as_str();
                                    if !solgrid_ast::is_upper_snake_case(name) {
                                        let range = solgrid_ast::span_to_range(
                                            name_ident.span,
                                        );
                                        let kind =
                                            if is_constant { "constant" } else { "immutable" };
                                        diagnostics.push(Diagnostic::new(
                                            META.id,
                                            format!(
                                                "{kind} `{name}` should use UPPER_SNAKE_CASE"
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
