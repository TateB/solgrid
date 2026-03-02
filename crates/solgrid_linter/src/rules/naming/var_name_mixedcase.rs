//! Rule: naming/var-name-mixedcase
//!
//! Local variables must use mixedCase (camelCase).

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::{ItemKind, Stmt, StmtKind, VarMut};
use solgrid_parser::with_parsed_ast_sequential;

static META: RuleMeta = RuleMeta {
    id: "naming/var-name-mixedcase",
    name: "var-name-mixedcase",
    category: RuleCategory::Naming,
    default_severity: Severity::Warning,
    description: "local variable name should use mixedCase (camelCase)",
    fix_availability: FixAvailability::None,
};

pub struct VarNameMixedcaseRule;

fn walk_stmts(stmts: &[Stmt], diagnostics: &mut Vec<Diagnostic>) {
    for stmt in stmts.iter() {
        match &stmt.kind {
            StmtKind::DeclSingle(var) => {
                // Skip constants and immutables
                if var.mutability == Some(VarMut::Constant)
                    || var.mutability == Some(VarMut::Immutable)
                {
                    continue;
                }
                if let Some(name_ident) = var.name {
                    let name = name_ident.as_str();
                    // Skip variables whose names start with `_`
                    if name.starts_with('_') {
                        continue;
                    }
                    if !solgrid_ast::is_camel_case(name) {
                        let range = solgrid_ast::span_to_range(name_ident.span);
                        diagnostics.push(Diagnostic::new(
                            META.id,
                            format!(
                                "local variable `{name}` should use mixedCase (camelCase)"
                            ),
                            META.default_severity,
                            range,
                        ));
                    }
                }
            }
            StmtKind::Block(block) => {
                walk_stmts(block.stmts, diagnostics);
            }
            StmtKind::UncheckedBlock(block) => {
                walk_stmts(block.stmts, diagnostics);
            }
            StmtKind::For { body, .. } => {
                walk_stmt(body, diagnostics);
            }
            StmtKind::While(_, body) => {
                walk_stmt(body, diagnostics);
            }
            StmtKind::DoWhile(body, _) => {
                walk_stmt(body, diagnostics);
            }
            StmtKind::If(_, then_stmt, else_stmt) => {
                walk_stmt(then_stmt, diagnostics);
                if let Some(else_stmt) = else_stmt {
                    walk_stmt(else_stmt, diagnostics);
                }
            }
            _ => {}
        }
    }
}

fn walk_stmt(stmt: &Stmt, diagnostics: &mut Vec<Diagnostic>) {
    match &stmt.kind {
        StmtKind::Block(block) => {
            walk_stmts(block.stmts, diagnostics);
        }
        _ => {
            walk_stmts(std::slice::from_ref(stmt), diagnostics);
        }
    }
}

impl Rule for VarNameMixedcaseRule {
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
                                walk_stmts(body.stmts, &mut diagnostics);
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
