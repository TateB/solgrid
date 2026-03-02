//! Rule: security/compiler-version
//!
//! Ensure a Solidity pragma is present and that the compiler version is not
//! outdated.  Flags files missing `pragma solidity` and files that specify
//! Solidity 0.4.x, 0.5.x, 0.6.x, or 0.7.x, which are known to contain
//! compiler bugs and lack important security features.

use crate::context::LintContext;
use crate::rule::Rule;
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

                let outdated_prefixes = ["0.4", "0.5", "0.6", "0.7"];
                for prefix in &outdated_prefixes {
                    if version_text.contains(prefix) {
                        let span_end = pos + pattern_len + line_end;
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
        diagnostics
    }
}
