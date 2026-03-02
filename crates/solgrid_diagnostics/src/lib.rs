use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::Range;

/// Severity level for a diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
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
    pub fn new(safety: FixSafety, message: impl Into<String>, edits: Vec<TextEdit>) -> Self {
        Self {
            safety,
            edits,
            message: message.into(),
        }
    }

    pub fn safe(message: impl Into<String>, edits: Vec<TextEdit>) -> Self {
        Self::new(FixSafety::Safe, message, edits)
    }

    pub fn suggestion(message: impl Into<String>, edits: Vec<TextEdit>) -> Self {
        Self::new(FixSafety::Suggestion, message, edits)
    }

    pub fn dangerous(message: impl Into<String>, edits: Vec<TextEdit>) -> Self {
        Self::new(FixSafety::Dangerous, message, edits)
    }
}

/// Category of a lint rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuleCategory {
    Security,
    BestPractices,
    Naming,
    Gas,
    Style,
    Docs,
}

impl RuleCategory {
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
