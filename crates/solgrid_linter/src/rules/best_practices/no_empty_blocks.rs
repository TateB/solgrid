//! Rule: best-practices/no-empty-blocks
//!
//! Disallow empty blocks (excluding receive/fallback).

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_config::NoEmptyBlocksSettings;
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::{FunctionKind, ItemKind};
use solgrid_parser::with_parsed_ast_sequential;
use std::ops::Range;

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
        let settings: NoEmptyBlocksSettings = ctx.config.rule_settings(META.id);
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
                                    if settings.allow_comments
                                        && empty_block_has_comment(
                                            ctx.source,
                                            solgrid_ast::span_to_range(body.span),
                                        )
                                    {
                                        continue;
                                    }
                                    let range = solgrid_ast::item_name_range(body_item);
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
                        if settings.allow_comments
                            && empty_block_has_comment(
                                ctx.source,
                                solgrid_ast::span_to_range(item.span),
                            )
                        {
                            continue;
                        }
                        let range = solgrid_ast::item_name_range(item);
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

fn empty_block_has_comment(source: &str, range: Range<usize>) -> bool {
    let Some(text) = source.get(range) else {
        return false;
    };
    let Some(close) = text.rfind('}') else {
        return false;
    };
    let Some(open) = text[..close].rfind('{') else {
        return false;
    };

    contains_comment(&text[open + 1..close])
}

fn contains_comment(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'/' && matches!(bytes[i + 1], b'/' | b'*') {
            return true;
        }
        i += 1;
    }
    false
}
