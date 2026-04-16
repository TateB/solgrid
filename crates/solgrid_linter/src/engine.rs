//! Lint engine — orchestrates parsing and rule execution.

use crate::context::LintContext;
use crate::registry::RuleRegistry;
use crate::suppression::parse_suppressions;
use solgrid_config::Config;
use solgrid_diagnostics::{apply_fixes, Diagnostic, FileResult, Fix, FixSafety, TextEdit};
use std::path::{Path, PathBuf};

/// The main lint engine.
pub struct LintEngine {
    registry: RuleRegistry,
    remappings: Vec<(String, PathBuf)>,
}

impl LintEngine {
    /// Create a new lint engine with all built-in rules.
    pub fn new() -> Self {
        Self {
            registry: RuleRegistry::new(),
            remappings: Vec::new(),
        }
    }

    /// Create a lint engine with remappings for import path rules.
    pub fn with_remappings(remappings: Vec<(String, PathBuf)>) -> Self {
        Self {
            registry: RuleRegistry::new(),
            remappings,
        }
    }

    /// Create a lint engine by auto-detecting the workspace root and loading remappings.
    pub fn from_workspace() -> Self {
        let workspace_root =
            solgrid_config::find_workspace_root(&std::env::current_dir().unwrap_or_default());
        let remappings = workspace_root
            .map(|root| solgrid_config::load_remappings(&root))
            .unwrap_or_default();
        Self::with_remappings(remappings)
    }

    /// Create a lint engine with a custom rule registry.
    pub fn with_registry(registry: RuleRegistry) -> Self {
        Self {
            registry,
            remappings: Vec::new(),
        }
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

    /// Lint source code directly with an explicit remapping set.
    pub fn lint_source_with_remappings(
        &self,
        source: &str,
        path: &Path,
        config: &Config,
        remappings: &[(String, PathBuf)],
    ) -> FileResult {
        let ctx = LintContext::new(source, path, config, remappings);
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
                        .rule_severity(&diag.rule_id, rule.meta().default_severity)
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

    /// Lint source code directly.
    pub fn lint_source(&self, source: &str, path: &Path, config: &Config) -> FileResult {
        self.lint_source_with_remappings(source, path, config, &self.remappings)
    }

    /// Lint and apply fixes with an explicit remapping set.
    pub fn fix_source_with_remappings(
        &self,
        source: &str,
        path: &Path,
        config: &Config,
        include_unsafe: bool,
        remappings: &[(String, PathBuf)],
    ) -> (String, FileResult) {
        let mut current_source = source.to_string();

        for _ in 0..8 {
            let result =
                self.lint_source_with_remappings(&current_source, path, config, remappings);
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

        if let Ok(formatted) = solgrid_formatter::format_source(&current_source, &config.format) {
            current_source = formatted;
        }

        let remaining = self.lint_source_with_remappings(&current_source, path, config, remappings);
        (current_source, remaining)
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
        self.fix_source_with_remappings(source, path, config, include_unsafe, &self.remappings)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::RuleRegistry;
    use crate::rule::Rule;
    use solgrid_diagnostics::{
        Diagnostic, Fix, FixAvailability, FixSafety, RuleCategory, RuleMeta, Severity, TextEdit,
    };
    use std::path::Path;

    static META: RuleMeta = RuleMeta {
        id: "style/test-fix-formatting",
        name: "test-fix-formatting",
        category: RuleCategory::Style,
        default_severity: Severity::Info,
        description: "test-only rule",
        fix_availability: FixAvailability::Available(FixSafety::Safe),
    };

    struct NeedsFormattingFixRule;

    impl Rule for NeedsFormattingFixRule {
        fn meta(&self) -> &RuleMeta {
            &META
        }

        fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
            let Some(start) = ctx.source.find("return 1;") else {
                return Vec::new();
            };
            let end = start + "return 1;".len();
            vec![Diagnostic::new(
                META.id,
                "replace return value",
                META.default_severity,
                start..end,
            )
            .with_fix(Fix::safe(
                "replace return value",
                vec![TextEdit::replace(start..end, "return 1    + 2;")],
            ))]
        }
    }

    #[test]
    fn fix_source_formats_final_output() {
        let mut registry = RuleRegistry::empty();
        registry.register(Box::new(NeedsFormattingFixRule));

        let engine = LintEngine::with_registry(registry);
        let mut config = Config::default();
        config.lint.preset = solgrid_config::RulePreset::All;

        let source = r#"contract T {
    function f() public pure returns (uint256) {
        return 1;
    }
}
"#;

        let (fixed, remaining) = engine.fix_source(source, Path::new("test.sol"), &config, false);

        assert_eq!(
            fixed,
            r#"contract T {
    function f() public pure returns (uint256) {
        return 1 + 2;
    }
}
"#
        );
        assert!(remaining.diagnostics.is_empty());
    }
}
