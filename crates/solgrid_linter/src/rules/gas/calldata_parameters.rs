//! Rule: gas/calldata-parameters
//!
//! Use `calldata` instead of `memory` for read-only external function parameters.
//! `calldata` avoids copying data into memory, saving gas.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::ItemKind;
use solgrid_parser::with_parsed_ast_sequential;

static META: RuleMeta = RuleMeta {
    id: "gas/calldata-parameters",
    name: "calldata-parameters",
    category: RuleCategory::Gas,
    default_severity: Severity::Info,
    description: "use `calldata` instead of `memory` for read-only external function parameters",
    fix_availability: FixAvailability::Available(FixSafety::Safe),
};

pub struct CalldataParametersRule;

impl Rule for CalldataParametersRule {
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
                            // Get function text to check for external visibility
                            let func_range = solgrid_ast::span_to_range(body_item.span);
                            let func_text = &ctx.source[func_range.clone()];

                            // Only check external functions
                            if !is_external_function(func_text) {
                                continue;
                            }

                            // Check parameters for `memory` keyword
                            for param in func.header.parameters.iter() {
                                let param_range = solgrid_ast::span_to_range(param.span);
                                let param_text = &ctx.source[param_range.clone()];

                                if let Some(memory_offset) = find_memory_keyword(param_text) {
                                    let abs_memory = param_range.start + memory_offset;
                                    diagnostics.push(
                                        Diagnostic::new(
                                            META.id,
                                            "use `calldata` instead of `memory` for read-only external function parameters to save gas",
                                            META.default_severity,
                                            abs_memory..abs_memory + 6,
                                        )
                                        .with_fix(Fix::safe(
                                            "Replace `memory` with `calldata`",
                                            vec![TextEdit::replace(
                                                abs_memory..abs_memory + 6,
                                                "calldata",
                                            )],
                                        )),
                                    );
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

fn is_external_function(func_text: &str) -> bool {
    // Check for `external` keyword before the function body
    let before_body = if let Some(brace) = func_text.find('{') {
        &func_text[..brace]
    } else {
        func_text
    };
    // Check word-boundary "external"
    let mut search_from = 0;
    while let Some(pos) = before_body[search_from..].find("external") {
        let abs_pos = search_from + pos;
        let before_ok = abs_pos == 0
            || !before_body.as_bytes()[abs_pos - 1].is_ascii_alphanumeric()
                && before_body.as_bytes()[abs_pos - 1] != b'_';
        let after_pos = abs_pos + 8;
        let after_ok = after_pos >= before_body.len()
            || !before_body.as_bytes()[after_pos].is_ascii_alphanumeric()
                && before_body.as_bytes()[after_pos] != b'_';
        if before_ok && after_ok {
            return true;
        }
        search_from = abs_pos + 8;
    }
    false
}

fn find_memory_keyword(param_text: &str) -> Option<usize> {
    let mut search_from = 0;
    while let Some(pos) = param_text[search_from..].find("memory") {
        let abs_pos = search_from + pos;
        let before_ok = abs_pos == 0
            || !param_text.as_bytes()[abs_pos - 1].is_ascii_alphanumeric()
                && param_text.as_bytes()[abs_pos - 1] != b'_';
        let after_pos = abs_pos + 6;
        let after_ok = after_pos >= param_text.len()
            || !param_text.as_bytes()[after_pos].is_ascii_alphanumeric()
                && param_text.as_bytes()[after_pos] != b'_';
        if before_ok && after_ok {
            return Some(abs_pos);
        }
        search_from = abs_pos + 6;
    }
    None
}
