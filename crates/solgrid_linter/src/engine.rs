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
        let result = self.lint_source(source, path, config);

        // Collect applicable fixes
        let mut applicable_fixes: Vec<&Fix> = Vec::new();
        for diag in &result.diagnostics {
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

            if applicable_fixes
                .iter()
                .any(|existing| same_fix(existing, fix))
            {
                continue;
            }

            applicable_fixes.push(fix);
        }

        let fixed_source = apply_fixes(source, &applicable_fixes);

        // Re-lint the fixed source to get remaining diagnostics
        let remaining = self.lint_source(&fixed_source, path, config);

        (fixed_source, remaining)
    }
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
