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
struct ImportStatement {
    start: usize,
    end: usize,
    path: String,
    blank_before: bool,
}

impl Rule for ImportsOrderingRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        let imports = collect_import_statements(ctx.source);

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
                let prev_end = imports[group_end - 1].end;
                let curr_start = imports[group_end].start;
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

                let group_range = imports[group_start].start..imports[group_end - 1].end;
                let mut sorted_text = String::new();
                for (idx, import) in sorted.iter().enumerate() {
                    if idx > 0 {
                        if import.blank_before {
                            sorted_text.push_str("\n\n");
                        } else {
                            sorted_text.push('\n');
                        }
                    }
                    sorted_text.push_str(&ctx.source[import.start..import.end]);
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
                            imports[violation_idx].start..imports[violation_idx].end,
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

fn collect_import_statements(source: &str) -> Vec<ImportStatement> {
    let mut imports = Vec::new();
    let mut offset = 0;
    let mut previous_import_end: Option<usize> = None;
    let mut current_start: Option<usize> = None;

    for line in source.split('\n') {
        let line_end = offset + line.len();
        let trimmed = line.trim();

        if current_start.is_none() && trimmed.starts_with("import ") {
            current_start = Some(offset);
        }

        if let Some(start) = current_start {
            if trimmed.ends_with(';') {
                let statement = &source[start..line_end];
                if let Some(path) = extract_import_path(statement) {
                    let blank_before = previous_import_end
                        .map(|prev_end| has_blank_line_between(&source[prev_end..start]))
                        .unwrap_or(false);
                    imports.push(ImportStatement {
                        start,
                        end: line_end,
                        path,
                        blank_before,
                    });
                    previous_import_end = Some(line_end);
                }
                current_start = None;
            }
        }

        offset = line_end + 1;
    }

    imports
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
