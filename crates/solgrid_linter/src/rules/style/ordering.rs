//! Rule: style/ordering
//!
//! Enforce top-level declaration order: pragma, imports, interfaces, libraries, contracts.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::{ContractKind, ItemKind};
use solgrid_parser::with_parsed_ast_sequential;

static META: RuleMeta = RuleMeta {
    id: "style/ordering",
    name: "ordering",
    category: RuleCategory::Style,
    default_severity: Severity::Info,
    description: "top-level declarations should follow order: pragma, imports, interfaces, libraries, contracts",
    fix_availability: FixAvailability::Available(FixSafety::Suggestion),
};

pub struct OrderingRule;

fn item_priority(kind: &ItemKind<'_>) -> Option<u8> {
    match kind {
        ItemKind::Pragma(_) => Some(0),
        ItemKind::Import(_) => Some(1),
        ItemKind::Contract(c) => match c.kind {
            ContractKind::Interface => Some(2),
            ContractKind::Library => Some(3),
            ContractKind::Contract | ContractKind::AbstractContract => Some(4),
        },
        ItemKind::Using(_) => Some(1), // treat using-for at file level like imports
        ItemKind::Function(_) => Some(5), // free functions after contracts
        _ => None,
    }
}

impl Rule for OrderingRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let filename = ctx.path.to_string_lossy().to_string();
        let result = with_parsed_ast_sequential(ctx.source, &filename, |source_unit| {
            let mut diagnostics = Vec::new();

            let items: Vec<_> = source_unit.items.iter().collect();
            if items.is_empty() {
                return diagnostics;
            }

            // First pass: detect violations
            let mut max_priority = 0u8;
            let mut violation_diags = Vec::new();
            for item in items.iter() {
                if let Some(priority) = item_priority(&item.kind) {
                    if priority < max_priority {
                        let range = solgrid_ast::item_name_range(item);
                        let kind_name = match &item.kind {
                            ItemKind::Pragma(_) => "pragma",
                            ItemKind::Import(_) => "import",
                            ItemKind::Contract(c) => match c.kind {
                                ContractKind::Interface => "interface",
                                ContractKind::Library => "library",
                                _ => "contract",
                            },
                            ItemKind::Using(_) => "using",
                            ItemKind::Function(_) => "free function",
                            _ => "declaration",
                        };
                        violation_diags.push(Diagnostic::new(
                            META.id,
                            format!(
                                "{kind_name} should appear before higher-priority declarations (expected order: pragma, imports, interfaces, libraries, contracts)"
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
                return diagnostics;
            }

            // Build fix: chunk-based reordering
            // Only include items with known priorities
            let prioritized: Vec<_> = items
                .iter()
                .filter_map(|item| {
                    item_priority(&item.kind).map(|p| (p, solgrid_ast::span_to_range(item.span)))
                })
                .collect();

            if prioritized.len() >= 2 {
                let first_start = prioritized[0].1.start;
                let last_end = prioritized.last().unwrap().1.end;

                // Build chunks
                let mut chunks: Vec<(u8, usize, String)> = Vec::new();
                for (idx, (priority, span_range)) in prioritized.iter().enumerate() {
                    let prev_end = if idx == 0 {
                        first_start
                    } else {
                        prioritized[idx - 1].1.end
                    };
                    let chunk = ctx.source[prev_end..span_range.end].to_string();
                    chunks.push((*priority, idx, chunk));
                }

                // Sort by (priority, original_index)
                chunks.sort_by_key(|&(p, i, _)| (p, i));

                let replacement: String = chunks
                    .iter()
                    .map(|(_, _, text)| text.as_str())
                    .collect::<String>();

                // Replace from first item start to last item end
                let fix = Fix::suggestion(
                    "Reorder top-level declarations",
                    vec![TextEdit::replace(
                        first_start..last_end,
                        replacement.trim_end().to_string(),
                    )],
                );

                violation_diags[0] = violation_diags[0].clone().with_fix(fix);
            }

            diagnostics.extend(violation_diags);
            diagnostics
        });

        result.unwrap_or_default()
    }
}
