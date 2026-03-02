//! Rule: naming/interface-starts-with-i
//!
//! Interface names must start with `I`.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::{ContractKind, ItemKind};
use solgrid_parser::with_parsed_ast_sequential;

static META: RuleMeta = RuleMeta {
    id: "naming/interface-starts-with-i",
    name: "interface-starts-with-i",
    category: RuleCategory::Naming,
    default_severity: Severity::Warning,
    description: "interface name should start with `I`",
    fix_availability: FixAvailability::None,
};

pub struct InterfaceStartsWithIRule;

impl Rule for InterfaceStartsWithIRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let filename = ctx.path.to_string_lossy().to_string();
        let result = with_parsed_ast_sequential(ctx.source, &filename, |source_unit| {
            let mut diagnostics = Vec::new();
            for item in source_unit.items.iter() {
                if let ItemKind::Contract(contract) = &item.kind {
                    if contract.kind == ContractKind::Interface {
                        let name = contract.name.as_str();
                        if !name.starts_with('I')
                            || name.len() < 2
                            || !name.chars().nth(1).unwrap().is_uppercase()
                        {
                            let range = solgrid_ast::span_to_range(contract.name.span);
                            diagnostics.push(Diagnostic::new(
                                META.id,
                                format!(
                                    "interface name `{name}` should start with `I` followed by an uppercase letter"
                                ),
                                META.default_severity,
                                range,
                            ));
                        }
                    }
                }
            }
            diagnostics
        });

        result.unwrap_or_default()
    }
}
