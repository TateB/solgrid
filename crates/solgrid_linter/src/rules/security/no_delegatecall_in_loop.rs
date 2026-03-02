//! Rule: security/no-delegatecall-in-loop
//!
//! Flag `delegatecall` usage inside loops. Using `delegatecall` in a loop
//! is dangerous because it executes code in the context of the calling contract,
//! and repeated calls can lead to unexpected state changes or exploits.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::{ItemKind, Stmt, StmtKind};
use solgrid_parser::with_parsed_ast_sequential;

static META: RuleMeta = RuleMeta {
    id: "security/no-delegatecall-in-loop",
    name: "no-delegatecall-in-loop",
    category: RuleCategory::Security,
    default_severity: Severity::Error,
    description: "`delegatecall` should not be used inside a loop",
    fix_availability: FixAvailability::None,
};

pub struct NoDelegatecallInLoopRule;

impl Rule for NoDelegatecallInLoopRule {
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
                            if let Some(body) = &func.body {
                                find_loops_with_pattern(
                                    ctx.source,
                                    body.stmts,
                                    "delegatecall",
                                    &mut diagnostics,
                                    META.id,
                                    META.default_severity,
                                    "`delegatecall` should not be used inside a loop",
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

fn find_loops_with_pattern(
    source: &str,
    stmts: &[Stmt<'_>],
    pattern: &str,
    diagnostics: &mut Vec<Diagnostic>,
    rule_id: &str,
    severity: Severity,
    message: &str,
) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::For { body, .. } => {
                let body_range = solgrid_ast::span_to_range(body.span);
                let body_text = &source[body_range.clone()];
                if body_text.contains(pattern) {
                    let mut search_from = 0;
                    while let Some(pos) = body_text[search_from..].find(pattern) {
                        let abs_pos = body_range.start + search_from + pos;
                        diagnostics.push(Diagnostic::new(
                            rule_id,
                            message,
                            severity,
                            abs_pos..abs_pos + pattern.len(),
                        ));
                        search_from += pos + pattern.len();
                    }
                }
                // Recurse into the body to find nested loops
                if let StmtKind::Block(block) = &body.kind {
                    find_loops_with_pattern(
                        source, block.stmts, pattern, diagnostics, rule_id, severity, message,
                    );
                }
            }
            StmtKind::While(_, body) | StmtKind::DoWhile(body, _) => {
                let body_range = solgrid_ast::span_to_range(body.span);
                let body_text = &source[body_range.clone()];
                if body_text.contains(pattern) {
                    let mut search_from = 0;
                    while let Some(pos) = body_text[search_from..].find(pattern) {
                        let abs_pos = body_range.start + search_from + pos;
                        diagnostics.push(Diagnostic::new(
                            rule_id,
                            message,
                            severity,
                            abs_pos..abs_pos + pattern.len(),
                        ));
                        search_from += pos + pattern.len();
                    }
                }
                if let StmtKind::Block(block) = &body.kind {
                    find_loops_with_pattern(
                        source, block.stmts, pattern, diagnostics, rule_id, severity, message,
                    );
                }
            }
            StmtKind::Block(block) => {
                find_loops_with_pattern(
                    source, block.stmts, pattern, diagnostics, rule_id, severity, message,
                );
            }
            StmtKind::If(_, then_stmt, else_stmt) => {
                if let StmtKind::Block(block) = &then_stmt.kind {
                    find_loops_with_pattern(
                        source, block.stmts, pattern, diagnostics, rule_id, severity, message,
                    );
                }
                if let Some(else_s) = else_stmt {
                    if let StmtKind::Block(block) = &else_s.kind {
                        find_loops_with_pattern(
                            source, block.stmts, pattern, diagnostics, rule_id, severity, message,
                        );
                    }
                }
            }
            StmtKind::UncheckedBlock(block) => {
                find_loops_with_pattern(
                    source, block.stmts, pattern, diagnostics, rule_id, severity, message,
                );
            }
            _ => {}
        }
    }
}
