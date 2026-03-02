//! Rule: security/reentrancy
//!
//! Detect state changes after external calls (Check-Effects-Interactions
//! pattern violation). Uses a simplified heuristic: within each function body,
//! flag any state variable assignment that follows an external call.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::{ItemKind, Stmt, StmtKind};
use solgrid_parser::with_parsed_ast_sequential;

static META: RuleMeta = RuleMeta {
    id: "security/reentrancy",
    name: "reentrancy",
    category: RuleCategory::Security,
    default_severity: Severity::Error,
    description: "possible reentrancy vulnerability: state change after external call",
    fix_availability: FixAvailability::None,
};

pub struct ReentrancyRule;

/// External call patterns to detect.
const EXTERNAL_CALL_PATTERNS: &[&str] =
    &[".call(", ".call{", ".send(", ".transfer(", ".delegatecall("];

impl Rule for ReentrancyRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let filename = ctx.path.to_string_lossy().to_string();
        let result = with_parsed_ast_sequential(ctx.source, &filename, |source_unit| {
            let mut diagnostics = Vec::new();

            for item in source_unit.items.iter() {
                if let ItemKind::Contract(contract) = &item.kind {
                    // Collect state variable names (owned strings for lifetime safety)
                    let state_vars: Vec<String> = contract
                        .body
                        .iter()
                        .filter_map(|body_item| {
                            if let ItemKind::Variable(var) = &body_item.kind {
                                var.name.map(|n| n.as_str().to_string())
                            } else {
                                None
                            }
                        })
                        .collect();

                    if state_vars.is_empty() {
                        continue;
                    }

                    // Check each function body
                    for body_item in contract.body.iter() {
                        if let ItemKind::Function(func) = &body_item.kind {
                            if let Some(body) = &func.body {
                                check_function_body(
                                    body.stmts,
                                    &state_vars,
                                    ctx.source,
                                    &mut diagnostics,
                                );
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

/// Check a function body for CEI violations.
/// Walk statements linearly, tracking whether an external call has been seen.
fn check_function_body(
    stmts: &[Stmt<'_>],
    state_vars: &[String],
    source: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut seen_external_call = false;

    for stmt in stmts {
        let stmt_text = solgrid_ast::span_text(source, stmt.span);

        // Check if this statement contains an external call
        if !seen_external_call && contains_external_call(stmt_text) {
            seen_external_call = true;
            continue;
        }

        // If we've seen an external call, check for state variable assignments
        if seen_external_call {
            if let Some(var_name) = assigns_state_var(stmt_text, state_vars) {
                let range = solgrid_ast::span_to_range(stmt.span);
                diagnostics.push(Diagnostic::new(
                    META.id,
                    format!(
                        "state variable `{var_name}` is modified after an external call (CEI violation)"
                    ),
                    META.default_severity,
                    range,
                ));
            }
        }

        // Recursively check nested blocks
        if let StmtKind::Block(block) = &stmt.kind {
            if seen_external_call {
                for s in block.stmts.iter() {
                    let s_text = solgrid_ast::span_text(source, s.span);
                    if let Some(var_name) = assigns_state_var(s_text, state_vars) {
                        let range = solgrid_ast::span_to_range(s.span);
                        diagnostics.push(Diagnostic::new(
                            META.id,
                            format!(
                                "state variable `{var_name}` is modified after an external call (CEI violation)"
                            ),
                            META.default_severity,
                            range,
                        ));
                    }
                }
            }
        }
    }
}

/// Check if a statement text contains an external call pattern.
fn contains_external_call(stmt_text: &str) -> bool {
    for pattern in EXTERNAL_CALL_PATTERNS {
        if stmt_text.contains(pattern) {
            return true;
        }
    }
    false
}

/// Check if a statement text assigns to a state variable.
/// Returns the variable name if found.
fn assigns_state_var<'a>(stmt_text: &str, state_vars: &'a [String]) -> Option<&'a str> {
    for var in state_vars {
        // Check for direct assignment: varName = ...
        // Also check: varName += ..., varName -= ..., etc.
        let assignment_patterns = [
            format!("{var} ="),
            format!("{var} +="),
            format!("{var} -="),
            format!("{var} *="),
            format!("{var} /="),
            format!("{var}["), // array element assignment
            format!("{var}++"),
            format!("{var}--"),
            format!("++{var}"),
            format!("--{var}"),
        ];

        for pattern in &assignment_patterns {
            if let Some(pos) = stmt_text.find(pattern.as_str()) {
                let before_ok = pos == 0
                    || !stmt_text.as_bytes()[pos - 1].is_ascii_alphanumeric()
                        && stmt_text.as_bytes()[pos - 1] != b'_';
                if before_ok {
                    return Some(var);
                }
            }
        }
    }
    None
}
