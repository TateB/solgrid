//! Expression formatting — converts Solar AST `Expr` nodes to FormatChunk IR.

use crate::format_ty::format_type;
use crate::ir::*;
use solar_ast::*;
use solgrid_ast::span_text;
use solgrid_config::{FormatConfig, NumberUnderscore};
use solgrid_parser::solar_interface::SpannedOption;

/// Format an expression.
pub fn format_expr(expr: &Expr<'_>, source: &str, config: &FormatConfig) -> FormatChunk {
    match &expr.kind {
        ExprKind::Lit(lit, sub_denom) => format_literal(lit, sub_denom, source, config),
        ExprKind::Ident(ident) => text(ident.as_str()),
        ExprKind::Unary(op, operand) => {
            let op_str = span_text(source, op.span);
            if op.kind.is_postfix() {
                concat(vec![format_expr(operand, source, config), text(op_str)])
            } else {
                // Prefix operators: no space for !, ~, ++, --; space for delete
                concat(vec![text(op_str), format_expr(operand, source, config)])
            }
        }
        ExprKind::Binary(lhs, op, rhs) => {
            let op_str = span_text(source, op.span);
            group(vec![
                format_expr(lhs, source, config),
                space(),
                text(op_str),
                line(),
                format_expr(rhs, source, config),
            ])
        }
        ExprKind::Ternary(cond, if_true, if_false) => group(vec![
            format_expr(cond, source, config),
            line(),
            text("? "),
            format_expr(if_true, source, config),
            line(),
            text(": "),
            format_expr(if_false, source, config),
        ]),
        ExprKind::Assign(lhs, op, rhs) => {
            let op_str = if let Some(binop) = op {
                format!("{} ", span_text(source, binop.span))
            } else {
                "= ".to_string()
            };
            group(vec![
                format_expr(lhs, source, config),
                space(),
                text(op_str),
                format_expr(rhs, source, config),
            ])
        }
        ExprKind::Call(callee, args) => {
            let callee_chunk = format_expr(callee, source, config);
            let args_chunk = format_call_args(args, source, config);
            concat(vec![callee_chunk, text("("), args_chunk, text(")")])
        }
        ExprKind::CallOptions(callee, options) => {
            let callee_chunk = format_expr(callee, source, config);
            let opts: Vec<FormatChunk> = options
                .iter()
                .map(|opt| {
                    concat(vec![
                        text(opt.name.as_str()),
                        text(": "),
                        format_expr(opt.value, source, config),
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
            let base_chunk = format_expr(base, source, config);
            let idx = match index {
                IndexKind::Index(Some(idx)) => format_expr(idx, source, config),
                IndexKind::Index(None) => concat(vec![]),
                IndexKind::Range(start, end) => {
                    let s = start
                        .as_ref()
                        .map(|e| format_expr(e, source, config))
                        .unwrap_or_else(|| concat(vec![]));
                    let e = end
                        .as_ref()
                        .map(|e| format_expr(e, source, config))
                        .unwrap_or_else(|| concat(vec![]));
                    concat(vec![s, text(":"), e])
                }
            };
            concat(vec![base_chunk, text("["), idx, text("]")])
        }
        ExprKind::Member(base, member) => concat(vec![
            format_expr(base, source, config),
            text("."),
            text(member.as_str()),
        ]),
        ExprKind::Tuple(elements) => {
            let items: Vec<FormatChunk> = elements
                .iter()
                .map(|elem| match elem {
                    SpannedOption::Some(e) => format_expr(e, source, config),
                    SpannedOption::None(_) => concat(vec![]),
                })
                .collect();
            let inner = join(items, text(", "));
            group(vec![text("("), inner, text(")")])
        }
        ExprKind::Type(ty) => concat(vec![
            text("type("),
            format_type(ty, source, config),
            text(")"),
        ]),
        ExprKind::TypeCall(ty) => concat(vec![
            text("type("),
            format_type(ty, source, config),
            text(")"),
        ]),
        ExprKind::New(ty) => concat(vec![text("new "), format_type(ty, source, config)]),
        ExprKind::Delete(operand) => {
            concat(vec![text("delete "), format_expr(operand, source, config)])
        }
        ExprKind::Payable(args) => {
            let args_chunk = format_call_args(args, source, config);
            concat(vec![text("payable("), args_chunk, text(")")])
        }
        ExprKind::Array(elements) => {
            let items: Vec<FormatChunk> = elements
                .iter()
                .map(|e| format_expr(e, source, config))
                .collect();
            let inner = join(items, text(", "));
            group(vec![text("["), inner, text("]")])
        }
    }
}

/// Format call arguments (positional or named).
pub fn format_call_args(args: &CallArgs<'_>, source: &str, config: &FormatConfig) -> FormatChunk {
    match &args.kind {
        CallArgsKind::Unnamed(exprs) => {
            if exprs.is_empty() {
                return concat(vec![]);
            }
            let items: Vec<FormatChunk> = exprs
                .iter()
                .map(|e| format_expr(e, source, config))
                .collect();
            group(vec![
                indent(vec![
                    softline(),
                    join(items, concat(vec![text(","), line()])),
                ]),
                softline(),
            ])
        }
        CallArgsKind::Named(named) => {
            if named.is_empty() {
                return concat(vec![]);
            }
            let items: Vec<FormatChunk> = named
                .iter()
                .map(|arg| {
                    concat(vec![
                        text(arg.name.as_str()),
                        text(": "),
                        format_expr(arg.value, source, config),
                    ])
                })
                .collect();
            if config.bracket_spacing {
                concat(vec![text("{ "), join(items, text(", ")), text(" }")])
            } else {
                concat(vec![text("{"), join(items, text(", ")), text("}")])
            }
        }
    }
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

    let mut result = text(formatted);

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
        result = concat(vec![result, space(), text(denom_str)]);
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
