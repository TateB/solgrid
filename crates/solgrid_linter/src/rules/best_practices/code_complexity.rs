//! Rule: best-practices/code-complexity
//!
//! Flag functions exceeding a cyclomatic complexity threshold.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::{ItemKind, Stmt, StmtKind};
use solgrid_parser::with_parsed_ast_sequential;

const DEFAULT_MAX_COMPLEXITY: usize = 7;

static META: RuleMeta = RuleMeta {
    id: "best-practices/code-complexity",
    name: "code-complexity",
    category: RuleCategory::BestPractices,
    default_severity: Severity::Warning,
    description: "function has too high cyclomatic complexity",
    fix_availability: FixAvailability::None,
};

pub struct CodeComplexityRule;

impl Rule for CodeComplexityRule {
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
                                let complexity = count_complexity(body.stmts) + 1;
                                if complexity > DEFAULT_MAX_COMPLEXITY {
                                    let name = func
                                        .header
                                        .name
                                        .map(|n| n.as_str().to_string())
                                        .unwrap_or_else(|| func.kind.to_string());
                                    let range = solgrid_ast::span_to_range(body_item.span);
                                    diagnostics.push(Diagnostic::new(
                                        META.id,
                                        format!(
                                            "function `{name}` has cyclomatic complexity of {complexity} (maximum is {DEFAULT_MAX_COMPLEXITY})"
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
            diagnostics
        });

        result.unwrap_or_default()
    }
}

fn count_complexity(stmts: &[Stmt<'_>]) -> usize {
    let mut count = 0;
    for stmt in stmts {
        count += count_stmt_complexity(stmt);
    }
    count
}

fn count_stmt_complexity(stmt: &Stmt<'_>) -> usize {
    let mut count = 0;
    match &stmt.kind {
        StmtKind::If(_, then_stmt, else_stmt) => {
            count += 1; // if branch
            count += count_stmt_complexity(then_stmt);
            if let Some(else_s) = else_stmt {
                count += count_stmt_complexity(else_s);
            }
        }
        StmtKind::For { body, .. } => {
            count += 1; // for loop
            count += count_stmt_complexity(body);
        }
        StmtKind::While(_, body) => {
            count += 1; // while loop
            count += count_stmt_complexity(body);
        }
        StmtKind::DoWhile(body, _) => {
            count += 1; // do-while loop
            count += count_stmt_complexity(body);
        }
        StmtKind::Block(block) => {
            count += count_complexity(block.stmts);
        }
        StmtKind::UncheckedBlock(block) => {
            count += count_complexity(block.stmts);
        }
        StmtKind::Try(try_stmt) => {
            count += 1; // try itself
            for clause in try_stmt.clauses.iter() {
                count += count_complexity(clause.block.stmts);
            }
        }
        _ => {}
    }
    count
}
