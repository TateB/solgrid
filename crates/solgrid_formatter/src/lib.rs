//! Minimal solgrid formatter (Phase 1 stub).
//!
//! Full chunk-based IR formatter is planned for Phase 2.
//! For now, this validates syntax and applies basic normalizations.

use solgrid_config::FormatConfig;

/// Format Solidity source code.
///
/// Phase 1: Returns the source as-is after validating syntax.
/// Phase 2 will implement the full chunk-based formatter.
pub fn format_source(source: &str, _config: &FormatConfig) -> Result<String, String> {
    // Validate syntax
    solgrid_parser::check_syntax(source, "<stdin>").map_err(|e| format!("syntax error: {e}"))?;

    // Phase 1: return source as-is
    // TODO: Implement chunk-based IR formatter
    Ok(source.to_string())
}

/// Check if source is already formatted.
pub fn check_formatted(source: &str, config: &FormatConfig) -> Result<bool, String> {
    let formatted = format_source(source, config)?;
    Ok(formatted == source)
}
