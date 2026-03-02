//! Rule: style/imports-ordering
//!
//! Sort import statements alphabetically by path.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;

static META: RuleMeta = RuleMeta {
    id: "style/imports-ordering",
    name: "imports-ordering",
    category: RuleCategory::Style,
    default_severity: Severity::Info,
    description: "import statements should be sorted alphabetically",
    fix_availability: FixAvailability::Available(FixSafety::Safe),
};

pub struct ImportsOrderingRule;

impl Rule for ImportsOrderingRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // Collect import statements and their paths
        let mut imports: Vec<(usize, usize, String)> = Vec::new(); // (line_start_offset, line_end_offset, path)
        let mut offset = 0;

        for line in ctx.source.split('\n') {
            let trimmed = line.trim();
            if trimmed.starts_with("import ") {
                if let Some(path) = extract_import_path(trimmed) {
                    imports.push((offset, offset + line.len(), path));
                }
            }
            offset += line.len() + 1;
        }

        // Check consecutive import groups are sorted
        if imports.len() < 2 {
            return diagnostics;
        }

        // Find groups of consecutive imports (separated by non-import lines)
        let mut group_start = 0;
        while group_start < imports.len() {
            let mut group_end = group_start + 1;
            while group_end < imports.len() {
                // Check if imports are on consecutive or near-consecutive lines
                let prev_end = imports[group_end - 1].1;
                let curr_start = imports[group_end].0;
                // Allow a small gap (blank lines between imports are okay)
                let gap = &ctx.source[prev_end..curr_start];
                if gap.trim().is_empty() || gap.split('\n').count() <= 2 {
                    group_end += 1;
                } else {
                    break;
                }
            }

            // Check if this group is sorted
            for i in group_start + 1..group_end {
                if imports[i].2.to_lowercase() < imports[i - 1].2.to_lowercase() {
                    diagnostics.push(Diagnostic::new(
                        META.id,
                        format!(
                            "import `{}` should appear before `{}`",
                            imports[i].2,
                            imports[i - 1].2
                        ),
                        META.default_severity,
                        imports[i].0..imports[i].1,
                    ));
                    break; // Report once per group
                }
            }

            group_start = group_end;
        }

        diagnostics
    }
}

/// Extract the import path from an import statement.
fn extract_import_path(line: &str) -> Option<String> {
    // Match patterns:
    //   import "path";
    //   import {Foo} from "path";
    //   import * as Foo from "path";
    for quote in ['"', '\''] {
        if let Some(start) = line.rfind(quote) {
            let before = &line[..start];
            if let Some(path_start) = before.rfind(quote) {
                let path = &line[path_start + 1..start];
                return Some(path.to_string());
            }
        }
    }
    None
}
