//! Statement formatting — converts Solar AST `Stmt` nodes to FormatChunk IR.

use crate::format_expr::{format_call_args, format_expr};
use crate::format_ty::{format_data_location, format_type};
use crate::ir::*;
use solar_ast::*;
use solgrid_ast::span_text;
use solgrid_config::FormatConfig;
use solgrid_parser::solar_interface::SpannedOption;

/// Format a statement.
pub fn format_stmt(stmt: &Stmt<'_>, source: &str, config: &FormatConfig) -> FormatChunk {
    match &stmt.kind {
        StmtKind::Block(block) => format_block(block, source, config),
        StmtKind::UncheckedBlock(block) => concat(vec![
            text("unchecked "),
            format_block(block, source, config),
        ]),
        StmtKind::If(cond, then_stmt, else_stmt) => {
            let mut parts = vec![
                text("if ("),
                format_expr(cond, source, config),
                text(") "),
                format_stmt(then_stmt, source, config),
            ];
            if let Some(else_branch) = else_stmt {
                parts.push(text(" else "));
                parts.push(format_stmt(else_branch, source, config));
            }
            concat(parts)
        }
        StmtKind::While(cond, body) => concat(vec![
            text("while ("),
            format_expr(cond, source, config),
            text(") "),
            format_stmt(body, source, config),
        ]),
        StmtKind::DoWhile(body, cond) => concat(vec![
            text("do "),
            format_stmt(body, source, config),
            text(" while ("),
            format_expr(cond, source, config),
            text(");"),
        ]),
        StmtKind::For {
            init,
            cond,
            next,
            body,
        } => {
            let init_chunk = match init {
                Some(init_stmt) => format_stmt(init_stmt, source, config),
                None => text(";"),
            };
            let cond_chunk = match cond {
                Some(c) => concat(vec![format_expr(c, source, config), text(";")]),
                None => text(";"),
            };
            let next_chunk = match next {
                Some(n) => format_expr(n, source, config),
                None => concat(vec![]),
            };
            group(vec![
                text("for ("),
                init_chunk,
                space(),
                cond_chunk,
                space(),
                next_chunk,
                text(") "),
                format_stmt(body, source, config),
            ])
        }
        StmtKind::Return(expr) => match expr {
            Some(e) => group(vec![
                text("return "),
                format_expr(e, source, config),
                text(";"),
            ]),
            None => text("return;"),
        },
        StmtKind::Emit(path, args) => {
            let path_str: Vec<FormatChunk> = path
                .segments()
                .iter()
                .map(|seg| text(seg.as_str()))
                .collect();
            let path_chunk = join(path_str, text("."));
            let args_chunk = format_call_args(args, source, config);
            concat(vec![
                text("emit "),
                path_chunk,
                text("("),
                args_chunk,
                text(");"),
            ])
        }
        StmtKind::Revert(path, args) => {
            let path_str: Vec<FormatChunk> = path
                .segments()
                .iter()
                .map(|seg| text(seg.as_str()))
                .collect();
            let path_chunk = join(path_str, text("."));
            let args_chunk = format_call_args(args, source, config);
            concat(vec![
                text("revert "),
                path_chunk,
                text("("),
                args_chunk,
                text(");"),
            ])
        }
        StmtKind::Try(try_stmt) => format_try_stmt(try_stmt, source, config),
        StmtKind::Assembly(_) => {
            // Assembly blocks are emitted verbatim from source.
            let asm_text = span_text(source, stmt.span);
            text(asm_text)
        }
        StmtKind::Expr(expr) => concat(vec![format_expr(expr, source, config), text(";")]),
        StmtKind::Break => text("break;"),
        StmtKind::Continue => text("continue;"),
        StmtKind::Placeholder => text("_;"),
        StmtKind::DeclSingle(var) => {
            let mut parts = vec![format_type(&var.ty, source, config)];
            if let Some(loc) = &var.data_location {
                parts.push(space());
                parts.push(text(format_data_location(*loc)));
            }
            if let Some(name) = &var.name {
                parts.push(space());
                parts.push(text(name.as_str()));
            }
            if let Some(init) = &var.initializer {
                parts.push(text(" = "));
                parts.push(format_expr(init, source, config));
            }
            parts.push(text(";"));
            concat(parts)
        }
        StmtKind::DeclMulti(vars, init) => {
            let var_chunks: Vec<FormatChunk> = vars
                .iter()
                .map(|var| match var {
                    SpannedOption::Some(v) => {
                        let mut parts = vec![format_type(&v.ty, source, config)];
                        if let Some(loc) = &v.data_location {
                            parts.push(space());
                            parts.push(text(format_data_location(*loc)));
                        }
                        if let Some(name) = &v.name {
                            parts.push(space());
                            parts.push(text(name.as_str()));
                        }
                        concat(parts)
                    }
                    SpannedOption::None(_) => concat(vec![]),
                })
                .collect();
            concat(vec![
                text("("),
                join(var_chunks, text(", ")),
                text(") = "),
                format_expr(init, source, config),
                text(";"),
            ])
        }
    }
}

/// Format a block of statements `{ ... }`.
pub fn format_block(block: &Block<'_>, source: &str, config: &FormatConfig) -> FormatChunk {
    if block.stmts.is_empty() {
        return text("{}");
    }

    let mut body_parts = Vec::new();
    for stmt in block.stmts.iter() {
        body_parts.push(hardline());
        body_parts.push(format_stmt(stmt, source, config));
    }

    concat(vec![text("{"), indent(body_parts), hardline(), text("}")])
}

/// Format a try statement.
fn format_try_stmt(try_stmt: &StmtTry<'_>, source: &str, config: &FormatConfig) -> FormatChunk {
    let mut parts = vec![text("try "), format_expr(try_stmt.expr, source, config)];

    // StmtTry has `clauses`: the first clause is the success block,
    // subsequent clauses are catch blocks.
    for (i, clause) in try_stmt.clauses.iter().enumerate() {
        if i == 0 {
            // Success clause — may have returns
            if !clause.args.is_empty() {
                parts.push(text(" returns ("));
                let params = format_params(&clause.args, source, config);
                parts.push(params);
                parts.push(text(")"));
            }
            parts.push(space());
            parts.push(format_block(&clause.block, source, config));
        } else {
            // Catch clause
            parts.push(text(" catch"));
            if let Some(name) = &clause.name {
                parts.push(space());
                parts.push(text(name.as_str()));
            }
            parts.push(text("("));
            let params = format_params(&clause.args, source, config);
            parts.push(params);
            parts.push(text(") "));
            parts.push(format_block(&clause.block, source, config));
        }
    }

    concat(parts)
}

/// Format a parameter list.
pub fn format_params(
    params: &ParameterList<'_>,
    source: &str,
    config: &FormatConfig,
) -> FormatChunk {
    let items: Vec<FormatChunk> = params
        .iter()
        .map(|p| {
            let mut parts = vec![format_type(&p.ty, source, config)];
            if let Some(loc) = &p.data_location {
                parts.push(space());
                parts.push(text(crate::format_ty::format_data_location(*loc)));
            }
            if let Some(name) = &p.name {
                parts.push(space());
                parts.push(text(name.as_str()));
            }
            concat(parts)
        })
        .collect();
    join(items, text(", "))
}
