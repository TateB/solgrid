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
            let mut max_priority = 0u8;

            for item in source_unit.items.iter() {
                if let Some(priority) = item_priority(&item.kind) {
                    if priority < max_priority {
                        let range = solgrid_ast::span_to_range(item.span);
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
                        diagnostics.push(Diagnostic::new(
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
            diagnostics
        });

        result.unwrap_or_default()
    }
}
