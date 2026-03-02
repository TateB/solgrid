//! Rule: best-practices/no-empty-blocks
//!
//! Disallow empty blocks (excluding receive/fallback).

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;
use solgrid_parser::with_parsed_ast_sequential;
use solgrid_parser::solar_ast::{ItemKind, FunctionKind};

static META: RuleMeta = RuleMeta {
    id: "best-practices/no-empty-blocks",
    name: "no-empty-blocks",
    category: RuleCategory::BestPractices,
    default_severity: Severity::Warning,
    description: "avoid empty code blocks",
    fix_availability: FixAvailability::None,
};

pub struct NoEmptyBlocksRule;

impl Rule for NoEmptyBlocksRule {
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
                            // Skip receive and fallback functions (empty is ok)
                            if matches!(func.kind, FunctionKind::Receive | FunctionKind::Fallback) {
                                continue;
                            }
                            // Check if function has an empty body
                            if let Some(body) = &func.body {
                                if body.is_empty() {
                                    let range = solgrid_ast::span_to_range(body_item.span);
                                    let name = func
                                        .header
                                        .name
                                        .map(|n| n.as_str().to_string())
                                        .unwrap_or_else(|| func.kind.to_string());
                                    diagnostics.push(Diagnostic::new(
                                        META.id,
                                        format!("function `{name}` has an empty body"),
                                        META.default_severity,
                                        range,
                                    ));
                                }
                            }
                        }
                    }

                    // Also check if the contract itself is empty
                    if contract.body.is_empty() {
                        let range = solgrid_ast::span_to_range(item.span);
                        let name = contract.name.as_str().to_string();
                        diagnostics.push(Diagnostic::new(
                            META.id,
                            format!("contract `{name}` has an empty body"),
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
