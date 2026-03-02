//! Rule: security/multiple-sends
//!
//! Flag functions that contain more than one `.send()` call.  Multiple sends
//! in a single function can lead to unexpected failures and re-entrancy issues.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::{FunctionKind, ItemKind};
use solgrid_parser::with_parsed_ast_sequential;

static META: RuleMeta = RuleMeta {
    id: "security/multiple-sends",
    name: "multiple-sends",
    category: RuleCategory::Security,
    default_severity: Severity::Warning,
    description: "avoid multiple `.send()` calls in a single function",
    fix_availability: FixAvailability::None,
};

pub struct MultipleSendsRule;

impl Rule for MultipleSendsRule {
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
                            // Skip functions without bodies (interface declarations)
                            if let Some(body) = &func.body {
                                let body_range = solgrid_ast::span_to_range(body.span);
                                let body_text = &ctx.source[body_range.clone()];

                                // Count `.send(` occurrences in the function body
                                let pattern = ".send(";
                                let mut send_positions = Vec::new();
                                let mut search_from = 0;
                                while let Some(pos) = body_text[search_from..].find(pattern) {
                                    let abs_pos = body_range.start + search_from + pos;
                                    send_positions.push(abs_pos);
                                    search_from += pos + pattern.len();
                                }

                                if send_positions.len() > 1 {
                                    let func_name = match func.kind {
                                        FunctionKind::Function => func
                                            .header
                                            .name
                                            .map(|n| n.as_str().to_string())
                                            .unwrap_or_else(|| "<unnamed>".to_string()),
                                        FunctionKind::Constructor => "constructor".to_string(),
                                        FunctionKind::Fallback => "fallback".to_string(),
                                        FunctionKind::Receive => "receive".to_string(),
                                        FunctionKind::Modifier => "modifier".to_string(),
                                    };
                                    // Flag each .send( occurrence after the first
                                    for &abs_pos in &send_positions[1..] {
                                        diagnostics.push(Diagnostic::new(
                                            META.id,
                                            format!(
                                                "multiple `.send()` calls in function `{func_name}` — consider using withdrawal pattern"
                                            ),
                                            META.default_severity,
                                            abs_pos..abs_pos + pattern.len(),
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
