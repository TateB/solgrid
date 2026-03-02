//! Rule: security/payable-fallback
//!
//! Require `payable` on fallback and receive functions.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::{FunctionKind, ItemKind, StateMutability};
use solgrid_parser::with_parsed_ast_sequential;

static META: RuleMeta = RuleMeta {
    id: "security/payable-fallback",
    name: "payable-fallback",
    category: RuleCategory::Security,
    default_severity: Severity::Warning,
    description: "fallback and receive functions should be `payable`",
    fix_availability: FixAvailability::None,
};

pub struct PayableFallbackRule;

impl Rule for PayableFallbackRule {
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
                            if matches!(func.kind, FunctionKind::Fallback | FunctionKind::Receive)
                                && func.header.state_mutability() != StateMutability::Payable
                            {
                                let kind_str = func.kind.to_str();
                                let range = solgrid_ast::span_to_range(body_item.span);
                                diagnostics.push(Diagnostic::new(
                                    META.id,
                                    format!("`{kind_str}` function should be marked `payable`"),
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
