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
/// Returns true if an external call was found in these statements.
fn check_function_body(
    stmts: &[Stmt<'_>],
    state_vars: &[String],
    source: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> bool {
    let mut seen_external_call = false;

    for stmt in stmts {
        let stmt_text = solgrid_ast::span_text(source, stmt.span);

        // Check if this statement contains an external call
        if contains_external_call(stmt_text) {
            seen_external_call = true;
        }

        // If we've seen an external call, check for state variable assignments
        // (including in the same statement that contains the call)
        if seen_external_call {
            check_stmt_for_state_assignment(stmt, state_vars, source, diagnostics);
        }

        // Recursively check nested control flow for external calls
        // that might set seen_external_call for subsequent statements
        if !seen_external_call {
            seen_external_call = check_nested_for_external_call(stmt, source);
        }
    }

    seen_external_call
}

/// Check a single statement (and its nested children) for state variable
/// assignments, used when we know an external call has already been seen.
fn check_stmt_for_state_assignment(
    stmt: &Stmt<'_>,
    state_vars: &[String],
    source: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let stmt_text = solgrid_ast::span_text(source, stmt.span);

    // Don't flag the statement if it only contains external calls, not assignments
    if contains_external_call(stmt_text) && assigns_state_var(stmt_text, state_vars).is_none() {
        return;
    }

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
        return;
    }

    // Recurse into nested blocks of control flow statements
    for nested in get_nested_stmts(stmt) {
        check_stmt_for_state_assignment(nested, state_vars, source, diagnostics);
    }
}

/// Check if a statement (or its nested children) contains an external call.
fn check_nested_for_external_call(stmt: &Stmt<'_>, source: &str) -> bool {
    let stmt_text = solgrid_ast::span_text(source, stmt.span);
    if contains_external_call(stmt_text) {
        return true;
    }
    for nested in get_nested_stmts(stmt) {
        if check_nested_for_external_call(nested, source) {
            return true;
        }
    }
    false
}

/// Extract nested statements from control flow constructs.
fn get_nested_stmts<'a, 'ast>(stmt: &'a Stmt<'ast>) -> Vec<&'a Stmt<'ast>> {
    match &stmt.kind {
        StmtKind::Block(block) | StmtKind::UncheckedBlock(block) => block.stmts.iter().collect(),
        StmtKind::If(_, then_stmt, else_stmt) => {
            let mut stmts: Vec<&Stmt<'ast>> = vec![then_stmt];
            if let Some(e) = else_stmt {
                stmts.push(e);
            }
            stmts
        }
        StmtKind::While(_, body) | StmtKind::DoWhile(body, _) => {
            vec![&**body]
        }
        StmtKind::For { body, .. } => {
            vec![&**body]
        }
        StmtKind::Try(try_stmt) => try_stmt
            .clauses
            .iter()
            .flat_map(|c| c.block.stmts.iter())
            .collect(),
        _ => vec![],
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
