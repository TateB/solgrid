//! AST utility helpers for writing lint rules.
//!
//! Provides convenience functions over Solar's AST types.

pub use solgrid_parser::solar_ast;
pub use solgrid_parser::solar_interface;

use solar_ast::{ExprKind, Item, ItemKind, Stmt, StmtKind, VariableDefinition};
use solar_interface::Span;

/// Extract the byte offset range from a Solar Span.
/// Note: Solar spans use BytePos which are offsets into the SourceMap.
/// For single-file parsing, these correspond to byte offsets in the source string.
pub fn span_to_range(span: Span) -> std::ops::Range<usize> {
    span.lo().0 as usize..span.hi().0 as usize
}

/// Get the source text for a span.
pub fn span_text(source: &str, span: Span) -> &str {
    let range = span_to_range(span);
    &source[range]
}

/// Check if a string is PascalCase (CapWords).
pub fn is_pascal_case(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let first = s.chars().next().unwrap();
    if !first.is_uppercase() {
        return false;
    }
    // Must not contain underscores (except leading _)
    !s.contains('_')
}

/// Check if a string is camelCase (mixedCase).
pub fn is_camel_case(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let first = s.chars().next().unwrap();
    first.is_lowercase() && !s.contains('_')
}

/// Check if a string is UPPER_SNAKE_CASE.
pub fn is_upper_snake_case(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    s.chars()
        .all(|c| c.is_uppercase() || c.is_ascii_digit() || c == '_')
}

/// Check if a variable definition is a state variable (at contract level, not local).
pub fn is_state_variable(var: &VariableDefinition<'_>) -> bool {
    // State variables have visibility; local variables don't.
    // State variables also don't have data_location (except for storage ref types).
    var.visibility.is_some() || var.data_location.is_none()
}

/// Recursively check if a statement contains an assembly block.
pub fn contains_assembly(stmt: &Stmt<'_>) -> bool {
    matches!(stmt.kind, StmtKind::Assembly(_))
}

/// Check if an expression is a member access of a specific form like `tx.origin`.
pub fn is_member_access<'a>(
    expr: &'a solar_ast::Expr<'a>,
    obj_name: &str,
    member_name: &str,
) -> bool {
    if let ExprKind::Member(base, member) = &expr.kind {
        if member.as_str() == member_name {
            if let ExprKind::Ident(ident) = &base.kind {
                return ident.as_str() == obj_name;
            }
        }
    }
    false
}

/// Check if an expression is a call to a member function like `.call()`, `.delegatecall()`.
pub fn is_member_call(expr: &solar_ast::Expr<'_>, member_names: &[&str]) -> bool {
    if let ExprKind::Call(callee, _) = &expr.kind {
        // Could be direct member: foo.call() or with options: foo.call{value: 1}()
        let callee = match &callee.kind {
            ExprKind::CallOptions(inner, _) => inner,
            _ => callee,
        };
        if let ExprKind::Member(_, member) = &callee.kind {
            return member_names.contains(&member.as_str());
        }
    }
    false
}

/// Iterate over all items inside a contract body.
pub fn contract_items<'a>(items: &'a [Item<'a>]) -> impl Iterator<Item = &'a Item<'a>> {
    items
        .iter()
        .filter(|item| matches!(item.kind, ItemKind::Contract(_)))
        .flat_map(|item| {
            if let ItemKind::Contract(contract) = &item.kind {
                contract.body.iter().collect::<Vec<_>>()
            } else {
                vec![]
            }
        })
}
