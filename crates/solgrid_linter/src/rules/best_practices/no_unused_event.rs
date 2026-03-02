//! Rule: best-practices/no-unused-event
//!
//! Detect declared but unused events.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::ItemKind;
use solgrid_parser::with_parsed_ast_sequential;

static META: RuleMeta = RuleMeta {
    id: "best-practices/no-unused-event",
    name: "no-unused-event",
    category: RuleCategory::BestPractices,
    default_severity: Severity::Warning,
    description: "event is declared but never emitted",
    fix_availability: FixAvailability::None,
};

pub struct NoUnusedEventRule;

impl Rule for NoUnusedEventRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let filename = ctx.path.to_string_lossy().to_string();
        let result = with_parsed_ast_sequential(ctx.source, &filename, |source_unit| {
            let mut diagnostics = Vec::new();
            let mut event_decls: Vec<(&str, std::ops::Range<usize>)> = Vec::new();

            // Collect all event declarations
            for item in source_unit.items.iter() {
                if let ItemKind::Contract(contract) = &item.kind {
                    for body_item in contract.body.iter() {
                        if let ItemKind::Event(ev) = &body_item.kind {
                            let name = ev.name.as_str();
                            let range = solgrid_ast::span_to_range(ev.name.span);
                            event_decls.push((name, range));
                        }
                    }
                }
            }

            // Check if each event is emitted somewhere in the source
            for (name, range) in &event_decls {
                let pattern = format!("emit {name}(");
                if !ctx.source.contains(&pattern) {
                    diagnostics.push(Diagnostic::new(
                        META.id,
                        format!("event `{name}` is declared but never emitted"),
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
