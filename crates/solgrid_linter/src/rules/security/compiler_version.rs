//! Rule: security/compiler-version
//!
//! Ensure a Solidity pragma is present and that the compiler version is not
//! outdated.  Flags files missing `pragma solidity` and files that specify
//! Solidity 0.4.x, 0.5.x, 0.6.x, or 0.7.x, which are known to contain
//! compiler bugs and lack important security features.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_config::{SolidityVersion, VersionOperator, VersionRequirement};
use solgrid_diagnostics::*;

static META: RuleMeta = RuleMeta {
    id: "security/compiler-version",
    name: "compiler-version",
    category: RuleCategory::Security,
    default_severity: Severity::Warning,
    description: "ensure a recent Solidity compiler version is used",
    fix_availability: FixAvailability::None,
};

pub struct CompilerVersionRule;

impl Rule for CompilerVersionRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let pattern = "pragma solidity";
        let pattern_len = pattern.len();
        let allowed = ctx.config.lint.compiler_version_allowed();

        match ctx.source.find(pattern) {
            None => {
                // No pragma found at all — flag the beginning of the file
                diagnostics.push(Diagnostic::new(
                    META.id,
                    "no `pragma solidity` version directive found",
                    META.default_severity,
                    0..0,
                ));
            }
            Some(pos) => {
                // Grab the rest of the line after `pragma solidity`
                let after = &ctx.source[pos + pattern_len..];
                let line_end = after.find(';').unwrap_or(after.len());
                let version_text = after[..line_end].trim();

                let span_end = pos + pattern_len + line_end;
                match allowed {
                    Ok(Some(ref requirements)) => match (
                        VersionInterval::from_requirements(requirements),
                        VersionInterval::parse_pragma(version_text),
                    ) {
                        (Ok(allowed_range), Ok(pragma_range)) => {
                            if !allowed_range.contains(&pragma_range) {
                                diagnostics.push(Diagnostic::new(
                                    META.id,
                                    format!(
                                        "compiler version `{version_text}` does not satisfy the configured allowed range"
                                    ),
                                    META.default_severity,
                                    pos..span_end,
                                ));
                            }
                        }
                        (Ok(_), Err(error)) => {
                            diagnostics.push(Diagnostic::new(
                                META.id,
                                format!(
                                    "compiler version `{version_text}` could not be verified against the configured allowed range: {error}"
                                ),
                                META.default_severity,
                                pos..span_end,
                            ));
                        }
                        (Err(_), _) => apply_outdated_version_fallback(
                            &mut diagnostics,
                            version_text,
                            pos,
                            span_end,
                        ),
                    },
                    Ok(None) | Err(_) => {
                        apply_outdated_version_fallback(
                            &mut diagnostics,
                            version_text,
                            pos,
                            span_end,
                        );
                    }
                }
            }
        }
        diagnostics
    }
}

fn apply_outdated_version_fallback(
    diagnostics: &mut Vec<Diagnostic>,
    version_text: &str,
    span_start: usize,
    span_end: usize,
) {
    let outdated_prefixes = ["0.4", "0.5", "0.6", "0.7"];
    for prefix in &outdated_prefixes {
        if version_text.contains(prefix) {
            diagnostics.push(Diagnostic::new(
                META.id,
                format!(
                    "compiler version is outdated; Solidity {prefix}.x has known bugs — use 0.8.x or later"
                ),
                META.default_severity,
                span_start..span_end,
            ));
            break;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VersionBound {
    version: SolidityVersion,
    inclusive: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct VersionInterval {
    lower: Option<VersionBound>,
    upper: Option<VersionBound>,
}

impl VersionInterval {
    fn parse_pragma(input: &str) -> Result<Self, String> {
        if input.contains("||") {
            return Err("`||` disjunction is not supported".to_string());
        }

        let mut interval = Self::default();
        let mut saw_clause = false;

        for token in input.split_whitespace() {
            saw_clause = true;
            interval.constrain(Self::parse_clause(token)?)?;
        }

        if !saw_clause {
            return Err("no version clauses found".to_string());
        }

        if interval.is_empty() {
            return Err("pragma range is empty".to_string());
        }

        Ok(interval)
    }

    fn from_requirements(requirements: &[VersionRequirement]) -> Result<Self, String> {
        let mut interval = Self::default();
        for requirement in requirements {
            interval.constrain(Self::from_requirement(*requirement))?;
        }

        if interval.is_empty() {
            return Err("configured allowed range is empty".to_string());
        }

        Ok(interval)
    }

    fn contains(&self, other: &Self) -> bool {
        self.lower_contains(other.lower) && self.upper_contains(other.upper)
    }

    fn lower_contains(&self, other: Option<VersionBound>) -> bool {
        match (self.lower, other) {
            (None, _) => true,
            (Some(_), None) => false,
            (Some(allowed), Some(candidate)) => is_lower_bound_at_least(candidate, allowed),
        }
    }

    fn upper_contains(&self, other: Option<VersionBound>) -> bool {
        match (self.upper, other) {
            (None, _) => true,
            (Some(_), None) => false,
            (Some(allowed), Some(candidate)) => is_upper_bound_at_most(candidate, allowed),
        }
    }

    fn constrain(&mut self, other: Self) -> Result<(), String> {
        self.lower = match (self.lower, other.lower) {
            (Some(left), Some(right)) => Some(stricter_lower(left, right)),
            (Some(bound), None) | (None, Some(bound)) => Some(bound),
            (None, None) => None,
        };
        self.upper = match (self.upper, other.upper) {
            (Some(left), Some(right)) => Some(stricter_upper(left, right)),
            (Some(bound), None) | (None, Some(bound)) => Some(bound),
            (None, None) => None,
        };

        if self.is_empty() {
            return Err("range is empty".to_string());
        }

        Ok(())
    }

    fn is_empty(&self) -> bool {
        let (Some(lower), Some(upper)) = (self.lower, self.upper) else {
            return false;
        };

        match lower.version.cmp_key().cmp(&upper.version.cmp_key()) {
            std::cmp::Ordering::Greater => true,
            std::cmp::Ordering::Less => false,
            std::cmp::Ordering::Equal => !lower.inclusive || !upper.inclusive,
        }
    }

    fn parse_clause(token: &str) -> Result<Self, String> {
        if let Some(version) = SolidityVersion::parse(token) {
            return Ok(Self::exact(version));
        }
        if let Some(version) = token.strip_prefix('^').and_then(SolidityVersion::parse) {
            return Ok(Self::caret(version));
        }
        if let Some(version) = token.strip_prefix('~').and_then(SolidityVersion::parse) {
            return Ok(Self::tilde(version));
        }

        let requirement = VersionRequirement::parse(token)
            .map_err(|_| format!("unsupported version clause `{token}`"))?;
        Ok(Self::from_requirement(requirement))
    }

    fn from_requirement(requirement: VersionRequirement) -> Self {
        match requirement.operator {
            VersionOperator::GreaterThan => Self {
                lower: Some(VersionBound {
                    version: requirement.version,
                    inclusive: false,
                }),
                upper: None,
            },
            VersionOperator::GreaterThanOrEqual => Self {
                lower: Some(VersionBound {
                    version: requirement.version,
                    inclusive: true,
                }),
                upper: None,
            },
            VersionOperator::LessThan => Self {
                lower: None,
                upper: Some(VersionBound {
                    version: requirement.version,
                    inclusive: false,
                }),
            },
            VersionOperator::LessThanOrEqual => Self {
                lower: None,
                upper: Some(VersionBound {
                    version: requirement.version,
                    inclusive: true,
                }),
            },
            VersionOperator::Equal => Self::exact(requirement.version),
        }
    }

    fn exact(version: SolidityVersion) -> Self {
        let bound = VersionBound {
            version,
            inclusive: true,
        };
        Self {
            lower: Some(bound),
            upper: Some(bound),
        }
    }

    fn caret(version: SolidityVersion) -> Self {
        let upper = if version.major > 0 {
            SolidityVersion {
                major: version.major + 1,
                minor: 0,
                patch: 0,
            }
        } else if version.minor > 0 {
            SolidityVersion {
                major: 0,
                minor: version.minor + 1,
                patch: 0,
            }
        } else {
            SolidityVersion {
                major: 0,
                minor: 0,
                patch: version.patch + 1,
            }
        };

        Self {
            lower: Some(VersionBound {
                version,
                inclusive: true,
            }),
            upper: Some(VersionBound {
                version: upper,
                inclusive: false,
            }),
        }
    }

    fn tilde(version: SolidityVersion) -> Self {
        Self {
            lower: Some(VersionBound {
                version,
                inclusive: true,
            }),
            upper: Some(VersionBound {
                version: SolidityVersion {
                    major: version.major,
                    minor: version.minor + 1,
                    patch: 0,
                },
                inclusive: false,
            }),
        }
    }
}

fn stricter_lower(left: VersionBound, right: VersionBound) -> VersionBound {
    match left.version.cmp_key().cmp(&right.version.cmp_key()) {
        std::cmp::Ordering::Greater => left,
        std::cmp::Ordering::Less => right,
        std::cmp::Ordering::Equal => VersionBound {
            version: left.version,
            inclusive: left.inclusive && right.inclusive,
        },
    }
}

fn stricter_upper(left: VersionBound, right: VersionBound) -> VersionBound {
    match left.version.cmp_key().cmp(&right.version.cmp_key()) {
        std::cmp::Ordering::Greater => right,
        std::cmp::Ordering::Less => left,
        std::cmp::Ordering::Equal => VersionBound {
            version: left.version,
            inclusive: left.inclusive && right.inclusive,
        },
    }
}

fn is_lower_bound_at_least(candidate: VersionBound, minimum: VersionBound) -> bool {
    match candidate.version.cmp_key().cmp(&minimum.version.cmp_key()) {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Less => false,
        std::cmp::Ordering::Equal => !minimum.inclusive || candidate.inclusive == minimum.inclusive,
    }
}

fn is_upper_bound_at_most(candidate: VersionBound, maximum: VersionBound) -> bool {
    match candidate.version.cmp_key().cmp(&maximum.version.cmp_key()) {
        std::cmp::Ordering::Greater => false,
        std::cmp::Ordering::Less => true,
        std::cmp::Ordering::Equal => !maximum.inclusive || candidate.inclusive == maximum.inclusive,
    }
}
