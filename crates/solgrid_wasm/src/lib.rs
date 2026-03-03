use serde::{Deserialize, Serialize};
use std::path::Path;
use wasm_bindgen::prelude::*;

use solgrid_config::{Config, FormatConfig};
use solgrid_diagnostics::{Diagnostic, Severity};
use solgrid_linter::LintEngine;

/// Serializable diagnostic for WASM consumers.
#[derive(Serialize, Deserialize)]
struct WasmDiagnostic {
    rule_id: String,
    message: String,
    severity: String,
    span_start: usize,
    span_end: usize,
    has_fix: bool,
}

impl From<&Diagnostic> for WasmDiagnostic {
    fn from(d: &Diagnostic) -> Self {
        WasmDiagnostic {
            rule_id: d.rule_id.clone(),
            message: d.message.clone(),
            severity: match d.severity {
                Severity::Error => "error".to_string(),
                Severity::Warning => "warning".to_string(),
                Severity::Info => "info".to_string(),
            },
            span_start: d.span.start,
            span_end: d.span.end,
            has_fix: d.fix.is_some(),
        }
    }
}

/// Serializable lint result for WASM consumers.
#[derive(Serialize, Deserialize)]
struct WasmLintResult {
    diagnostics: Vec<WasmDiagnostic>,
    diagnostic_count: usize,
    error_count: usize,
    warning_count: usize,
    info_count: usize,
}

/// Lint Solidity source code and return diagnostics as JSON.
///
/// # Arguments
/// * `source` — Solidity source code
/// * `config_json` — JSON-encoded solgrid configuration (or empty string for defaults)
///
/// # Returns
/// JSON string with lint diagnostics
#[wasm_bindgen]
pub fn lint(source: &str, config_json: &str) -> String {
    let config = parse_config(config_json);
    let engine = LintEngine::new();
    let result = engine.lint_source(source, Path::new("input.sol"), &config);

    let diagnostics: Vec<WasmDiagnostic> = result.diagnostics.iter().map(Into::into).collect();
    let error_count = result
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .count();
    let warning_count = result
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Warning)
        .count();
    let info_count = result
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Info)
        .count();

    let wasm_result = WasmLintResult {
        diagnostic_count: diagnostics.len(),
        diagnostics,
        error_count,
        warning_count,
        info_count,
    };

    serde_json::to_string(&wasm_result)
        .unwrap_or_else(|e| format!(r#"{{"error":"serialization failed: {}"}}"#, e))
}

/// Lint and auto-fix Solidity source code.
///
/// # Arguments
/// * `source` — Solidity source code
/// * `config_json` — JSON-encoded solgrid configuration (or empty string for defaults)
/// * `include_unsafe` — whether to include suggestion-level fixes
///
/// # Returns
/// JSON string with `fixed_source` and `diagnostics` (remaining after fix)
#[wasm_bindgen]
pub fn fix(source: &str, config_json: &str, include_unsafe: bool) -> String {
    let config = parse_config(config_json);
    let engine = LintEngine::new();
    let (fixed, result) =
        engine.fix_source(source, Path::new("input.sol"), &config, include_unsafe);

    let diagnostics: Vec<WasmDiagnostic> = result.diagnostics.iter().map(Into::into).collect();

    #[derive(Serialize)]
    struct FixResult {
        fixed_source: String,
        diagnostics: Vec<WasmDiagnostic>,
        diagnostic_count: usize,
    }

    let fix_result = FixResult {
        fixed_source: fixed,
        diagnostic_count: diagnostics.len(),
        diagnostics,
    };

    serde_json::to_string(&fix_result)
        .unwrap_or_else(|e| format!(r#"{{"error":"serialization failed: {}"}}"#, e))
}

/// Format Solidity source code.
///
/// # Arguments
/// * `source` — Solidity source code
/// * `config_json` — JSON-encoded format configuration (or empty string for defaults)
///
/// # Returns
/// Formatted source code, or a JSON error object on failure
#[wasm_bindgen]
pub fn format(source: &str, config_json: &str) -> String {
    let format_config = parse_format_config(config_json);

    match solgrid_formatter::format_source(source, &format_config) {
        Ok(formatted) => formatted,
        Err(e) => format!(r#"{{"error":"{}"}}"#, e.replace('"', "\\\"")),
    }
}

/// Return the solgrid version string.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// List all available lint rules as JSON.
///
/// # Returns
/// JSON array of rule objects with `id`, `category`, `description`, `severity`, and `has_fix` fields
#[wasm_bindgen]
pub fn list_rules() -> String {
    let engine = LintEngine::new();
    let rules: Vec<serde_json::Value> = engine
        .registry()
        .rules()
        .iter()
        .map(|r| {
            let meta = r.meta();
            serde_json::json!({
                "id": meta.id,
                "name": meta.name,
                "category": meta.category.as_str(),
                "description": meta.description,
                "severity": format!("{}", meta.default_severity),
                "has_fix": meta.fix_availability != solgrid_diagnostics::FixAvailability::None,
            })
        })
        .collect();

    serde_json::to_string(&rules).unwrap_or_else(|_| "[]".to_string())
}

fn parse_config(json: &str) -> Config {
    if json.is_empty() {
        return Config::default();
    }
    serde_json::from_str(json).unwrap_or_default()
}

fn parse_format_config(json: &str) -> FormatConfig {
    if json.is_empty() {
        return FormatConfig::default();
    }
    serde_json::from_str(json).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_SOL: &str = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    uint256 public value;

    function setValue(uint256 _value) external {
        value = _value;
    }
}
"#;

    #[test]
    fn test_lint_default_config() {
        let result = lint(SAMPLE_SOL, "");
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(parsed["diagnostics"].is_array());
        assert!(parsed["diagnostic_count"].is_number());
    }

    #[test]
    fn test_lint_with_config() {
        let config = r#"{"lint":{"preset":"recommended","rules":{}},"format":{},"global":{}}"#;
        let result = lint(SAMPLE_SOL, config);
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(parsed["diagnostics"].is_array());
    }

    #[test]
    fn test_fix_returns_source_and_diagnostics() {
        let result = fix(SAMPLE_SOL, "", false);
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(parsed["fixed_source"].is_string());
        assert!(parsed["diagnostics"].is_array());
    }

    #[test]
    fn test_format_valid_solidity() {
        let result = format(SAMPLE_SOL, "");
        // Should not be a JSON error object
        assert!(!result.starts_with(r#"{"error":"#));
        // Should contain the contract
        assert!(result.contains("contract Test"));
    }

    #[test]
    fn test_format_with_options() {
        let config = r#"{"tab_width":2,"use_tabs":false}"#;
        let result = format(SAMPLE_SOL, config);
        assert!(!result.starts_with(r#"{"error":"#));
    }

    #[test]
    fn test_version() {
        let v = version();
        assert!(v.starts_with("0."));
    }

    #[test]
    fn test_list_rules() {
        let result = list_rules();
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();
        assert!(!parsed.is_empty());
        // Check that each rule has required fields
        for rule in &parsed {
            assert!(rule["id"].is_string());
            assert!(rule["category"].is_string());
            assert!(rule["description"].is_string());
        }
    }

    #[test]
    fn test_lint_empty_source() {
        let result = lint("", "");
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(parsed["diagnostics"].is_array());
    }

    #[test]
    fn test_format_invalid_source() {
        let result = format("this is not {{{ valid solidity", "");
        // Should return an error (either as JSON error or as-is)
        // The formatter may return the source unchanged or error
        assert!(!result.is_empty());
    }
}
