//! Rule: docs/natspec-param-mismatch
//!
//! NatSpec `@param` names must match actual parameter names.

use crate::context::LintContext;
use crate::rule::Rule;
use crate::rules::best_practices::natspec_helpers::{extract_natspec, parse_natspec_params};
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::{FunctionKind, ItemKind, Visibility};
use solgrid_parser::with_parsed_ast_sequential;

static META: RuleMeta = RuleMeta {
    id: "docs/natspec-param-mismatch",
    name: "natspec-param-mismatch",
    category: RuleCategory::Docs,
    default_severity: Severity::Info,
    description: "NatSpec @param names must match actual parameter names",
    fix_availability: FixAvailability::Available(FixSafety::Safe),
};

pub struct NatspecParamMismatchRule;

impl Rule for NatspecParamMismatchRule {
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

                            if func.header.parameters.is_empty() {
                                continue;
                            }

                            let span_start = solgrid_ast::span_to_range(body_item.span).start;
                            let natspec = match extract_natspec(ctx.source, span_start) {
                                Some(n) => n,
                                None => continue,
                            };

                            let documented_params = parse_natspec_params(&natspec);
                            if documented_params.is_empty() {
                                continue;
                            }

                            // Collect actual parameter names
                            let actual_params: Vec<String> = func
                                .header
                                .parameters
                                .iter()
                                .filter_map(|p| p.name.map(|n| n.as_str().to_string()))
                                .collect();

                            // Check each documented @param against actual params
                            for doc_param in &documented_params {
                                if !actual_params.contains(doc_param) {
                                    let range = solgrid_ast::item_name_range(body_item);
                                    let func_name = func
                                        .header
                                        .name
                                        .map(|n| n.as_str().to_string())
                                        .unwrap_or_else(|| func.kind.to_str().to_string());

                                    // Try to find the mismatched @param in source and create a fix
                                    let natspec_start =
                                        span_start.saturating_sub(natspec.len() + 10);
                                    let search_area = &ctx.source[natspec_start..span_start];

                                    let fix = find_param_in_source(
                                        search_area,
                                        natspec_start,
                                        doc_param,
                                        &actual_params,
                                    );

                                    let mut diag = Diagnostic::new(
                                        META.id,
                                        format!(
                                            "function `{func_name}` NatSpec @param `{doc_param}` does not match any parameter"
                                        ),
                                        META.default_severity,
                                        range,
                                    );

                                    if let Some(fix) = fix {
                                        diag = diag.with_fix(fix);
                                    }

                                    diagnostics.push(diag);
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

/// Find a `@param name` in source text and create a fix to replace it with the best match.
fn find_param_in_source(
    search_area: &str,
    area_offset: usize,
    wrong_name: &str,
    actual_params: &[String],
) -> Option<Fix> {
    // Find the @param wrong_name in the search area
    let pattern = format!("@param {wrong_name}");
    let pos = search_area.find(&pattern)?;

    let param_name_start = area_offset + pos + "@param ".len();
    let param_name_end = param_name_start + wrong_name.len();

    // Find the best matching actual param (first one not already documented)
    let best_match = actual_params.first()?;

    Some(Fix::safe(
        format!("Replace @param `{wrong_name}` with `{best_match}`"),
        vec![TextEdit::replace(
            param_name_start..param_name_end,
            best_match.clone(),
        )],
    ))
}
