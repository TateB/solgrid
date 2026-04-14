//! Expression formatting — converts Solar AST `Expr` nodes to FormatChunk IR.

use crate::comments::{Comment, CommentStore};
use crate::format_ty::format_type;
use crate::ir::*;
use solar_ast::*;
use solgrid_ast::{span_text, span_to_range};
use solgrid_config::{FormatConfig, NumberUnderscore};
use solgrid_parser::solar_interface::SpannedOption;

/// Format an expression.
pub fn format_expr(
    expr: &Expr<'_>,
    source: &str,
    config: &FormatConfig,
    comments: &mut CommentStore,
) -> FormatChunk {
    match &expr.kind {
        ExprKind::Lit(lit, sub_denom) => format_literal(lit, sub_denom, source, config),
        ExprKind::Ident(ident) => text(ident.as_str()),
        ExprKind::Unary(op, operand) => {
            let op_str = span_text(source, op.span);
            if op.kind.is_postfix() {
                concat(vec![
                    format_expr(operand, source, config, comments),
                    text(op_str),
                ])
            } else {
                // Prefix operators: no space for !, ~, ++, --; space for delete
                concat(vec![
                    text(op_str),
                    format_expr(operand, source, config, comments),
                ])
            }
        }
        ExprKind::Binary(lhs, op, rhs) => match op.kind {
            BinOpKind::And | BinOpKind::Or => {
                format_binary_chain(lhs, *op, rhs, source, config, comments, true)
            }
            BinOpKind::BitAnd | BinOpKind::BitOr | BinOpKind::BitXor => {
                format_binary_chain(lhs, *op, rhs, source, config, comments, false)
            }
            _ => {
                let op_str = span_text(source, op.span);
                let rest = vec![
                    line(),
                    text(op_str),
                    space(),
                    format_expr(rhs, source, config, comments),
                ];
                if matches!(op_str, "==" | "!=" | "<" | ">" | "<=" | ">=") {
                    group(vec![
                        format_expr(lhs, source, config, comments),
                        concat(rest),
                    ])
                } else {
                    group(vec![
                        format_expr(lhs, source, config, comments),
                        indent(rest),
                    ])
                }
            }
        },
        ExprKind::Ternary(cond, if_true, if_false) => {
            let cond_chunk = format_expr(cond, source, config, comments);
            let true_chunk = format_expr(if_true, source, config, comments);
            let false_chunk = format_expr(if_false, source, config, comments);

            if span_text(source, expr.span).contains('\n') {
                concat(vec![
                    cond_chunk,
                    indent(vec![
                        hardline(),
                        text("? "),
                        true_chunk,
                        hardline(),
                        text(": "),
                        false_chunk,
                    ]),
                ])
            } else {
                group(vec![
                    cond_chunk,
                    indent(vec![
                        line(),
                        text("? "),
                        true_chunk,
                        line(),
                        text(": "),
                        false_chunk,
                    ]),
                ])
            }
        }
        ExprKind::Assign(lhs, op, rhs) => {
            let op_str = if let Some(binop) = op {
                format!("{} ", span_text(source, binop.span))
            } else {
                "= ".to_string()
            };
            group(vec![
                format_expr(lhs, source, config, comments),
                space(),
                text(op_str),
                format_expr(rhs, source, config, comments),
            ])
        }
        ExprKind::Call(callee, args) => {
            let callee_chunk = format_expr(callee, source, config, comments);
            let args_chunk = format_call_args(args, source, config, comments);
            concat(vec![callee_chunk, text("("), args_chunk, text(")")])
        }
        ExprKind::CallOptions(callee, options) => {
            let callee_chunk = format_expr(callee, source, config, comments);
            let opts: Vec<FormatChunk> = options
                .iter()
                .map(|opt| {
                    concat(vec![
                        text(opt.name.as_str()),
                        text(": "),
                        format_expr(opt.value, source, config, comments),
                    ])
                })
                .collect();
            let opts_chunk = join(opts, text(", "));
            if config.bracket_spacing {
                concat(vec![callee_chunk, text("{ "), opts_chunk, text(" }")])
            } else {
                concat(vec![callee_chunk, text("{"), opts_chunk, text("}")])
            }
        }
        ExprKind::Index(base, index) => {
            let base_chunk = format_expr(base, source, config, comments);
            let idx = match index {
                IndexKind::Index(Some(idx)) => format_expr(idx, source, config, comments),
                IndexKind::Index(None) => concat(vec![]),
                IndexKind::Range(start, end) => {
                    let s = start
                        .as_ref()
                        .map(|e| format_expr(e, source, config, comments))
                        .unwrap_or_else(|| concat(vec![]));
                    let e = end
                        .as_ref()
                        .map(|e| format_expr(e, source, config, comments))
                        .unwrap_or_else(|| concat(vec![]));
                    concat(vec![s, text(":"), e])
                }
            };
            concat(vec![base_chunk, text("["), idx, text("]")])
        }
        ExprKind::Member(base, member) => concat(vec![
            format_expr(base, source, config, comments),
            text("."),
            text(member.as_str()),
        ]),
        ExprKind::Tuple(elements) => {
            if let [SpannedOption::Some(expr)] = elements.as_ref() {
                return group(vec![
                    text("("),
                    indent(vec![format_expr(expr, source, config, comments)]),
                    text(")"),
                ]);
            }

            let items: Vec<FormatChunk> = elements
                .iter()
                .map(|elem| match elem {
                    SpannedOption::Some(e) => format_expr(e, source, config, comments),
                    SpannedOption::None(_) => concat(vec![]),
                })
                .collect();
            format_grouped_tuple(items)
        }
        ExprKind::Type(ty) => format_type(ty, source, config),
        ExprKind::TypeCall(ty) => concat(vec![
            text("type("),
            format_type(ty, source, config),
            text(")"),
        ]),
        ExprKind::New(ty) => concat(vec![text("new "), format_type(ty, source, config)]),
        ExprKind::Delete(operand) => concat(vec![
            text("delete "),
            format_expr(operand, source, config, comments),
        ]),
        ExprKind::Payable(args) => {
            let args_chunk = format_call_args(args, source, config, comments);
            concat(vec![text("payable("), args_chunk, text(")")])
        }
        ExprKind::Array(elements) => {
            let items: Vec<FormatChunk> = elements
                .iter()
                .map(|e| format_expr(e, source, config, comments))
                .collect();
            let inner = join(items, text(", "));
            group(vec![text("["), inner, text("]")])
        }
    }
}

/// Format an expression outside the main formatter comment-attachment flow.
pub fn format_expr_detached(expr: &Expr<'_>, source: &str, config: &FormatConfig) -> FormatChunk {
    let mut comments = CommentStore::new(source);
    format_expr(expr, source, config, &mut comments)
}

fn format_binary_chain(
    lhs: &Expr<'_>,
    op: BinOp,
    rhs: &Expr<'_>,
    source: &str,
    config: &FormatConfig,
    comments: &mut CommentStore,
    indent_rest: bool,
) -> FormatChunk {
    let mut parts = Vec::new();
    collect_binary_terms(lhs, op.kind, source, config, comments, &mut parts);
    collect_binary_terms(rhs, op.kind, source, config, comments, &mut parts);

    let mut iter = parts.into_iter();
    let first = iter
        .next()
        .unwrap_or_else(|| format_expr(lhs, source, config, comments));
    let op_str = span_text(source, op.span).to_string();
    let rest: Vec<FormatChunk> = iter
        .flat_map(|part| vec![line(), text(op_str.clone()), space(), part])
        .collect();

    if indent_rest {
        group(vec![first, indent(rest)])
    } else {
        group(vec![first, concat(rest)])
    }
}

fn collect_binary_terms(
    expr: &Expr<'_>,
    op_kind: BinOpKind,
    source: &str,
    config: &FormatConfig,
    comments: &mut CommentStore,
    out: &mut Vec<FormatChunk>,
) {
    if let ExprKind::Binary(lhs, op, rhs) = &expr.kind {
        if op.kind == op_kind {
            collect_binary_terms(lhs, op_kind, source, config, comments, out);
            collect_binary_terms(rhs, op_kind, source, config, comments, out);
            return;
        }
    }

    out.push(format_expr(expr, source, config, comments));
}

/// Format call arguments (positional or named).
pub fn format_call_args(
    args: &CallArgs<'_>,
    source: &str,
    config: &FormatConfig,
    comments: &mut CommentStore,
) -> FormatChunk {
    match &args.kind {
        CallArgsKind::Unnamed(exprs) => {
            if exprs.is_empty() {
                return concat(vec![]);
            }
            let args_end = span_to_range(args.span).end;
            let items: Vec<(FormatChunk, Vec<Comment>)> = exprs
                .iter()
                .enumerate()
                .map(|(index, expr)| {
                    let next_start = exprs
                        .get(index + 1)
                        .map(|next| span_to_range(next.span).start)
                        .unwrap_or(args_end);
                    take_expr_with_attached_comments(expr, next_start, source, config, comments)
                })
                .collect();
            if items
                .iter()
                .any(|(_, trailing)| has_line_comment(trailing.as_slice()))
            {
                format_multiline_items_with_comments(items)
            } else {
                format_grouped_items(
                    items
                        .into_iter()
                        .map(|(item, trailing)| attach_inline_comments(item, trailing))
                        .collect(),
                )
            }
        }
        CallArgsKind::Named(named) => {
            if named.is_empty() {
                return concat(vec![]);
            }
            let args_end = span_to_range(args.span).end;
            let items: Vec<(FormatChunk, Vec<Comment>)> = named
                .iter()
                .enumerate()
                .map(|(index, arg)| {
                    let item = concat(vec![
                        text(arg.name.as_str()),
                        text(": "),
                        format_expr(arg.value, source, config, comments),
                    ]);
                    let next_start = named
                        .get(index + 1)
                        .map(|next| span_to_range(next.name.span).start)
                        .unwrap_or(args_end);
                    let trailing =
                        comments.take_within(span_to_range(arg.value.span).end..next_start);
                    (item, trailing)
                })
                .collect();
            if items
                .iter()
                .any(|(_, trailing)| has_line_comment(trailing.as_slice()))
            {
                concat(vec![
                    text("{"),
                    format_multiline_items_with_comments(items),
                    text("}"),
                ])
            } else {
                let items: Vec<FormatChunk> = items
                    .into_iter()
                    .map(|(item, trailing)| attach_inline_comments(item, trailing))
                    .collect();
                if config.bracket_spacing {
                    concat(vec![text("{ "), join(items, text(", ")), text(" }")])
                } else {
                    concat(vec![text("{"), join(items, text(", ")), text("}")])
                }
            }
        }
    }
}

pub fn format_grouped_items(items: Vec<FormatChunk>) -> FormatChunk {
    group(vec![
        indent(vec![
            softline(),
            join(items, concat(vec![text(","), line()])),
        ]),
        softline(),
    ])
}

pub(crate) fn format_multiline_items_with_comments(
    items: Vec<(FormatChunk, Vec<Comment>)>,
) -> FormatChunk {
    let len = items.len();
    let mut chunks = Vec::new();

    for (index, (item, trailing)) in items.into_iter().enumerate() {
        let has_next = index + 1 < len;
        let (item_chunk, separator_before_comment) =
            attach_comments_with_separator(item, trailing, has_next);
        chunks.push(item_chunk);
        if has_next {
            if !separator_before_comment {
                chunks.push(text(","));
            }
            chunks.push(hardline());
        }
    }

    group(vec![indent(vec![hardline(), concat(chunks)]), hardline()])
}

pub(crate) fn attach_inline_comments(item: FormatChunk, trailing: Vec<Comment>) -> FormatChunk {
    let mut parts = vec![item];
    for comment in trailing {
        parts.push(space());
        parts.push(FormatChunk::Comment(comment.kind, comment.content));
    }
    concat(parts)
}

fn attach_comments_with_separator(
    item: FormatChunk,
    trailing: Vec<Comment>,
    has_next: bool,
) -> (FormatChunk, bool) {
    let mut parts = vec![item];

    if has_next {
        if let Some(line_index) = trailing.iter().position(|comment| {
            matches!(
                comment.kind,
                crate::ir::CommentKind::Line | crate::ir::CommentKind::DocLine
            )
        }) {
            for comment in trailing.iter().take(line_index) {
                parts.push(space());
                parts.push(FormatChunk::Comment(comment.kind, comment.content.clone()));
            }
            parts.push(text(","));
            for comment in trailing.iter().skip(line_index) {
                parts.push(space());
                parts.push(FormatChunk::Comment(comment.kind, comment.content.clone()));
            }
            return (concat(parts), true);
        }
    }

    for comment in trailing {
        parts.push(space());
        parts.push(FormatChunk::Comment(comment.kind, comment.content));
    }

    (concat(parts), false)
}

fn has_line_comment(trailing: &[Comment]) -> bool {
    trailing.iter().any(|comment| {
        matches!(
            comment.kind,
            crate::ir::CommentKind::Line | crate::ir::CommentKind::DocLine
        )
    })
}

fn take_expr_with_attached_comments(
    expr: &Expr<'_>,
    comments_end: usize,
    source: &str,
    config: &FormatConfig,
    comments: &mut CommentStore,
) -> (FormatChunk, Vec<Comment>) {
    let item = format_expr(expr, source, config, comments);
    let trailing = comments.take_within(span_to_range(expr.span).end..comments_end);
    (item, trailing)
}

pub fn format_grouped_tuple(items: Vec<FormatChunk>) -> FormatChunk {
    group(vec![text("("), format_grouped_items(items), text(")")])
}

/// Format a literal value.
fn format_literal(
    lit: &Lit<'_>,
    sub_denom: &Option<SubDenomination>,
    source: &str,
    config: &FormatConfig,
) -> FormatChunk {
    // For literals, we generally use the source text to preserve the original representation,
    // but apply transformations for single_quote and number_underscore.
    let raw = span_text(source, lit.span);

    let formatted = match &lit.kind {
        LitKind::Str(str_kind, _, _) => format_string_literal(raw, *str_kind, config),
        LitKind::Number(_) => format_number_literal(raw, config),
        _ => raw.to_string(),
    };

    let mut result = text(formatted.clone());

    if let Some(denom) = sub_denom {
        let denom_str = match denom {
            SubDenomination::Ether(e) => match e {
                EtherSubDenomination::Wei => "wei",
                EtherSubDenomination::Gwei => "gwei",
                EtherSubDenomination::Ether => "ether",
            },
            SubDenomination::Time(t) => match t {
                TimeSubDenomination::Seconds => "seconds",
                TimeSubDenomination::Minutes => "minutes",
                TimeSubDenomination::Hours => "hours",
                TimeSubDenomination::Days => "days",
                TimeSubDenomination::Weeks => "weeks",
                TimeSubDenomination::Years => "years",
            },
        };
        if !formatted.split_whitespace().any(|part| part == denom_str) {
            result = concat(vec![result, space(), text(denom_str)]);
        }
    }

    result
}

/// Apply single_quote config to string literals.
fn format_string_literal(raw: &str, kind: StrKind, config: &FormatConfig) -> String {
    // Only transform regular strings, not hex or unicode
    if kind != StrKind::Str {
        return raw.to_string();
    }

    if config.single_quote {
        // Convert "..." to '...'
        if raw.starts_with('"') && raw.ends_with('"') {
            let inner = &raw[1..raw.len() - 1];
            // Unescape double quotes, escape single quotes
            let inner = inner.replace("\\'", "'").replace('\'', "\\'");
            return format!("'{inner}'");
        }
    } else {
        // Convert '...' to "..."
        if raw.starts_with('\'') && raw.ends_with('\'') {
            let inner = &raw[1..raw.len() - 1];
            let inner = inner.replace("\\\"", "\"").replace('"', "\\\"");
            return format!("\"{inner}\"");
        }
    }

    raw.to_string()
}

/// Apply number_underscore config to number literals.
fn format_number_literal(raw: &str, config: &FormatConfig) -> String {
    match config.number_underscore {
        NumberUnderscore::Preserve => raw.to_string(),
        NumberUnderscore::Remove => raw.replace('_', ""),
        NumberUnderscore::Thousands => {
            // Only format decimal integers
            if raw.starts_with("0x") || raw.starts_with("0X") || raw.contains('.') {
                return raw.replace('_', "");
            }
            let clean = raw.replace('_', "");
            if clean.len() <= 3 {
                return clean;
            }
            // Insert underscores every 3 digits from the right
            let mut result = String::new();
            for (i, ch) in clean.chars().rev().enumerate() {
                if i > 0 && i % 3 == 0 {
                    result.push('_');
                }
                result.push(ch);
            }
            result.chars().rev().collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_number_thousands() {
        let config = FormatConfig {
            number_underscore: NumberUnderscore::Thousands,
            ..FormatConfig::default()
        };
        assert_eq!(format_number_literal("1000000", &config), "1_000_000");
        assert_eq!(format_number_literal("100", &config), "100");
        assert_eq!(format_number_literal("1000", &config), "1_000");
        assert_eq!(format_number_literal("1_00_0000", &config), "1_000_000");
    }

    #[test]
    fn test_format_number_remove() {
        let config = FormatConfig {
            number_underscore: NumberUnderscore::Remove,
            ..FormatConfig::default()
        };
        assert_eq!(format_number_literal("1_000_000", &config), "1000000");
    }

    #[test]
    fn test_format_string_single_quote() {
        let config = FormatConfig {
            single_quote: true,
            ..FormatConfig::default()
        };
        assert_eq!(
            format_string_literal("\"hello\"", StrKind::Str, &config),
            "'hello'"
        );
    }

    #[test]
    fn test_format_string_double_quote() {
        let config = FormatConfig {
            single_quote: false,
            ..FormatConfig::default()
        };
        assert_eq!(
            format_string_literal("'hello'", StrKind::Str, &config),
            "\"hello\""
        );
    }
}
