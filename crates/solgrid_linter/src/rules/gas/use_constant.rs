//! Rule: gas/use-constant
//!
//! State variables with compile-time-known values that are never reassigned
//! should be declared as `constant` to save gas. Constants are replaced
//! inline at compile time and avoid SLOAD operations.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::ItemKind;
use solgrid_parser::with_parsed_ast_sequential;

static META: RuleMeta = RuleMeta {
    id: "gas/use-constant",
    name: "use-constant",
    category: RuleCategory::Gas,
    default_severity: Severity::Info,
    description: "state variable with compile-time-known value should be `constant`",
    fix_availability: FixAvailability::Available(FixSafety::Suggestion),
};

pub struct UseConstantRule;

impl Rule for UseConstantRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let filename = ctx.path.to_string_lossy().to_string();
        let result = with_parsed_ast_sequential(ctx.source, &filename, |source_unit| {
            let mut diagnostics = Vec::new();
            for item in source_unit.items.iter() {
                if let ItemKind::Contract(contract) = &item.kind {
                    let contract_range = solgrid_ast::span_to_range(item.span);
                    let contract_text = &ctx.source[contract_range.clone()];

                    // Collect state variables with literal initializers
                    for body_item in contract.body.iter() {
                        if let ItemKind::Variable(var) = &body_item.kind {
                            let var_range = solgrid_ast::span_to_range(body_item.span);
                            let var_text = &ctx.source[var_range.clone()];

                            // Skip if already constant or immutable
                            if var_text.contains("constant") || var_text.contains("immutable") {
                                continue;
                            }

                            // Must have an initializer
                            if let Some(init) = &var.initializer {
                                let init_range = solgrid_ast::span_to_range(init.span);
                                let init_text = ctx.source[init_range].trim();

                                // Check if the initializer is a compile-time literal
                                if is_compile_time_literal(init_text) {
                                    // Get variable name
                                    let var_name = var
                                        .name
                                        .map(|n| n.as_str().to_string())
                                        .unwrap_or_default();

                                    if var_name.is_empty() {
                                        continue;
                                    }

                                    // Check that the variable is never assigned in the contract
                                    if !has_assignments(contract_text, &var_name) {
                                        let name_span =
                                            var.name.map(|n| solgrid_ast::span_to_range(n.span));
                                        let diag_range = name_span.unwrap_or(var_range.clone());

                                        diagnostics.push(
                                            Diagnostic::new(
                                                META.id,
                                                format!(
                                                    "state variable `{var_name}` has a compile-time-known value and is never reassigned; declare it as `constant` to save gas"
                                                ),
                                                META.default_severity,
                                                diag_range,
                                            ),
                                        );
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

/// Check if an expression is a compile-time literal.
fn is_compile_time_literal(text: &str) -> bool {
    let t = text.trim();
    // Numeric literal (decimal or hex)
    if t.chars()
        .next()
        .map(|c| c.is_ascii_digit())
        .unwrap_or(false)
    {
        return true;
    }
    if t.starts_with("0x") || t.starts_with("0X") {
        return true;
    }
    // Boolean
    if t == "true" || t == "false" {
        return true;
    }
    // String literal
    if t.starts_with('"') && t.ends_with('"') {
        return true;
    }
    // Address literal
    if t.starts_with("address(0)") || t.starts_with("address(0x") {
        return true;
    }
    false
}

/// Check if a variable name appears in an assignment context within the text.
fn has_assignments(contract_text: &str, var_name: &str) -> bool {
    // Look for patterns like `varName =` (but not `==`, `!=`, `<=`, `>=`)
    let assign_pattern = format!("{var_name} =");
    let mut search_from = 0;
    while let Some(pos) = contract_text[search_from..].find(&assign_pattern) {
        let abs_pos = search_from + pos;

        // Check word boundary before
        if abs_pos > 0 {
            let prev = contract_text.as_bytes()[abs_pos - 1];
            if prev.is_ascii_alphanumeric() || prev == b'_' || prev == b'.' {
                search_from = abs_pos + assign_pattern.len();
                continue;
            }
        }

        // Check it's not `==`
        let after_pattern = abs_pos + assign_pattern.len();
        if after_pattern < contract_text.len() && contract_text.as_bytes()[after_pattern] == b'=' {
            search_from = abs_pos + assign_pattern.len();
            continue;
        }

        // Check it's not part of the initial declaration (contains type before it on same line)
        let line_start = contract_text[..abs_pos]
            .rfind('\n')
            .map(|p| p + 1)
            .unwrap_or(0);
        let line = &contract_text[line_start..abs_pos];
        // If the line contains a type keyword, this might be the declaration
        let is_declaration = line.contains("uint")
            || line.contains("int ")
            || line.contains("address")
            || line.contains("bool")
            || line.contains("string")
            || line.contains("bytes")
            || line.contains("mapping");
        if is_declaration {
            search_from = abs_pos + assign_pattern.len();
            continue;
        }

        return true;
    }

    // Also check for `varName +=`, `-=`, `*=`, `/=`
    for op in &["+=", "-=", "*=", "/=", "|=", "&=", "^=", "<<=", ">>="] {
        let pattern = format!("{var_name} {op}");
        if let Some(pos) = contract_text.find(&pattern) {
            // Check word boundary
            if pos > 0 {
                let prev = contract_text.as_bytes()[pos - 1];
                if prev.is_ascii_alphanumeric() || prev == b'_' {
                    continue;
                }
            }
            return true;
        }
    }

    false
}
