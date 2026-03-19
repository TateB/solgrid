//! Rule: gas/bool-storage
//!
//! Using `bool` for storage variables costs more gas than `uint256` due to
//! extra SLOAD operations for EVM storage slot access patterns.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;

static META: RuleMeta = RuleMeta {
    id: "gas/bool-storage",
    name: "bool-storage",
    category: RuleCategory::Gas,
    default_severity: Severity::Info,
    description: "`bool` storage variables cost more gas than `uint256`",
    fix_availability: FixAvailability::None,
};

pub struct BoolStorageRule;

impl Rule for BoolStorageRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let mut in_contract = false;
        let mut brace_depth: i32 = 0;
        let mut contract_brace_depth: i32 = 0;
        let mut in_function = false;
        let mut function_brace_depth: i32 = 0;

        for line in ctx.source.lines() {
            let trimmed = line.trim();

            // Track brace depth
            for ch in trimmed.chars() {
                match ch {
                    '{' => {
                        brace_depth += 1;
                    }
                    '}' => {
                        brace_depth -= 1;
                        if in_function && brace_depth < function_brace_depth {
                            in_function = false;
                        }
                        if in_contract && brace_depth < contract_brace_depth {
                            in_contract = false;
                        }
                    }
                    _ => {}
                }
            }

            // Detect contract/struct/library start
            if (trimmed.starts_with("contract ")
                || trimmed.starts_with("abstract contract ")
                || trimmed.starts_with("library "))
                && trimmed.contains('{')
            {
                in_contract = true;
                contract_brace_depth = brace_depth;
            }

            // Detect function/modifier start
            if in_contract
                && (trimmed.starts_with("function ")
                    || trimmed.starts_with("modifier ")
                    || trimmed.starts_with("constructor"))
                && trimmed.contains('{')
            {
                in_function = true;
                function_brace_depth = brace_depth;
            }

            // Only flag state variables (inside contract, outside functions)
            if in_contract && !in_function {
                // Look for bool state variable declarations
                if let Some(bool_pos) = find_bool_state_var(line) {
                    let line_start = (line.as_ptr() as usize) - (ctx.source.as_ptr() as usize);
                    let abs_pos = line_start + bool_pos;
                    diagnostics.push(Diagnostic::new(
                        META.id,
                        "`bool` storage variable costs more gas than `uint256`; consider using `uint256(1)` and `uint256(0)` instead",
                        META.default_severity,
                        abs_pos..abs_pos + 4,
                    ));
                }
            }
        }
        diagnostics
    }
}

/// Find a `bool` state variable declaration in a line. Returns the offset of "bool" if found.
fn find_bool_state_var(line: &str) -> Option<usize> {
    // Skip comments
    let trimmed = line.trim();
    if trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with('*') {
        return None;
    }

    // Skip event, error, struct, enum, function declarations
    if trimmed.starts_with("event ")
        || trimmed.starts_with("error ")
        || trimmed.starts_with("struct ")
        || trimmed.starts_with("enum ")
        || trimmed.starts_with("function ")
        || trimmed.starts_with("mapping")
    {
        return None;
    }

    // Look for "bool" at the start of a declaration
    let mut search_from = 0;
    while let Some(pos) = line[search_from..].find("bool") {
        let abs_pos = search_from + pos;

        // Check word boundary before
        if abs_pos > 0 {
            let prev = line.as_bytes()[abs_pos - 1];
            if prev.is_ascii_alphanumeric() || prev == b'_' {
                search_from = abs_pos + 4;
                continue;
            }
        }

        // Check word boundary after
        let after = abs_pos + 4;
        if after < line.len() {
            let next = line.as_bytes()[after];
            if next.is_ascii_alphanumeric() || next == b'_' {
                search_from = abs_pos + 4;
                continue;
            }
        }

        // Must look like a state variable (has a semicolon on the line)
        if line.contains(';') {
            return Some(abs_pos);
        }

        search_from = abs_pos + 4;
    }
    None
}
