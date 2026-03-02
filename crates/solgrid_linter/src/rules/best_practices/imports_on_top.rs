//! Rule: best-practices/imports-on-top
//!
//! All imports must be at the top of the file (after pragma).

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::ItemKind;
use solgrid_parser::with_parsed_ast_sequential;

static META: RuleMeta = RuleMeta {
    id: "best-practices/imports-on-top",
    name: "imports-on-top",
    category: RuleCategory::BestPractices,
    default_severity: Severity::Warning,
    description: "import statements should be at the top of the file",
    fix_availability: FixAvailability::None,
};

pub struct ImportsOnTopRule;

impl Rule for ImportsOnTopRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let filename = ctx.path.to_string_lossy().to_string();
        let result = with_parsed_ast_sequential(ctx.source, &filename, |source_unit| {
            let mut diagnostics = Vec::new();
            let mut seen_non_import = false;

            for item in source_unit.items.iter() {
                match &item.kind {
                    ItemKind::Pragma(_) => {
                        // Pragmas are always allowed at the top
                    }
                    ItemKind::Import(_) => {
                        if seen_non_import {
                            let range = solgrid_ast::span_to_range(item.span);
                            diagnostics.push(Diagnostic::new(
                                META.id,
                                "import statements should be at the top of the file",
                                META.default_severity,
                                range,
                            ));
                        }
                    }
                    _ => {
                        seen_non_import = true;
                    }
                }
            }

            diagnostics
        });

        result.unwrap_or_default()
    }
}
