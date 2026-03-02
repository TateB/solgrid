//! Rule: best-practices/no-global-import
//!
//! Disallow `import "file.sol"` — use named imports instead.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::{ImportItems, ItemKind};
use solgrid_parser::with_parsed_ast_sequential;

static META: RuleMeta = RuleMeta {
    id: "best-practices/no-global-import",
    name: "no-global-import",
    category: RuleCategory::BestPractices,
    default_severity: Severity::Warning,
    description: "use named imports instead of importing entire files",
    fix_availability: FixAvailability::None,
};

pub struct NoGlobalImportRule;

impl Rule for NoGlobalImportRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let filename = ctx.path.to_string_lossy().to_string();
        let result = with_parsed_ast_sequential(ctx.source, &filename, |source_unit| {
            let mut diagnostics = Vec::new();
            for item in source_unit.items.iter() {
                if let ItemKind::Import(import) = &item.kind {
                    // Plain import with no alias: `import "file.sol";`
                    if matches!(import.items, ImportItems::Plain(None)) {
                        let path_str = import.path.value.as_str();
                        let range = solgrid_ast::span_to_range(item.span);
                        diagnostics.push(Diagnostic::new(
                            META.id,
                            format!(
                                "use named imports instead of `import \"{path_str}\"`"
                            ),
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
