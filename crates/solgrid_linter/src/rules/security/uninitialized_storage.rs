//! Rule: security/uninitialized-storage
//!
//! Detect uninitialized local storage pointers. In Solidity <0.5.0,
//! uninitialized storage pointers default to storage slot 0, which
//! can lead to overwriting critical state.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;

static META: RuleMeta = RuleMeta {
    id: "security/uninitialized-storage",
    name: "uninitialized-storage",
    category: RuleCategory::Security,
    default_severity: Severity::Error,
    description: "local variable with `storage` location has no initializer",
    fix_availability: FixAvailability::None,
};

pub struct UninitializedStorageRule;

impl Rule for UninitializedStorageRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // Search for patterns like: `TypeName storage varName;`
        // This is a local variable with storage location but no initializer.
        // We look for lines matching: <type> storage <name>;
        // (not <type> storage <name> = ...)
        for (line_idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();

            // Skip comments
            if trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with('*') {
                continue;
            }

            // Look for "storage" keyword in local variable declarations
            if !trimmed.contains(" storage ") {
                continue;
            }

            // Must end with ; (it's a statement, not a parameter)
            if !trimmed.ends_with(';') {
                continue;
            }

            // Must not contain = (no initializer)
            if trimmed.contains('=') {
                continue;
            }

            // Must not start with state variable keywords
            if trimmed.starts_with("mapping")
                || trimmed.starts_with("function")
                || trimmed.starts_with("event")
                || trimmed.starts_with("error")
                || trimmed.starts_with("modifier")
            {
                continue;
            }

            // Skip if it looks like a function parameter (contains a comma or is inside parens context)
            // Function parameters use storage too but they're not dangerous in the same way
            // We only want local variable declarations inside function bodies

            // Parse the pattern: <type> storage <name>;
            if let Some(storage_pos) = trimmed.find(" storage ") {
                let after_storage = &trimmed[storage_pos + " storage ".len()..];
                let var_name = after_storage.trim_end_matches(';').trim();

                // Variable name should be a single identifier
                if !var_name.is_empty()
                    && var_name.chars().all(|c| c.is_alphanumeric() || c == '_')
                    && var_name.starts_with(|c: char| c.is_alphabetic() || c == '_')
                {
                    // Calculate byte offset for this line
                    let line_start = ctx
                        .source
                        .lines()
                        .take(line_idx)
                        .map(|l| l.len() + 1) // +1 for newline
                        .sum::<usize>();
                    let trimmed_offset = line.find(trimmed).unwrap_or(0);
                    let abs_start = line_start + trimmed_offset;
                    let abs_end = abs_start + trimmed.len();

                    diagnostics.push(Diagnostic::new(
                        META.id,
                        format!("local storage variable `{var_name}` is not initialized"),
                        META.default_severity,
                        abs_start..abs_end,
                    ));
                }
            }
        }

        diagnostics
    }
}
