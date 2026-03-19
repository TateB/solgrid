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

#[derive(Clone)]
struct ImportLine {
    line_start: usize,
    line_end: usize,
    path: String,
    blank_before: bool,
}

impl Rule for ImportsOrderingRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // Collect import statements and their paths
        let mut imports: Vec<ImportLine> = Vec::new();
        let mut offset = 0;
        let mut previous_import_end: Option<usize> = None;

        for line in ctx.source.split('\n') {
            let trimmed = line.trim();
            if trimmed.starts_with("import ") {
                if let Some(path) = extract_import_path(trimmed) {
                    let blank_before = previous_import_end
                        .map(|prev_end| has_blank_line_between(&ctx.source[prev_end..offset]))
                        .unwrap_or(false);
                    imports.push(ImportLine {
                        line_start: offset,
                        line_end: offset + line.len(),
                        path,
                        blank_before,
                    });
                    previous_import_end = Some(offset + line.len());
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
                let prev_end = imports[group_end - 1].line_end;
                let curr_start = imports[group_end].line_start;
                // Allow a small gap (blank lines between imports are okay)
                let gap = &ctx.source[prev_end..curr_start];
                if gap.trim().is_empty() || gap.split('\n').count() <= 2 {
                    group_end += 1;
                } else {
                    break;
                }
            }

            // Check if this group is sorted
            let mut max_path = imports[group_start].path.to_lowercase();
            let mut violation_indexes = Vec::new();
            for (i, import) in imports
                .iter()
                .enumerate()
                .take(group_end)
                .skip(group_start + 1)
            {
                let path_lower = import.path.to_lowercase();
                if path_lower < max_path {
                    violation_indexes.push(i);
                } else {
                    max_path = path_lower;
                }
            }

            if !violation_indexes.is_empty() {
                let mut sorted = imports[group_start..group_end].to_vec();
                sorted.sort_by(|a, b| a.path.to_lowercase().cmp(&b.path.to_lowercase()));

                let group_range = imports[group_start].line_start..imports[group_end - 1].line_end;
                let mut sorted_text = String::new();
                for (idx, import) in sorted.iter().enumerate() {
                    if idx > 0 {
                        if import.blank_before {
                            sorted_text.push_str("\n\n");
                        } else {
                            sorted_text.push('\n');
                        }
                    }
                    sorted_text.push_str(&ctx.source[import.line_start..import.line_end]);
                }

                let fix = Fix::safe(
                    "Sort imports alphabetically",
                    vec![TextEdit::replace(group_range, sorted_text)],
                );

                for violation_idx in violation_indexes {
                    diagnostics.push(
                        Diagnostic::new(
                            META.id,
                            format!(
                                "import `{}` should appear before `{}`",
                                imports[violation_idx].path,
                                imports[violation_idx - 1].path
                            ),
                            META.default_severity,
                            imports[violation_idx].line_start..imports[violation_idx].line_end,
                        )
                        .with_fix(fix.clone()),
                    );
                }
            }

            group_start = group_end;
        }

        diagnostics
    }
}

fn has_blank_line_between(gap: &str) -> bool {
    gap.lines().filter(|line| line.trim().is_empty()).count() > 1
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
