//! Rule: docs/natspec-contract
//!
//! Contracts must have `@title` and `@author` NatSpec documentation.

use crate::context::LintContext;
use crate::rule::Rule;
use crate::rules::best_practices::natspec_helpers::extract_natspec;
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::ItemKind;
use solgrid_parser::with_parsed_ast_sequential;

static META: RuleMeta = RuleMeta {
    id: "docs/natspec-contract",
    name: "natspec-contract",
    category: RuleCategory::Docs,
    default_severity: Severity::Info,
    description: "contracts must have @title and @author NatSpec documentation",
    fix_availability: FixAvailability::None,
};

pub struct NatspecContractRule;

impl Rule for NatspecContractRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let filename = ctx.path.to_string_lossy().to_string();
        let result = with_parsed_ast_sequential(ctx.source, &filename, |source_unit| {
            let mut diagnostics = Vec::new();

            for item in source_unit.items.iter() {
                if let ItemKind::Contract(contract) = &item.kind {
                    let name = contract.name.as_str().to_string();

                    let span_start = solgrid_ast::span_to_range(item.span).start;
                    let range = solgrid_ast::item_name_range(item);

                    match extract_natspec(ctx.source, span_start) {
                        None => {
                            diagnostics.push(Diagnostic::new(
                                META.id,
                                format!(
                                    "contract `{name}` is missing NatSpec documentation (@title and @author)"
                                ),
                                META.default_severity,
                                range,
                            ));
                        }
                        Some(natspec) => {
                            let has_title = natspec.contains("@title");
                            let has_author = natspec.contains("@author");

                            if !has_title || !has_author {
                                let mut missing = Vec::new();
                                if !has_title {
                                    missing.push("@title");
                                }
                                if !has_author {
                                    missing.push("@author");
                                }
                                diagnostics.push(Diagnostic::new(
                                    META.id,
                                    format!(
                                        "contract `{name}` NatSpec is missing {}",
                                        missing.join(" and ")
                                    ),
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
