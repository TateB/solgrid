//! solgrid formatter — chunk-based IR Solidity formatter.
//!
//! Formats Solidity source code using a Wadler-Lindig inspired
//! intermediate representation with line-fitting.

pub mod comments;
pub mod directives;
pub mod format;
pub mod format_expr;
pub mod format_item;
pub mod format_stmt;
pub mod format_ty;
pub mod ir;
pub mod printer;

use solgrid_config::FormatConfig;

/// Format Solidity source code.
///
/// Parses the source, builds a chunk-based IR, and prints it with
/// line-fitting according to the provided configuration.
pub fn format_source(source: &str, config: &FormatConfig) -> Result<String, String> {
    solgrid_parser::with_parsed_ast_sequential(source, "<stdin>", |ast| {
        let ir = format::format_source_unit(source, ast, config);
        printer::print_chunks(&ir, config)
    })
    .map_err(|e| format!("syntax error: {e}"))
}

/// Check if source is already formatted.
pub fn check_formatted(source: &str, config: &FormatConfig) -> Result<bool, String> {
    let formatted = format_source(source, config)?;
    Ok(formatted == source)
}

/// Format source code and verify idempotency.
///
/// Returns an error if formatting is not idempotent (format(format(x)) != format(x)).
pub fn format_source_verified(source: &str, config: &FormatConfig) -> Result<String, String> {
    let first = format_source(source, config)?;
    let second = format_source(&first, config)?;
    if first != second {
        return Err("formatter is not idempotent: format(format(x)) != format(x)".into());
    }
    Ok(first)
}
