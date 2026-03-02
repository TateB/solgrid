//! Rule: style/no-multiple-empty-lines
//!
//! Disallow more than a specified number of consecutive empty lines (default: 2).

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;

static META: RuleMeta = RuleMeta {
    id: "style/no-multiple-empty-lines",
    name: "no-multiple-empty-lines",
    category: RuleCategory::Style,
    default_severity: Severity::Info,
    description: "no more than 2 consecutive empty lines",
    fix_availability: FixAvailability::Available(FixSafety::Safe),
};

pub struct NoMultipleEmptyLinesRule;

const MAX_CONSECUTIVE_EMPTY: usize = 2;

impl Rule for NoMultipleEmptyLinesRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let mut consecutive_empty = 0usize;
        let mut run_start_offset: Option<usize> = None;
        let mut offset = 0;

        for line in ctx.source.split('\n') {
            if line.trim().is_empty() {
                consecutive_empty += 1;
                if consecutive_empty == MAX_CONSECUTIVE_EMPTY + 1 && run_start_offset.is_none() {
                    // The extra empty lines start after the allowed ones.
                    // We need to track from the start of this excess line.
                    run_start_offset = Some(offset);
                }
            } else {
                if consecutive_empty > MAX_CONSECUTIVE_EMPTY {
                    if let Some(start) = run_start_offset {
                        // Delete from the start of excess empty lines to the current position
                        diagnostics.push(
                            Diagnostic::new(
                                META.id,
                                format!(
                                    "more than {MAX_CONSECUTIVE_EMPTY} consecutive empty lines"
                                ),
                                META.default_severity,
                                start..offset,
                            )
                            .with_fix(Fix::safe(
                                format!("Collapse to {MAX_CONSECUTIVE_EMPTY} empty lines"),
                                vec![TextEdit::delete(start..offset)],
                            )),
                        );
                    }
                }
                consecutive_empty = 0;
                run_start_offset = None;
            }
            offset += line.len() + 1; // +1 for '\n'
        }

        // Handle trailing excess empty lines
        if consecutive_empty > MAX_CONSECUTIVE_EMPTY {
            if let Some(start) = run_start_offset {
                let end = ctx.source.len();
                diagnostics.push(
                    Diagnostic::new(
                        META.id,
                        format!("more than {MAX_CONSECUTIVE_EMPTY} consecutive empty lines"),
                        META.default_severity,
                        start..end,
                    )
                    .with_fix(Fix::safe(
                        format!("Collapse to {MAX_CONSECUTIVE_EMPTY} empty lines"),
                        vec![TextEdit::delete(start..end)],
                    )),
                );
            }
        }

        diagnostics
    }
}
