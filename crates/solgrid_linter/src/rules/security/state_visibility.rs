//! Rule: security/state-visibility
//!
//! Require explicit visibility on all state variables.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;
use solgrid_parser::with_parsed_ast_sequential;

static META: RuleMeta = RuleMeta {
    id: "security/state-visibility",
    name: "state-visibility",
    category: RuleCategory::Security,
    default_severity: Severity::Error,
    description: "state variable missing explicit visibility modifier",
    fix_availability: FixAvailability::None,
};

pub struct StateVisibilityRule;

impl Rule for StateVisibilityRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let filename = ctx.path.to_string_lossy().to_string();
        let result = with_parsed_ast_sequential(ctx.source, &filename, |source_unit| {
            let mut diagnostics = Vec::new();
            for item in source_unit.items.iter() {
                if let solgrid_parser::solar_ast::ItemKind::Contract(contract) = &item.kind {
                    for body_item in contract.body.iter() {
                        if let solgrid_parser::solar_ast::ItemKind::Variable(var) = &body_item.kind
                        {
                            if var.visibility.is_none() {
                                let span = body_item.span;
                                let range = solgrid_ast::span_to_range(span);
                                let name = var
                                    .name
                                    .map(|n| n.as_str().to_string())
                                    .unwrap_or_else(|| "<unnamed>".to_string());
                                diagnostics.push(Diagnostic::new(
                                    META.id,
                                    format!(
                                        "state variable `{name}` has no explicit visibility modifier"
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
