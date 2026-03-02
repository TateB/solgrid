//! Rule trait and metadata.

use solgrid_diagnostics::{Diagnostic, RuleMeta};

use crate::context::LintContext;

/// A lint rule that can check Solidity source code.
pub trait Rule: Send + Sync {
    /// Returns the rule's metadata.
    fn meta(&self) -> &RuleMeta;

    /// Check the source code and return any diagnostics.
    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic>;
}
