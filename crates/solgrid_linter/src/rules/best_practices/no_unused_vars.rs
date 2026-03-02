//! Rule: best-practices/no-unused-vars
//!
//! Detect unused local variables within function bodies.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::{ItemKind, Stmt, StmtKind};
use solgrid_parser::solar_interface::SpannedOption;
use solgrid_parser::with_parsed_ast_sequential;

static META: RuleMeta = RuleMeta {
    id: "best-practices/no-unused-vars",
    name: "no-unused-vars",
    category: RuleCategory::BestPractices,
    default_severity: Severity::Warning,
    description: "local variable is declared but never used",
    fix_availability: FixAvailability::Available(FixSafety::Suggestion),
};

pub struct NoUnusedVarsRule;

impl Rule for NoUnusedVarsRule {
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
                                let func_range = solgrid_ast::span_to_range(body_item.span);
                                let func_source = &ctx.source[func_range.start..func_range.end];

                                // Collect local variable declarations
                                let var_decls = collect_var_decls(body.stmts);

                                // Check each variable is used elsewhere in the function
                                for (name, name_range) in &var_decls {
                                    // Skip variables starting with _
                                    if name.starts_with('_') {
                                        continue;
                                    }

                                    if !is_var_used_in_function(
                                        func_source,
                                        name,
                                        name_range,
                                        func_range.start,
                                    ) {
                                        let mut diag = Diagnostic::new(
                                            META.id,
                                            format!(
                                                "local variable `{name}` is declared but never used"
                                            ),
                                            META.default_severity,
                                            name_range.clone(),
                                        );

                                        // Suggest prefixing with underscore
                                        diag = diag.with_fix(Fix::suggestion(
                                            format!("prefix `{name}` with underscore"),
                                            vec![TextEdit::replace(
                                                name_range.clone(),
                                                format!("_{name}"),
                                            )],
                                        ));

                                        diagnostics.push(diag);
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

/// Collect variable declarations from statement list.
fn collect_var_decls(stmts: &[Stmt<'_>]) -> Vec<(String, std::ops::Range<usize>)> {
    let mut decls = Vec::new();
    for stmt in stmts {
        collect_var_decls_from_stmt(stmt, &mut decls);
    }
    decls
}

fn collect_var_decls_from_stmt(
    stmt: &Stmt<'_>,
    decls: &mut Vec<(String, std::ops::Range<usize>)>,
) {
    match &stmt.kind {
        StmtKind::DeclSingle(var_def) => {
            if let Some(name_ident) = var_def.name {
                let name = name_ident.as_str().to_string();
                let range = solgrid_ast::span_to_range(name_ident.span);
                decls.push((name, range));
            }
        }
        StmtKind::DeclMulti(var_defs, _) => {
            for decl in var_defs.iter() {
                if let SpannedOption::Some(var) = decl {
                    if let Some(name_ident) = var.name {
                        let name = name_ident.as_str().to_string();
                        let range = solgrid_ast::span_to_range(name_ident.span);
                        decls.push((name, range));
                    }
                }
            }
        }
        StmtKind::Block(block) => {
            for s in block.stmts.iter() {
                collect_var_decls_from_stmt(s, decls);
            }
        }
        StmtKind::UncheckedBlock(block) => {
            for s in block.stmts.iter() {
                collect_var_decls_from_stmt(s, decls);
            }
        }
        StmtKind::If(_, then_stmt, else_stmt) => {
            collect_var_decls_from_stmt(then_stmt, decls);
            if let Some(else_s) = else_stmt {
                collect_var_decls_from_stmt(else_s, decls);
            }
        }
        StmtKind::For { body, .. } => {
            collect_var_decls_from_stmt(body, decls);
        }
        StmtKind::While(_, body) => {
            collect_var_decls_from_stmt(body, decls);
        }
        StmtKind::DoWhile(body, _) => {
            collect_var_decls_from_stmt(body, decls);
        }
        StmtKind::Try(try_stmt) => {
            for clause in try_stmt.clauses.iter() {
                for s in clause.block.stmts.iter() {
                    collect_var_decls_from_stmt(s, decls);
                }
            }
        }
        _ => {}
    }
}

/// Check if a variable name is used in the function body outside its declaration.
fn is_var_used_in_function(
    func_source: &str,
    name: &str,
    name_range: &std::ops::Range<usize>,
    func_start: usize,
) -> bool {
    let mut search_from = 0;
    while let Some(pos) = func_source[search_from..].find(name) {
        let abs_pos_in_func = search_from + pos;
        let abs_pos = func_start + abs_pos_in_func;

        // Skip if this is the declaration itself
        if abs_pos >= name_range.start && abs_pos < name_range.end {
            search_from = abs_pos_in_func + name.len();
            continue;
        }

        // Check word boundaries
        let before_ok = abs_pos_in_func == 0
            || !func_source.as_bytes()[abs_pos_in_func - 1].is_ascii_alphanumeric()
                && func_source.as_bytes()[abs_pos_in_func - 1] != b'_';

        let after_pos = abs_pos_in_func + name.len();
        let after_ok = after_pos >= func_source.len()
            || !func_source.as_bytes()[after_pos].is_ascii_alphanumeric()
                && func_source.as_bytes()[after_pos] != b'_';

        if before_ok && after_ok {
            return true;
        }

        search_from = abs_pos_in_func + name.len();
    }
    false
}
