//! Rule: style/func-order
//!
//! Enforce function ordering within contracts per the Solidity style guide:
//! constructor, receive, fallback, external, public, internal, private.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::{ContractKind, FunctionKind, ItemKind, Visibility};
use solgrid_parser::with_parsed_ast_sequential;

static META: RuleMeta = RuleMeta {
    id: "style/func-order",
    name: "func-order",
    category: RuleCategory::Style,
    default_severity: Severity::Info,
    description: "functions should be ordered: constructor, receive, fallback, external, public, internal, private",
    fix_availability: FixAvailability::Available(FixSafety::Suggestion),
};

pub struct FuncOrderRule;

fn is_comment_line(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("//")
        || trimmed.starts_with("/*")
        || trimmed.starts_with('*')
        || trimmed.ends_with("*/")
}

fn line_start(source: &str, pos: usize) -> usize {
    source[..pos].rfind('\n').map(|idx| idx + 1).unwrap_or(0)
}

fn attached_comment_start(source: &str, item_start: usize, body_start: usize) -> usize {
    let mut start = line_start(source, item_start).max(body_start);

    while start > body_start {
        let prev_line_end = start.saturating_sub(1);
        let prev_line_start = source[..prev_line_end]
            .rfind('\n')
            .map(|idx| idx + 1)
            .unwrap_or(body_start);
        let line = &source[prev_line_start..prev_line_end];

        if line.trim().is_empty() || !is_comment_line(line) {
            break;
        }

        start = prev_line_start.max(body_start);
    }

    start
}

fn func_priority(kind: FunctionKind, visibility: Option<Visibility>) -> u8 {
    match kind {
        FunctionKind::Constructor => 0,
        FunctionKind::Receive => 1,
        FunctionKind::Fallback => 2,
        FunctionKind::Function | FunctionKind::Modifier => match visibility {
            Some(Visibility::External) => 3,
            Some(Visibility::Public) => 4,
            Some(Visibility::Internal) => 5,
            Some(Visibility::Private) => 6,
            None => 5, // default is internal
        },
    }
}

impl Rule for FuncOrderRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let filename = ctx.path.to_string_lossy().to_string();
        let result = with_parsed_ast_sequential(ctx.source, &filename, |source_unit| {
            let mut diagnostics = Vec::new();

            for item in source_unit.items.iter() {
                if let ItemKind::Contract(contract) = &item.kind {
                    // Skip interfaces and libraries — they have different conventions
                    if matches!(
                        contract.kind,
                        ContractKind::Interface | ContractKind::Library
                    ) {
                        continue;
                    }

                    // First pass: detect violations
                    let mut max_priority = 0u8;
                    let mut violation_diags = Vec::new();
                    for body_item in contract.body.iter() {
                        if let ItemKind::Function(func) = &body_item.kind {
                            if func.kind == FunctionKind::Modifier {
                                continue;
                            }

                            let priority = func_priority(func.kind, func.header.visibility());
                            if priority < max_priority {
                                let range = solgrid_ast::item_name_range(body_item);
                                let name = func
                                    .header
                                    .name
                                    .map(|n| n.as_str().to_string())
                                    .unwrap_or_else(|| func.kind.to_str().to_string());
                                violation_diags.push(Diagnostic::new(
                                    META.id,
                                    format!(
                                        "function `{name}` is out of order (expected: constructor, receive, fallback, external, public, internal, private)"
                                    ),
                                    META.default_severity,
                                    range,
                                ));
                            } else {
                                max_priority = priority;
                            }
                        }
                    }

                    if violation_diags.is_empty() {
                        continue;
                    }

                    // Build fix: chunk-based reordering of functions only
                    // Each chunk = text from end of previous function to end of this function
                    // Non-function items between functions get carried with the preceding function
                    let contract_range = solgrid_ast::span_to_range(item.span);
                    let contract_text = &ctx.source[contract_range.clone()];
                    if let (Some(brace_open), Some(brace_close)) =
                        (contract_text.find('{'), contract_text.rfind('}'))
                    {
                        let body_start = contract_range.start + brace_open + 1;
                        let body_end = contract_range.start + brace_close;

                        // Collect function items with their priorities and spans
                        let mut func_items: Vec<(u8, usize, std::ops::Range<usize>)> = Vec::new();
                        for body_item in contract.body.iter() {
                            if let ItemKind::Function(func) = &body_item.kind {
                                if func.kind == FunctionKind::Modifier {
                                    continue;
                                }
                                let priority = func_priority(func.kind, func.header.visibility());
                                let span_range = solgrid_ast::span_to_range(body_item.span);
                                func_items.push((priority, func_items.len(), span_range));
                            }
                        }

                        if func_items.len() >= 2 {
                            let first_chunk_start = attached_comment_start(
                                ctx.source,
                                func_items.first().unwrap().2.start,
                                body_start,
                            );
                            let prefix = &ctx.source[body_start..first_chunk_start];
                            let trailing = &ctx.source[func_items.last().unwrap().2.end..body_end];

                            let mut functions: Vec<(u8, usize, String, String)> = func_items
                                .iter()
                                .enumerate()
                                .map(|(idx, (priority, orig_idx, span_range))| {
                                    let chunk_start = attached_comment_start(
                                        ctx.source,
                                        span_range.start,
                                        body_start,
                                    );
                                    let leading = if idx == 0 {
                                        String::new()
                                    } else {
                                        ctx.source[func_items[idx - 1].2.end..chunk_start]
                                            .to_string()
                                    };

                                    (
                                        *priority,
                                        *orig_idx,
                                        leading,
                                        ctx.source[chunk_start..span_range.end].to_string(),
                                    )
                                })
                                .collect();

                            // Sort by (priority, original_index) for stable ordering
                            functions.sort_by_key(|&(p, i, _, _)| (p, i));

                            let mut replacement = String::from(prefix);
                            if let Some((_, _, _, first_body)) = functions.first() {
                                replacement.push_str(first_body);
                                for (_, _, leading, body) in functions.iter().skip(1) {
                                    if leading.trim().is_empty() {
                                        replacement.push_str("\n\n");
                                    } else {
                                        replacement.push_str(leading);
                                    }
                                    replacement.push_str(body);
                                }
                            }
                            replacement.push_str(trailing);

                            let fix = Fix::suggestion(
                                "Reorder functions",
                                vec![TextEdit::replace(body_start..body_end, replacement)],
                            );

                            for diag in &mut violation_diags {
                                *diag = diag.clone().with_fix(fix.clone());
                            }
                        }
                    }

                    diagnostics.extend(violation_diags);
                }
            }
            diagnostics
        });

        result.unwrap_or_default()
    }
}
