//! Rule: docs/license-identifier
//!
//! File must contain an SPDX license identifier.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;

static META: RuleMeta = RuleMeta {
    id: "docs/license-identifier",
    name: "license-identifier",
    category: RuleCategory::Docs,
    default_severity: Severity::Info,
    description: "file must contain an SPDX license identifier",
    fix_availability: FixAvailability::None,
};

pub struct LicenseIdentifierRule;

impl Rule for LicenseIdentifierRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        if ctx.source.contains("SPDX-License-Identifier:") {
            return Vec::new();
        }

        vec![Diagnostic::new(
            META.id,
            "file is missing SPDX license identifier (e.g., `// SPDX-License-Identifier: MIT`)",
            META.default_severity,
            0..0,
        )]
    }
}
