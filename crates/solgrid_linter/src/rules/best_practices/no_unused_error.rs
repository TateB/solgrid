//! Rule: best-practices/no-unused-error
//!
//! Detect declared but unused custom errors.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::ItemKind;
use solgrid_parser::with_parsed_ast_sequential;

static META: RuleMeta = RuleMeta {
    id: "best-practices/no-unused-error",
    name: "no-unused-error",
    category: RuleCategory::BestPractices,
    default_severity: Severity::Warning,
    description: "custom error is declared but never used",
    fix_availability: FixAvailability::None,
};

pub struct NoUnusedErrorRule;

impl Rule for NoUnusedErrorRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let filename = ctx.path.to_string_lossy().to_string();
        let result = with_parsed_ast_sequential(ctx.source, &filename, |source_unit| {
            let mut diagnostics = Vec::new();
            let mut error_decls: Vec<(&str, std::ops::Range<usize>)> = Vec::new();

            // Collect all error declarations
            for item in source_unit.items.iter() {
                // Top-level errors
                if let ItemKind::Error(err) = &item.kind {
                    let name = err.name.as_str();
                    let range = solgrid_ast::span_to_range(err.name.span);
                    error_decls.push((name, range));
                }
                // Errors inside contracts
                if let ItemKind::Contract(contract) = &item.kind {
                    for body_item in contract.body.iter() {
                        if let ItemKind::Error(err) = &body_item.kind {
                            let name = err.name.as_str();
                            let range = solgrid_ast::span_to_range(err.name.span);
                            error_decls.push((name, range));
                        }
                    }
                }
            }

            // Check if each error is used in the source (in a revert statement)
            for (name, range) in &error_decls {
                let pattern = format!("revert {name}(");
                let pattern2 = format!("revert {name};");
                if !ctx.source.contains(&pattern) && !ctx.source.contains(&pattern2) {
                    diagnostics.push(Diagnostic::new(
                        META.id,
                        format!("custom error `{name}` is declared but never used"),
                        META.default_severity,
                        range.clone(),
                    ));
                }
            }

            diagnostics
        });

        result.unwrap_or_default()
    }
}
