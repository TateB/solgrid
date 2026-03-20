//! Rule: security/compiler-version
//!
//! Ensure a Solidity pragma is present and that the compiler version is not
//! outdated.  Flags files missing `pragma solidity` and files that specify
//! Solidity 0.4.x, 0.5.x, 0.6.x, or 0.7.x, which are known to contain
//! compiler bugs and lack important security features.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_config::SolidityVersion;
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
                    Ok(Some(ref requirements)) => {
                        let versions = extract_versions(version_text);
                        let is_allowed = versions.iter().any(|version| {
                            requirements
                                .iter()
                                .all(|requirement| requirement.matches(*version))
                        });
                        if !is_allowed {
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
                    Ok(None) | Err(_) => {
                        let outdated_prefixes = ["0.4", "0.5", "0.6", "0.7"];
                        for prefix in &outdated_prefixes {
                            if version_text.contains(prefix) {
                                diagnostics.push(Diagnostic::new(
                                    META.id,
                                    format!(
                                        "compiler version is outdated; Solidity {prefix}.x has known bugs — use 0.8.x or later"
                                    ),
                                    META.default_severity,
                                    pos..span_end,
                                ));
                                break;
                            }
                        }
                    }
                }
            }
        }
        diagnostics
    }
}

fn extract_versions(version_text: &str) -> Vec<SolidityVersion> {
    version_text
        .split(|ch: char| !(ch.is_ascii_digit() || ch == '.'))
        .filter_map(|token| {
            if token.is_empty() {
                None
            } else {
                SolidityVersion::parse(token)
            }
        })
        .collect()
}
