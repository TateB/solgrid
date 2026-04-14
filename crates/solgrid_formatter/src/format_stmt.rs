//! Statement formatting — converts Solar AST `Stmt` nodes to FormatChunk IR.

use crate::comments::CommentStore;
use crate::format_expr::{
    attach_inline_comments, format_call_args, format_expr, format_grouped_tuple,
    format_multiline_items_with_comments,
};
use crate::format_item::has_blank_line_between;
use crate::format_ty::{format_data_location, format_type};
use crate::ir::*;
use solar_ast::*;
use solgrid_ast::{span_text, span_to_range};
use solgrid_config::FormatConfig;
use solgrid_parser::solar_interface::SpannedOption;

/// Format a statement.
pub fn format_stmt(
    stmt: &Stmt<'_>,
    source: &str,
    config: &FormatConfig,
    comments: &mut CommentStore,
) -> FormatChunk {
    match &stmt.kind {
        StmtKind::Block(block) => format_block(block, source, config, comments),
        StmtKind::UncheckedBlock(block) => concat(vec![
            text("unchecked "),
            format_block(block, source, config, comments),
        ]),
        StmtKind::If(cond, then_stmt, else_stmt) => {
            let mut parts = vec![
                group(vec![
                    text("if ("),
                    indent(vec![
                        softline(),
                        format_condition_expr(cond, source, config, comments),
                    ]),
                    softline(),
                    text(")"),
                ]),
                space(),
                format_stmt(then_stmt, source, config, comments),
            ];
            if let Some(else_branch) = else_stmt {
                parts.push(text(" else "));
                parts.push(format_stmt(else_branch, source, config, comments));
            }
            concat(parts)
        }
        StmtKind::While(cond, body) => concat(vec![
            group(vec![
                text("while ("),
                indent(vec![
                    softline(),
                    format_condition_expr(cond, source, config, comments),
                ]),
                softline(),
                text(")"),
            ]),
            space(),
            format_stmt(body, source, config, comments),
        ]),
        StmtKind::DoWhile(body, cond) => concat(vec![
            text("do "),
            format_stmt(body, source, config, comments),
            space(),
            group(vec![
                text("while ("),
                indent(vec![
                    softline(),
                    format_condition_expr(cond, source, config, comments),
                ]),
                softline(),
                text(");"),
            ]),
        ]),
        StmtKind::For {
            init,
            cond,
            next,
            body,
        } => {
            let init_chunk = match init {
                Some(init_stmt) => format_stmt(init_stmt, source, config, comments),
                None => text(";"),
            };
            let cond_chunk = match cond {
                Some(c) => concat(vec![
                    format_condition_expr(c, source, config, comments),
                    text(";"),
                ]),
                None => text(";"),
            };
            let next_chunk = match next {
                Some(n) => format_expr(n, source, config, comments),
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
                format_stmt(body, source, config, comments),
            ])
        }
        StmtKind::Return(expr) => match expr {
            Some(e) => group(vec![
                text("return"),
                indent(vec![line(), format_expr(e, source, config, comments)]),
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
            let args_chunk = format_call_args(args, source, config, comments);
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
            let args_chunk = format_call_args(args, source, config, comments);
            concat(vec![
                text("revert "),
                path_chunk,
                text("("),
                args_chunk,
                text(");"),
            ])
        }
        StmtKind::Try(try_stmt) => format_try_stmt(try_stmt, source, config, comments),
        StmtKind::Assembly(_) => {
            comments.take_within(span_to_range(stmt.span));
            // Assembly blocks are emitted verbatim from source.
            let asm_text = span_text(source, stmt.span);
            text(asm_text)
        }
        StmtKind::Expr(expr) => {
            concat(vec![format_expr(expr, source, config, comments), text(";")])
        }
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
                let preserve_multiline = matches!(init.kind, ExprKind::Binary(..))
                    && span_text(source, var.span).contains('\n');
                parts.push(format_initializer(
                    init,
                    source,
                    config,
                    comments,
                    preserve_multiline,
                ));
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
            group(vec![
                format_grouped_tuple(var_chunks),
                text(" ="),
                indent(vec![line(), format_expr(init, source, config, comments)]),
                text(";"),
            ])
        }
    }
}

fn format_condition_expr(
    expr: &Expr<'_>,
    source: &str,
    config: &FormatConfig,
    comments: &mut CommentStore,
) -> FormatChunk {
    let range = span_to_range(expr.span);
    let inner_comments = comments.take_within(range.clone());
    if inner_comments.is_empty() {
        format_expr(expr, source, config, comments)
    } else {
        text(span_text(source, expr.span))
    }
}

/// Format a block of statements `{ ... }`.
pub fn format_block(
    block: &Block<'_>,
    source: &str,
    config: &FormatConfig,
    comments: &mut CommentStore,
) -> FormatChunk {
    if block.stmts.is_empty() {
        let inner_comments = comments.take_within(span_to_range(block.span));
        if inner_comments.is_empty() {
            return text("{}");
        }

        let mut body = Vec::new();
        for comment in &inner_comments {
            body.push(hardline());
            body.push(FormatChunk::Comment(comment.kind, comment.content.clone()));
        }
        return concat(vec![text("{"), indent(body), hardline(), text("}")]);
    }

    let mut body_parts = Vec::new();
    let mut prev_stmt_end: Option<usize> = None;
    for stmt in block.stmts.iter() {
        let stmt_range = span_to_range(stmt.span);
        let need_blank_line = prev_stmt_end
            .map(|end| has_blank_line_between(source, end, stmt_range.start))
            .unwrap_or(false);

        // Emit leading comments for this statement
        let leading = comments.take_leading(stmt_range.start);
        let prefix_lines = if prev_stmt_end.is_some() {
            1 + usize::from(need_blank_line)
        } else {
            1
        };

        if leading.is_empty() {
            for _ in 0..prefix_lines {
                body_parts.push(hardline());
            }
        } else {
            for (i, comment) in leading.iter().enumerate() {
                if i == 0 {
                    for _ in 0..prefix_lines {
                        body_parts.push(hardline());
                    }
                } else {
                    body_parts.push(hardline());
                    if has_blank_line_between(source, leading[i - 1].range.end, comment.range.start)
                    {
                        body_parts.push(hardline());
                    }
                }
                body_parts.push(FormatChunk::Comment(comment.kind, comment.content.clone()));
            }

            body_parts.push(hardline());
            if leading.last().is_some_and(|comment| {
                has_blank_line_between(source, comment.range.end, stmt_range.start)
            }) {
                body_parts.push(hardline());
            }
        }

        body_parts.push(format_stmt(stmt, source, config, comments));

        // Trailing comments on the same line
        let trailing = comments.take_trailing(source, stmt_range.end);
        for comment in &trailing {
            body_parts.push(space());
            body_parts.push(FormatChunk::Comment(comment.kind, comment.content.clone()));
        }

        prev_stmt_end = Some(stmt_range.end);
    }

    concat(vec![text("{"), indent(body_parts), hardline(), text("}")])
}

fn format_initializer(
    expr: &Expr<'_>,
    source: &str,
    config: &FormatConfig,
    comments: &mut CommentStore,
    preserve_multiline: bool,
) -> FormatChunk {
    let value = format_expr(expr, source, config, comments);
    if preserve_multiline {
        concat(vec![text(" ="), indent(vec![hardline(), value])])
    } else if matches!(expr.kind, ExprKind::Ternary(..)) {
        concat(vec![text(" = "), value])
    } else {
        group(vec![text(" ="), indent(vec![line(), value])])
    }
}

/// Format a try statement.
fn format_try_stmt(
    try_stmt: &StmtTry<'_>,
    source: &str,
    config: &FormatConfig,
    comments: &mut CommentStore,
) -> FormatChunk {
    let mut parts = vec![
        text("try "),
        format_expr(try_stmt.expr, source, config, comments),
    ];

    // StmtTry has `clauses`: the first clause is the success block,
    // subsequent clauses are catch blocks.
    for (i, clause) in try_stmt.clauses.iter().enumerate() {
        if i == 0 {
            // Success clause — may have returns
            if !clause.args.is_empty() {
                parts.push(text(" returns ("));
                let params = format_params(&clause.args, source, config, comments);
                parts.push(params);
                parts.push(text(")"));
            }
            parts.push(space());
            parts.push(format_block(&clause.block, source, config, comments));
        } else {
            // Catch clause
            parts.push(text(" catch"));
            if let Some(name) = &clause.name {
                parts.push(space());
                parts.push(text(name.as_str()));
            }
            if !clause.args.is_empty() {
                if clause.name.is_none() {
                    parts.push(space());
                }
                parts.push(text("("));
                let params = format_params(&clause.args, source, config, comments);
                parts.push(params);
                parts.push(text(")"));
            }
            parts.push(space());
            parts.push(format_block(&clause.block, source, config, comments));
        }
    }

    concat(parts)
}

/// Format a parameter list.
pub fn format_params(
    params: &ParameterList<'_>,
    source: &str,
    config: &FormatConfig,
    comments: &mut CommentStore,
) -> FormatChunk {
    let mut force_multiline = false;
    let params_end = span_to_range(params.span).end;
    let items: Vec<(FormatChunk, Vec<crate::comments::Comment>)> = params
        .iter()
        .enumerate()
        .map(|(index, p)| {
            let mut parts = vec![format_type(&p.ty, source, config)];
            if let Some(loc) = &p.data_location {
                parts.push(space());
                parts.push(text(crate::format_ty::format_data_location(*loc)));
            }
            if let Some(name) = &p.name {
                parts.push(space());
                parts.push(text(name.as_str()));
            }
            let item = concat(parts);
            let next_start = params
                .get(index + 1)
                .map(|next| span_to_range(next.span).start)
                .unwrap_or(params_end);
            let trailing = comments.take_within(span_to_range(p.span).end..next_start);
            if !trailing.is_empty() {
                force_multiline = true;
            }
            (item, trailing)
        })
        .collect();
    if force_multiline {
        format_multiline_items_with_comments(items)
    } else {
        let items: Vec<FormatChunk> = items
            .into_iter()
            .map(|(item, trailing)| attach_inline_comments(item, trailing))
            .collect();
        group(vec![
            indent(vec![
                softline(),
                join(items, concat(vec![text(","), line()])),
            ]),
            softline(),
        ])
    }
}
