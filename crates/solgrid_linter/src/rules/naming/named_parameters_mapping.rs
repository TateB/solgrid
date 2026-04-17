//! Rule: naming/named-parameters-mapping
//!
//! Require named parameters on mapping definitions.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::{ItemKind, Stmt, StmtKind, Type, TypeKind, VariableDefinition};
use solgrid_parser::solar_interface::SpannedOption;
use solgrid_parser::with_parsed_ast_sequential;

static META: RuleMeta = RuleMeta {
    id: "naming/named-parameters-mapping",
    name: "named-parameters-mapping",
    category: RuleCategory::Naming,
    default_severity: Severity::Warning,
    description: "mapping key and value parameters should be named",
    fix_availability: FixAvailability::None,
};

pub struct NamedParametersMappingRule;

impl Rule for NamedParametersMappingRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let filename = ctx.path.to_string_lossy().to_string();
        with_parsed_ast_sequential(ctx.source, &filename, |source_unit| {
            let mut diagnostics = Vec::new();

            for item in source_unit.items.iter() {
                walk_item(item, &mut diagnostics);
            }

            diagnostics
        })
        .unwrap_or_default()
    }
}

fn walk_item(item: &solgrid_parser::solar_ast::Item<'_>, diagnostics: &mut Vec<Diagnostic>) {
    match &item.kind {
        ItemKind::Contract(contract) => {
            for body_item in contract.body.iter() {
                walk_item(body_item, diagnostics);
            }
        }
        ItemKind::Function(func) => {
            for param in func.header.parameters.iter() {
                check_mapping_variable(param, diagnostics);
            }
            if let Some(returns) = &func.header.returns {
                for param in returns.iter() {
                    check_mapping_variable(param, diagnostics);
                }
            }
            if let Some(body) = &func.body {
                walk_stmts(body.stmts, diagnostics);
            }
        }
        ItemKind::Struct(struct_def) => {
            for field in struct_def.fields.iter() {
                check_mapping_variable(field, diagnostics);
            }
        }
        ItemKind::Variable(variable) => {
            check_mapping_variable(variable, diagnostics);
        }
        _ => {}
    }
}

fn walk_stmts(stmts: &[Stmt<'_>], diagnostics: &mut Vec<Diagnostic>) {
    for stmt in stmts {
        walk_stmt(stmt, diagnostics);
    }
}

fn walk_stmt(stmt: &Stmt<'_>, diagnostics: &mut Vec<Diagnostic>) {
    match &stmt.kind {
        StmtKind::DeclSingle(variable) => {
            check_mapping_variable(variable, diagnostics);
        }
        StmtKind::DeclMulti(var_defs, _) => {
            for decl in var_defs.iter() {
                if let SpannedOption::Some(variable) = decl {
                    check_mapping_variable(variable, diagnostics);
                }
            }
        }
        StmtKind::Block(block) => {
            walk_stmts(block.stmts, diagnostics);
        }
        StmtKind::UncheckedBlock(block) => {
            walk_stmts(block.stmts, diagnostics);
        }
        StmtKind::If(_, then_stmt, else_stmt) => {
            walk_stmt(then_stmt, diagnostics);
            if let Some(else_stmt) = else_stmt {
                walk_stmt(else_stmt, diagnostics);
            }
        }
        StmtKind::For { init, body, .. } => {
            if let Some(init_stmt) = init {
                walk_stmt(init_stmt, diagnostics);
            }
            walk_stmt(body, diagnostics);
        }
        StmtKind::While(_, body) | StmtKind::DoWhile(body, _) => {
            walk_stmt(body, diagnostics);
        }
        StmtKind::Try(try_stmt) => {
            for clause in try_stmt.clauses.iter() {
                walk_stmts(clause.block.stmts, diagnostics);
            }
        }
        _ => {}
    }
}

fn check_mapping_variable(variable: &VariableDefinition<'_>, diagnostics: &mut Vec<Diagnostic>) {
    let label = variable
        .name
        .map(|name| format!("mapping `{}`", name.as_str()))
        .unwrap_or_else(|| "mapping".to_string());
    check_mapping_type(&variable.ty, &label, diagnostics);
}

fn check_mapping_type(ty: &Type<'_>, label: &str, diagnostics: &mut Vec<Diagnostic>) {
    let TypeKind::Mapping(mapping) = &ty.kind else {
        return;
    };

    if mapping.key_name.is_none() {
        diagnostics.push(Diagnostic::new(
            META.id,
            format!("main key parameter in {label} is not named"),
            META.default_severity,
            solgrid_ast::span_to_range(mapping.key.span),
        ));
    }

    let is_nested = matches!(mapping.value.kind, TypeKind::Mapping(_));
    if !is_nested && mapping.value_name.is_none() {
        diagnostics.push(Diagnostic::new(
            META.id,
            format!("value parameter in {label} is not named"),
            META.default_severity,
            solgrid_ast::span_to_range(mapping.value.span),
        ));
    }
}
