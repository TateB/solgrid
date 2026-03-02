//! Rule: best-practices/one-contract-per-file
//!
//! Enforce one contract/interface/library per file.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::ItemKind;
use solgrid_parser::with_parsed_ast_sequential;

static META: RuleMeta = RuleMeta {
    id: "best-practices/one-contract-per-file",
    name: "one-contract-per-file",
    category: RuleCategory::BestPractices,
    default_severity: Severity::Warning,
    description: "only one contract/interface/library should be defined per file",
    fix_availability: FixAvailability::None,
};

pub struct OneContractPerFileRule;

impl Rule for OneContractPerFileRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let filename = ctx.path.to_string_lossy().to_string();
        let result = with_parsed_ast_sequential(ctx.source, &filename, |source_unit| {
            let mut diagnostics = Vec::new();
            let mut contract_count = 0;
            for item in source_unit.items.iter() {
                if let ItemKind::Contract(contract) = &item.kind {
                    contract_count += 1;
                    if contract_count > 1 {
                        let name = contract.name.as_str();
                        let range = solgrid_ast::span_to_range(item.span);
                        diagnostics.push(Diagnostic::new(
                            META.id,
                            format!(
                                "multiple contracts in one file; `{name}` should be in its own file"
                            ),
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
