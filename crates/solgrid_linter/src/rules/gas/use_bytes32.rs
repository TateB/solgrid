//! Rule: gas/use-bytes32
//!
//! Use `bytes32` instead of `string` for short fixed-length strings. `bytes32`
//! fits in a single storage slot while `string` requires dynamic storage.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;

static META: RuleMeta = RuleMeta {
    id: "gas/use-bytes32",
    name: "use-bytes32",
    category: RuleCategory::Gas,
    default_severity: Severity::Info,
    description: "use `bytes32` instead of `string` for short fixed-length data",
    fix_availability: FixAvailability::None,
};

pub struct UseBytes32Rule;

impl Rule for UseBytes32Rule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let mut in_contract = false;
        let mut in_function = false;
        let mut brace_depth: i32 = 0;
        let mut contract_brace_depth: i32 = 0;
        let mut function_brace_depth: i32 = 0;

        for line in ctx.source.lines() {
            let trimmed = line.trim();

            for ch in trimmed.chars() {
                match ch {
                    '{' => brace_depth += 1,
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

            if (trimmed.starts_with("contract ")
                || trimmed.starts_with("abstract contract ")
                || trimmed.starts_with("library "))
                && trimmed.contains('{')
            {
                in_contract = true;
                contract_brace_depth = brace_depth;
            }

            if in_contract
                && (trimmed.starts_with("function ")
                    || trimmed.starts_with("modifier ")
                    || trimmed.starts_with("constructor"))
                && trimmed.contains('{')
            {
                in_function = true;
                function_brace_depth = brace_depth;
            }

            // Only check state variable declarations (inside contract, outside function)
            if in_contract && !in_function && trimmed.contains(';') {
                // Look for `string` state variables with short literal initializers
                if let Some(string_pos) = find_string_state_var(trimmed) {
                    // Check if it has a short string literal initializer
                    if has_short_string_initializer(trimmed) {
                        let line_start = (line.as_ptr() as usize) - (ctx.source.as_ptr() as usize);
                        let abs_pos = line_start + string_pos;
                        diagnostics.push(Diagnostic::new(
                            META.id,
                            "consider using `bytes32` instead of `string` for short fixed-length data to save gas",
                            META.default_severity,
                            abs_pos..abs_pos + 6,
                        ));
                    }
                }
            }
        }
        diagnostics
    }
}

fn find_string_state_var(line: &str) -> Option<usize> {
    let trimmed = line.trim();
    if trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with('*') {
        return None;
    }

    let mut search_from = 0;
    while let Some(pos) = line[search_from..].find("string") {
        let abs_pos = search_from + pos;

        // Check word boundary before
        if abs_pos > 0 {
            let prev = line.as_bytes()[abs_pos - 1];
            if prev.is_ascii_alphanumeric() || prev == b'_' {
                search_from = abs_pos + 6;
                continue;
            }
        }

        // Check word boundary after
        let after = abs_pos + 6;
        if after < line.len() {
            let next = line.as_bytes()[after];
            if next.is_ascii_alphanumeric() || next == b'_' {
                search_from = abs_pos + 6;
                continue;
            }
        }

        return Some(abs_pos);
    }
    None
}

fn has_short_string_initializer(line: &str) -> bool {
    // Look for `= "..."` with string <= 32 bytes
    if let Some(eq_pos) = line.find('=') {
        let after_eq = &line[eq_pos + 1..];
        if let Some(q1) = after_eq.find('"') {
            let after_quote = &after_eq[q1 + 1..];
            if let Some(q2) = after_quote.find('"') {
                let content = &after_quote[..q2];
                return content.len() <= 32;
            }
        }
    }
    false
}
