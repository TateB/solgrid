//! Rule: best-practices/use-natspec
//!
//! Require NatSpec documentation on public/external functions.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::{FunctionKind, ItemKind, Visibility};
use solgrid_parser::with_parsed_ast_sequential;

#[allow(dead_code)]
static META: RuleMeta = RuleMeta {
    id: "best-practices/use-natspec",
    name: "use-natspec",
    category: RuleCategory::BestPractices,
    default_severity: Severity::Warning,
    description: "public/external function is missing NatSpec documentation",
    fix_availability: FixAvailability::None,
};

#[allow(dead_code)]
pub struct UseNatspecRule;

impl Rule for UseNatspecRule {
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

                            let span_start = solgrid_ast::span_to_range(body_item.span).start;
                            if extract_natspec(ctx.source, span_start).is_none() {
                                let name = func
                                    .header
                                    .name
                                    .map(|n| n.as_str().to_string())
                                    .unwrap_or_else(|| func.kind.to_str().to_string());
                                let range = solgrid_ast::item_name_range(body_item);
                                diagnostics.push(Diagnostic::new(
                                    META.id,
                                    format!("function `{name}` is missing NatSpec documentation"),
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

/// Extract NatSpec comment block immediately preceding the given byte position.
/// Returns `None` if no NatSpec is found.
///
/// Supports both `///` line comments and `/** ... */` block comments.
pub(crate) fn extract_natspec(source: &str, item_start: usize) -> Option<String> {
    let before = &source[..item_start];

    // Trim trailing whitespace/newlines before the item
    let trimmed = before.trim_end();
    if trimmed.is_empty() {
        return None;
    }

    // Check for /** ... */ block comment
    if trimmed.ends_with("*/") {
        if let Some(block_start) = trimmed.rfind("/**") {
            let block = &trimmed[block_start..];
            return Some(block.to_string());
        }
        // Regular /* */ comment is not NatSpec
        return None;
    }

    // Check for consecutive /// lines
    let mut natspec_lines = Vec::new();
    for line in before.lines().rev() {
        let trimmed_line = line.trim();
        if trimmed_line.is_empty() {
            if natspec_lines.is_empty() {
                // Skip blank lines between function and potential NatSpec
                continue;
            } else {
                // End of NatSpec block
                break;
            }
        }
        if trimmed_line.starts_with("///") {
            natspec_lines.push(trimmed_line.to_string());
        } else {
            break;
        }
    }

    if natspec_lines.is_empty() {
        return None;
    }

    natspec_lines.reverse();
    Some(natspec_lines.join("\n"))
}

/// Parse `@param` names from a NatSpec string.
pub(crate) fn parse_natspec_params(natspec: &str) -> Vec<String> {
    let mut params = Vec::new();
    for line in natspec.lines() {
        let trimmed = line
            .trim()
            .trim_start_matches('/')
            .trim_start_matches('*')
            .trim();
        if let Some(rest) = trimmed.strip_prefix("@param") {
            let rest = rest.trim();
            // The first word after @param is the parameter name
            if let Some(name) = rest.split_whitespace().next() {
                params.push(name.to_string());
            }
        }
    }
    params
}

/// Count `@return` tags in a NatSpec string.
pub(crate) fn count_natspec_returns(natspec: &str) -> usize {
    let mut count = 0;
    for line in natspec.lines() {
        let trimmed = line
            .trim()
            .trim_start_matches('/')
            .trim_start_matches('*')
            .trim();
        if trimmed.starts_with("@return") {
            count += 1;
        }
    }
    count
}
