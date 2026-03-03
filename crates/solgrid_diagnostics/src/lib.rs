//! Diagnostic types and reporting for solgrid.
//!
//! Provides core types used across the solgrid workspace: [`Diagnostic`],
//! [`Severity`], [`Fix`], [`TextEdit`], [`RuleMeta`], and [`FileResult`].

use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::Range;

/// Severity level for a diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// A hard error that must be fixed.
    Error,
    /// A warning that should be addressed.
    Warning,
    /// An informational suggestion.
    Info,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Severity::Error => write!(f, "error"),
            Severity::Warning => write!(f, "warning"),
            Severity::Info => write!(f, "info"),
        }
    }
}

/// Safety tier for auto-fixes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FixSafety {
    /// Guaranteed to preserve semantics. Applied with `--fix`.
    Safe,
    /// Likely correct but may change semantics. Applied with `--fix --unsafe-fixes`.
    Suggestion,
    /// Requires manual confirmation. Shown as editor code actions only.
    Dangerous,
}

impl fmt::Display for FixSafety {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FixSafety::Safe => write!(f, "safe"),
            FixSafety::Suggestion => write!(f, "suggestion"),
            FixSafety::Dangerous => write!(f, "dangerous"),
        }
    }
}

/// A single text edit within a source file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextEdit {
    /// Byte range in the original source to replace.
    pub range: Range<usize>,
    /// Replacement text (empty string = deletion).
    pub replacement: String,
}

impl TextEdit {
    /// Create a new text edit replacing the given byte range.
    pub fn new(range: Range<usize>, replacement: impl Into<String>) -> Self {
        Self {
            range,
            replacement: replacement.into(),
        }
    }

    /// Create a replacement edit.
    pub fn replace(range: Range<usize>, replacement: impl Into<String>) -> Self {
        Self::new(range, replacement)
    }

    /// Create a deletion edit.
    pub fn delete(range: Range<usize>) -> Self {
        Self::new(range, "")
    }

    /// Create an insertion edit at a position.
    pub fn insert(position: usize, text: impl Into<String>) -> Self {
        Self::new(position..position, text)
    }
}

/// An auto-fix for a diagnostic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fix {
    /// Safety tier.
    pub safety: FixSafety,
    /// The text edits to apply.
    pub edits: Vec<TextEdit>,
    /// Human-readable description of what the fix does.
    pub message: String,
}

impl Fix {
    /// Create a new fix with the given safety tier, message, and edits.
    pub fn new(safety: FixSafety, message: impl Into<String>, edits: Vec<TextEdit>) -> Self {
        Self {
            safety,
            edits,
            message: message.into(),
        }
    }

    /// Create a safe fix (applied with `--fix`).
    pub fn safe(message: impl Into<String>, edits: Vec<TextEdit>) -> Self {
        Self::new(FixSafety::Safe, message, edits)
    }

    /// Create a suggestion fix (applied with `--fix --unsafe-fixes`).
    pub fn suggestion(message: impl Into<String>, edits: Vec<TextEdit>) -> Self {
        Self::new(FixSafety::Suggestion, message, edits)
    }

    /// Create a dangerous fix (shown as editor code actions only).
    pub fn dangerous(message: impl Into<String>, edits: Vec<TextEdit>) -> Self {
        Self::new(FixSafety::Dangerous, message, edits)
    }
}

/// Category of a lint rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuleCategory {
    /// Security vulnerabilities and unsafe patterns.
    Security,
    /// Community-accepted best practices.
    BestPractices,
    /// Naming convention enforcement.
    Naming,
    /// Gas optimization opportunities.
    Gas,
    /// Code style and layout.
    Style,
    /// NatSpec and documentation completeness.
    Docs,
}

impl RuleCategory {
    /// Return the category as a kebab-case string.
    pub fn as_str(&self) -> &'static str {
        match self {
            RuleCategory::Security => "security",
            RuleCategory::BestPractices => "best-practices",
            RuleCategory::Naming => "naming",
            RuleCategory::Gas => "gas",
            RuleCategory::Style => "style",
            RuleCategory::Docs => "docs",
        }
    }

    /// Return the default severity for this category.
    pub fn default_severity(&self) -> Severity {
        match self {
            RuleCategory::Security => Severity::Error,
            RuleCategory::BestPractices => Severity::Warning,
            RuleCategory::Naming => Severity::Warning,
            RuleCategory::Gas => Severity::Info,
            RuleCategory::Style => Severity::Info,
            RuleCategory::Docs => Severity::Info,
        }
    }
}

impl fmt::Display for RuleCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Whether a rule provides auto-fix capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FixAvailability {
    /// No auto-fix available.
    None,
    /// Auto-fix available at the given safety level.
    Available(FixSafety),
}

/// Metadata for a lint rule.
#[derive(Debug, Clone)]
pub struct RuleMeta {
    /// Full rule ID, e.g. "security/tx-origin".
    pub id: &'static str,
    /// Short name, e.g. "tx-origin".
    pub name: &'static str,
    /// Category.
    pub category: RuleCategory,
    /// Default severity.
    pub default_severity: Severity,
    /// One-line description.
    pub description: &'static str,
    /// Whether a fix is available.
    pub fix_availability: FixAvailability,
}

impl RuleMeta {
    /// Return the full rule ID in `category/name` format.
    pub fn full_id(&self) -> String {
        format!("{}/{}", self.category, self.name)
    }
}

/// A diagnostic produced by a lint rule.
#[derive(Debug, Clone, Serialize)]
pub struct Diagnostic {
    /// Full rule ID, e.g. "security/tx-origin".
    pub rule_id: String,
    /// Human-readable message.
    pub message: String,
    /// Severity level.
    pub severity: Severity,
    /// Byte range in the source file.
    pub span: Range<usize>,
    /// Optional auto-fix.
    pub fix: Option<Fix>,
}

impl Diagnostic {
    /// Create a new diagnostic without an auto-fix.
    pub fn new(
        rule_id: impl Into<String>,
        message: impl Into<String>,
        severity: Severity,
        span: Range<usize>,
    ) -> Self {
        Self {
            rule_id: rule_id.into(),
            message: message.into(),
            severity,
            span,
            fix: None,
        }
    }

    /// Attach an auto-fix to this diagnostic.
    pub fn with_fix(mut self, fix: Fix) -> Self {
        self.fix = Some(fix);
        self
    }
}

/// Result of linting a single file.
#[derive(Debug, Clone, Serialize)]
pub struct FileResult {
    /// Path to the file.
    pub path: String,
    /// Diagnostics produced.
    pub diagnostics: Vec<Diagnostic>,
}

/// Apply a set of non-overlapping fixes to source text.
/// Returns the fixed source text. Fixes must be sorted by range start
/// and must not overlap.
pub fn apply_fixes(source: &str, fixes: &[&Fix]) -> String {
    let mut edits: Vec<&TextEdit> = fixes.iter().flat_map(|f| f.edits.iter()).collect();
    edits.sort_by_key(|e| e.range.start);

    // Check for overlaps
    for window in edits.windows(2) {
        if window[0].range.end > window[1].range.start {
            // Overlapping edits — skip all
            return source.to_string();
        }
    }

    let mut result = String::with_capacity(source.len());
    let mut last_end = 0;

    for edit in &edits {
        result.push_str(&source[last_end..edit.range.start]);
        result.push_str(&edit.replacement);
        last_end = edit.range.end;
    }
    result.push_str(&source[last_end..]);

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apply_single_fix() {
        let source = "hello world";
        let fix = Fix::safe("replace hello", vec![TextEdit::replace(0..5, "goodbye")]);
        let result = apply_fixes(source, &[&fix]);
        assert_eq!(result, "goodbye world");
    }

    #[test]
    fn test_apply_multiple_non_overlapping_fixes() {
        let source = "aaa bbb ccc";
        let fix1 = Fix::safe("fix1", vec![TextEdit::replace(0..3, "xxx")]);
        let fix2 = Fix::safe("fix2", vec![TextEdit::replace(8..11, "zzz")]);
        let result = apply_fixes(source, &[&fix1, &fix2]);
        assert_eq!(result, "xxx bbb zzz");
    }

    #[test]
    fn test_apply_overlapping_fixes_returns_original() {
        let source = "hello world";
        let fix1 = Fix::safe("fix1", vec![TextEdit::replace(0..7, "aaa")]);
        let fix2 = Fix::safe("fix2", vec![TextEdit::replace(5..11, "bbb")]);
        let result = apply_fixes(source, &[&fix1, &fix2]);
        assert_eq!(result, "hello world"); // unchanged due to overlap
    }

    #[test]
    fn test_text_edit_delete() {
        let source = "hello world";
        let fix = Fix::safe("delete", vec![TextEdit::delete(5..6)]);
        let result = apply_fixes(source, &[&fix]);
        assert_eq!(result, "helloworld");
    }

    #[test]
    fn test_text_edit_insert() {
        let source = "helloworld";
        let fix = Fix::safe("insert", vec![TextEdit::insert(5, " ")]);
        let result = apply_fixes(source, &[&fix]);
        assert_eq!(result, "hello world");
    }

    #[test]
    fn test_diagnostic_with_fix() {
        let diag = Diagnostic::new(
            "security/tx-origin",
            "use of tx.origin",
            Severity::Error,
            10..20,
        )
        .with_fix(Fix::dangerous(
            "replace with msg.sender",
            vec![TextEdit::replace(10..20, "msg.sender")],
        ));

        assert_eq!(diag.rule_id, "security/tx-origin");
        assert_eq!(diag.severity, Severity::Error);
        assert!(diag.fix.is_some());
        assert_eq!(diag.fix.unwrap().safety, FixSafety::Dangerous);
    }

    #[test]
    fn test_severity_display() {
        assert_eq!(format!("{}", Severity::Error), "error");
        assert_eq!(format!("{}", Severity::Warning), "warning");
        assert_eq!(format!("{}", Severity::Info), "info");
    }

    #[test]
    fn test_rule_category_display() {
        assert_eq!(RuleCategory::Security.as_str(), "security");
        assert_eq!(RuleCategory::BestPractices.as_str(), "best-practices");
        assert_eq!(RuleCategory::Gas.as_str(), "gas");
    }

    #[test]
    fn test_rule_category_default_severity() {
        assert_eq!(RuleCategory::Security.default_severity(), Severity::Error);
        assert_eq!(
            RuleCategory::BestPractices.default_severity(),
            Severity::Warning
        );
        assert_eq!(RuleCategory::Gas.default_severity(), Severity::Info);
    }

    #[test]
    fn test_fix_safety_display() {
        assert_eq!(format!("{}", FixSafety::Safe), "safe");
        assert_eq!(format!("{}", FixSafety::Suggestion), "suggestion");
        assert_eq!(format!("{}", FixSafety::Dangerous), "dangerous");
    }
}
