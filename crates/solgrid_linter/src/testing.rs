//! Test utilities for solgrid lint rules.
//!
//! Provides helpers for writing concise rule tests.

use crate::LintEngine;
use solgrid_config::{Config, RulePreset};
use solgrid_diagnostics::Diagnostic;
use std::path::{Path, PathBuf};

/// Lint a source string using the default engine and return diagnostics.
pub fn lint_source(source: &str) -> Vec<Diagnostic> {
    let mut config = Config::default();
    config.lint.preset = RulePreset::All;
    lint_source_with_config(source, &config)
}

/// Lint a source string with a specific config and return diagnostics.
pub fn lint_source_with_config(source: &str, config: &Config) -> Vec<Diagnostic> {
    let engine = LintEngine::new();
    let path = Path::new("test.sol");
    let result = engine.lint_source(source, path, config);
    result.diagnostics
}

/// Lint a source string and return only diagnostics for a specific rule.
pub fn lint_source_for_rule(source: &str, rule_id: &str) -> Vec<Diagnostic> {
    lint_source(source)
        .into_iter()
        .filter(|d| d.rule_id == rule_id)
        .collect()
}

/// Lint a source string using the provided config and return only diagnostics
/// for a specific rule.
pub fn lint_source_for_rule_with_config(
    source: &str,
    rule_id: &str,
    config: &Config,
) -> Vec<Diagnostic> {
    lint_source_with_config(source, config)
        .into_iter()
        .filter(|d| d.rule_id == rule_id)
        .collect()
}

/// Lint a source string and apply fixes, returning the fixed source.
pub fn fix_source(source: &str) -> String {
    let mut config = Config::default();
    config.lint.preset = RulePreset::All;
    fix_source_with_config(source, &config)
}

/// Lint a source string and apply fixes using the provided config, returning
/// the fixed source.
pub fn fix_source_with_config(source: &str, config: &Config) -> String {
    let engine = LintEngine::new();
    let path = Path::new("test.sol");
    let (fixed, _) = engine.fix_source(source, path, config, false);
    fixed
}

/// Lint a source string and apply all fixes (including unsafe), returning the fixed source.
pub fn fix_source_unsafe(source: &str) -> String {
    let mut config = Config::default();
    config.lint.preset = RulePreset::All;
    fix_source_unsafe_with_config(source, &config)
}

/// Lint a source string and apply all fixes (including unsafe) using the
/// provided config, returning the fixed source.
pub fn fix_source_unsafe_with_config(source: &str, config: &Config) -> String {
    let engine = LintEngine::new();
    let path = Path::new("test.sol");
    let (fixed, _) = engine.fix_source(source, path, config, true);
    fixed
}

/// Assert that linting produces the expected number of diagnostics for a rule.
pub fn assert_diagnostic_count(source: &str, rule_id: &str, expected: usize) {
    let diagnostics = lint_source_for_rule(source, rule_id);
    assert_eq!(
        diagnostics.len(),
        expected,
        "Expected {} diagnostics for rule '{}', got {}.\nDiagnostics: {:#?}",
        expected,
        rule_id,
        diagnostics.len(),
        diagnostics,
    );
}

/// Assert that linting produces no diagnostics for a rule.
pub fn assert_no_diagnostics(source: &str, rule_id: &str) {
    assert_diagnostic_count(source, rule_id, 0);
}

/// Lint a source string with remappings and a specific file path, returning diagnostics.
pub fn lint_source_with_remappings(
    source: &str,
    path: &Path,
    remappings: &[(String, PathBuf)],
) -> Vec<Diagnostic> {
    let engine = LintEngine::with_remappings(remappings.to_vec());
    let mut config = Config::default();
    config.lint.preset = RulePreset::All;
    let result = engine.lint_source(source, path, &config);
    result.diagnostics
}

/// Lint a source string with remappings and return only diagnostics for a specific rule.
pub fn lint_source_with_remappings_for_rule(
    source: &str,
    path: &Path,
    remappings: &[(String, PathBuf)],
    rule_id: &str,
) -> Vec<Diagnostic> {
    lint_source_with_remappings(source, path, remappings)
        .into_iter()
        .filter(|d| d.rule_id == rule_id)
        .collect()
}

/// Apply fixes with remappings and a specific file path, returning the fixed source.
pub fn fix_source_with_remappings(
    source: &str,
    path: &Path,
    remappings: &[(String, PathBuf)],
    include_unsafe: bool,
) -> String {
    let engine = LintEngine::with_remappings(remappings.to_vec());
    let mut config = Config::default();
    config.lint.preset = RulePreset::All;
    let (fixed, _) = engine.fix_source(source, path, &config, include_unsafe);
    fixed
}

/// Format diagnostics into a simple string for snapshot testing.
pub fn format_diagnostics(diagnostics: &[Diagnostic]) -> String {
    let mut lines = Vec::new();
    for d in diagnostics {
        lines.push(format!(
            "[{}] {} ({}) at {}..{}",
            d.severity, d.message, d.rule_id, d.span.start, d.span.end
        ));
    }
    lines.join("\n")
}
