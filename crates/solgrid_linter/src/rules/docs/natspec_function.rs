//! Rule: docs/natspec-function
//!
//! External and public functions must have `@notice` NatSpec documentation.

use crate::context::LintContext;
use crate::rule::Rule;
use crate::rules::best_practices::natspec_helpers::extract_natspec;
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::{FunctionKind, ItemKind, Visibility};
use solgrid_parser::with_parsed_ast_sequential;

static META: RuleMeta = RuleMeta {
    id: "docs/natspec-function",
    name: "natspec-function",
    category: RuleCategory::Docs,
    default_severity: Severity::Info,
    description: "external/public functions must have @notice NatSpec documentation",
    fix_availability: FixAvailability::None,
};

pub struct NatspecFunctionRule;

impl Rule for NatspecFunctionRule {
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
                            // Skip constructors, fallback, receive
                            if matches!(
                                func.kind,
                                FunctionKind::Constructor
                                    | FunctionKind::Fallback
                                    | FunctionKind::Receive
                                    | FunctionKind::Modifier
                            ) {
                                continue;
                            }

                            // Only check public and external functions
                            let is_public_or_external = matches!(
                                func.header.visibility(),
                                Some(Visibility::Public) | Some(Visibility::External)
                            );

                            if !is_public_or_external {
                                continue;
                            }

                            let span_start = solgrid_ast::span_to_range(body_item.span).start;
                            let range = solgrid_ast::item_name_range(body_item);

                            match extract_natspec(ctx.source, span_start) {
                                None => {
                                    let name = func
                                        .header
                                        .name
                                        .map(|n| n.as_str().to_string())
                                        .unwrap_or_else(|| func.kind.to_str().to_string());
                                    diagnostics.push(Diagnostic::new(
                                        META.id,
                                        format!(
                                            "function `{name}` is missing @notice NatSpec documentation"
                                        ),
                                        META.default_severity,
                                        range,
                                    ));
                                }
                                Some(natspec) => {
                                    if !natspec.contains("@notice") {
                                        let name = func
                                            .header
                                            .name
                                            .map(|n| n.as_str().to_string())
                                            .unwrap_or_else(|| func.kind.to_str().to_string());
                                        diagnostics.push(Diagnostic::new(
                                            META.id,
                                            format!("function `{name}` NatSpec is missing @notice"),
                                            META.default_severity,
                                            range,
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
