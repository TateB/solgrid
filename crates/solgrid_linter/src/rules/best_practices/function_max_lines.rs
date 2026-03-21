//! Rule: best-practices/function-max-lines
//!
//! Flag functions exceeding a maximum line count.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::ItemKind;
use solgrid_parser::with_parsed_ast_sequential;

static META: RuleMeta = RuleMeta {
    id: "best-practices/function-max-lines",
    name: "function-max-lines",
    category: RuleCategory::BestPractices,
    default_severity: Severity::Warning,
    description: "function body exceeds maximum line count",
    fix_availability: FixAvailability::None,
};

pub struct FunctionMaxLinesRule;

impl Rule for FunctionMaxLinesRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let max_lines = ctx.config.lint.function_max_lines();
        let filename = ctx.path.to_string_lossy().to_string();
        let result = with_parsed_ast_sequential(ctx.source, &filename, |source_unit| {
            let mut diagnostics = Vec::new();
            for item in source_unit.items.iter() {
                if let ItemKind::Contract(contract) = &item.kind {
                    for body_item in contract.body.iter() {
                        if let ItemKind::Function(func) = &body_item.kind {
                            if let Some(body) = &func.body {
                                let body_text = solgrid_ast::span_text(ctx.source, body.span);
                                let line_count = body_text.lines().count();
                                if line_count > max_lines {
                                    let name = func
                                        .header
                                        .name
                                        .map(|n| n.as_str().to_string())
                                        .unwrap_or_else(|| func.kind.to_string());
                                    let range = solgrid_ast::item_name_range(body_item);
                                    diagnostics.push(Diagnostic::new(
                                        META.id,
                                        format!(
                                            "function `{name}` has {line_count} lines (maximum is {max_lines})"
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
            diagnostics
        });

        result.unwrap_or_default()
    }
}
