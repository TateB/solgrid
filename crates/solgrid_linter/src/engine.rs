//! Lint engine — orchestrates parsing and rule execution.

use crate::context::LintContext;
use crate::registry::RuleRegistry;
use crate::suppression::parse_suppressions;
use solgrid_config::Config;
use solgrid_diagnostics::{apply_fixes, Diagnostic, FileResult, Fix, FixSafety, TextEdit};
use std::path::Path;

/// The main lint engine.
pub struct LintEngine {
    registry: RuleRegistry,
}

impl LintEngine {
    /// Create a new lint engine with all built-in rules.
    pub fn new() -> Self {
        Self {
            registry: RuleRegistry::new(),
        }
    }

    /// Create a lint engine with a custom rule registry.
    pub fn with_registry(registry: RuleRegistry) -> Self {
        Self { registry }
    }

    /// Get a reference to the underlying rule registry.
    pub fn registry(&self) -> &RuleRegistry {
        &self.registry
    }

    /// Lint a single file and return diagnostics.
    pub fn lint_file(&self, path: &Path, config: &Config) -> FileResult {
        let source = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                return FileResult {
                    path: path.display().to_string(),
                    diagnostics: vec![Diagnostic::new(
                        "internal",
                        format!("failed to read file: {e}"),
                        solgrid_diagnostics::Severity::Error,
                        0..0,
                    )],
                };
            }
        };

        self.lint_source(&source, path, config)
    }

    /// Lint source code directly.
    pub fn lint_source(&self, source: &str, path: &Path, config: &Config) -> FileResult {
        let ctx = LintContext::new(source, path, config);
        let enabled_rules = self.registry.enabled_rules(config);
        let suppressions = parse_suppressions(source);

        let mut diagnostics: Vec<Diagnostic> = Vec::new();

        for rule in &enabled_rules {
            let rule_diagnostics = rule.check(&ctx);
            for diag in rule_diagnostics {
                // Check suppressions
                let line = ctx.line_number(diag.span.start);
                if !suppressions.is_suppressed(&diag.rule_id, line) {
                    // Apply severity override from config
                    let severity = config
                        .lint
                        .rule_severity(&diag.rule_id, rule.meta().category)
                        .unwrap_or(diag.severity);
                    diagnostics.push(Diagnostic { severity, ..diag });
                }
            }
        }

        // Sort diagnostics by position
        diagnostics.sort_by_key(|d| d.span.start);

        FileResult {
            path: path.display().to_string(),
            diagnostics,
        }
    }

    /// Lint and apply fixes to source code.
    /// Returns the fixed source and any remaining diagnostics.
    pub fn fix_source(
        &self,
        source: &str,
        path: &Path,
        config: &Config,
        include_unsafe: bool,
    ) -> (String, FileResult) {
        let mut current_source = source.to_string();

        for _ in 0..8 {
            let result = self.lint_source(&current_source, path, config);
            let applicable_fixes = collect_applicable_fixes(&result.diagnostics, include_unsafe);
            let selected_fixes = select_non_overlapping_fixes(&applicable_fixes);

            if selected_fixes.is_empty() {
                break;
            }

            let next_source = apply_fixes(&current_source, &selected_fixes);
            if next_source == current_source {
                break;
            }

            current_source = next_source;
        }

        let remaining = self.lint_source(&current_source, path, config);
        (current_source, remaining)
    }
}

fn collect_applicable_fixes(diagnostics: &[Diagnostic], include_unsafe: bool) -> Vec<Fix> {
    let mut fixes = Vec::new();

    for diag in diagnostics {
        let Some(fix) = diag.fix.as_ref() else {
            continue;
        };

        let allowed = match fix.safety {
            FixSafety::Safe => true,
            FixSafety::Suggestion => include_unsafe,
            FixSafety::Dangerous => false,
        };
        if !allowed {
            continue;
        }

        if fixes.iter().any(|existing| same_fix(existing, fix)) {
            continue;
        }

        fixes.push(fix.clone());
    }

    fixes
}

fn select_non_overlapping_fixes(fixes: &[Fix]) -> Vec<&Fix> {
    let mut ordered: Vec<&Fix> = fixes.iter().collect();
    ordered.sort_by(|left, right| {
        let left_start = left
            .edits
            .iter()
            .map(|edit| edit.range.start)
            .min()
            .unwrap_or(0);
        let right_start = right
            .edits
            .iter()
            .map(|edit| edit.range.start)
            .min()
            .unwrap_or(0);

        left_start
            .cmp(&right_start)
            .then_with(|| total_fix_span(left).cmp(&total_fix_span(right)))
            .then_with(|| left.edits.len().cmp(&right.edits.len()))
    });

    let mut selected = Vec::new();
    let mut selected_edits: Vec<&TextEdit> = Vec::new();

    for fix in ordered {
        if fix.edits.iter().any(|edit| {
            selected_edits
                .iter()
                .any(|existing| edits_overlap(edit, existing))
        }) {
            continue;
        }

        selected_edits.extend(fix.edits.iter());
        selected.push(fix);
    }

    selected
}

fn total_fix_span(fix: &Fix) -> usize {
    fix.edits
        .iter()
        .map(|edit| edit.range.end.saturating_sub(edit.range.start))
        .sum()
}

fn edits_overlap(left: &TextEdit, right: &TextEdit) -> bool {
    left.range.start < right.range.end && right.range.start < left.range.end
}

fn same_fix(a: &Fix, b: &Fix) -> bool {
    a.safety == b.safety
        && a.edits.len() == b.edits.len()
        && a.edits
            .iter()
            .zip(&b.edits)
            .all(|(left, right)| same_edit(left, right))
}

fn same_edit(a: &TextEdit, b: &TextEdit) -> bool {
    a.range == b.range && a.replacement == b.replacement
}

impl Default for LintEngine {
    fn default() -> Self {
        Self::new()
    }
}
