//! Rule: best-practices/no-unused-imports
//!
//! Detect unused imports.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::{ImportItems, ItemKind};
use solgrid_parser::with_parsed_ast_sequential;

static META: RuleMeta = RuleMeta {
    id: "best-practices/no-unused-imports",
    name: "no-unused-imports",
    category: RuleCategory::BestPractices,
    default_severity: Severity::Warning,
    description: "imported symbol is unused",
    fix_availability: FixAvailability::Available(FixSafety::Safe),
};

pub struct NoUnusedImportsRule;

impl Rule for NoUnusedImportsRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let filename = ctx.path.to_string_lossy().to_string();
        let result = with_parsed_ast_sequential(ctx.source, &filename, |source_unit| {
            let mut diagnostics = Vec::new();

            for item in source_unit.items.iter() {
                if let ItemKind::Import(import) = &item.kind {
                    match &import.items {
                        ImportItems::Aliases(aliases) => {
                            // Named imports: import {Foo, Bar as Baz} from "file.sol";
                            let import_range = solgrid_ast::span_to_range(item.span);
                            let import_end = import_range.end;

                            if import_end >= ctx.source.len() {
                                continue;
                            }

                            let rest = &ctx.source[import_end..];
                            let unused_aliases: Vec<_> = aliases
                                .iter()
                                .enumerate()
                                .filter_map(|(idx, alias)| {
                                    let imported_name = if let Some(alias_name) = alias.1 {
                                        alias_name.as_str().to_string()
                                    } else {
                                        alias.0.as_str().to_string()
                                    };

                                    (!is_identifier_used(rest, &imported_name)).then_some((
                                        idx,
                                        imported_name,
                                        solgrid_ast::span_to_range(
                                            alias
                                                .1
                                                .map(|alias_name| alias_name.span)
                                                .unwrap_or(alias.0.span),
                                        ),
                                    ))
                                })
                                .collect();

                            let unused_indexes: Vec<usize> =
                                unused_aliases.iter().map(|(idx, _, _)| *idx).collect();
                            let fix = build_unused_import_fix(
                                ctx.source,
                                &import_range,
                                aliases.len(),
                                &unused_indexes,
                            );

                            for (diag_idx, (_, imported_name, name_range)) in
                                unused_aliases.into_iter().enumerate()
                            {
                                let mut diag = Diagnostic::new(
                                    META.id,
                                    format!("imported symbol `{imported_name}` is unused"),
                                    META.default_severity,
                                    name_range,
                                );
                                if diag_idx == 0 {
                                    if let Some(fix) = fix.clone() {
                                        diag = diag.with_fix(fix);
                                    }
                                }
                                diagnostics.push(diag);
                            }
                        }
                        ImportItems::Glob(alias) => {
                            // import * as Alias from "file.sol";
                            let alias_name = alias.as_str();
                            let import_range = solgrid_ast::span_to_range(item.span);
                            let import_end = import_range.end;

                            if import_end < ctx.source.len() {
                                let rest = &ctx.source[import_end..];
                                if !is_identifier_used(rest, alias_name) {
                                    let name_range = solgrid_ast::span_to_range(alias.span);
                                    let fix = delete_import_line_fix(ctx.source, &import_range);
                                    diagnostics.push(
                                        Diagnostic::new(
                                            META.id,
                                            format!("imported symbol `{alias_name}` is unused"),
                                            META.default_severity,
                                            name_range,
                                        )
                                        .with_fix(fix),
                                    );
                                }
                            }
                        }
                        ImportItems::Plain(_) => {
                            // Global imports — skip (handled by no-global-import rule)
                        }
                    }
                }
            }

            diagnostics
        });

        result.unwrap_or_default()
    }
}

/// Check if an identifier is used in the given source text.
/// Uses word boundary checking to avoid false positives.
fn is_identifier_used(source: &str, name: &str) -> bool {
    let mut search_from = 0;
    while let Some(pos) = source[search_from..].find(name) {
        let abs_pos = search_from + pos;

        // Check word boundaries
        let before_ok = abs_pos == 0
            || !source.as_bytes()[abs_pos - 1].is_ascii_alphanumeric()
                && source.as_bytes()[abs_pos - 1] != b'_';

        let after_pos = abs_pos + name.len();
        let after_ok = after_pos >= source.len()
            || !source.as_bytes()[after_pos].is_ascii_alphanumeric()
                && source.as_bytes()[after_pos] != b'_';

        if before_ok && after_ok {
            // Make sure it's not inside a comment
            if !is_in_line_comment(source, abs_pos) {
                return true;
            }
        }

        search_from = abs_pos + name.len();
    }
    false
}

/// Build a fix that deletes an entire import line (including trailing newline).
fn delete_import_line_fix(source: &str, import_range: &std::ops::Range<usize>) -> Fix {
    let end = if import_range.end < source.len() && source.as_bytes()[import_range.end] == b'\n' {
        import_range.end + 1
    } else {
        import_range.end
    };
    Fix::safe(
        "Remove unused import",
        vec![TextEdit::delete(import_range.start..end)],
    )
}

/// Build a fix for an unused named import alias.
/// If this is the only alias (or all are unused), delete the entire import line.
/// Otherwise, remove just this alias from the `{...}` list.
fn build_unused_import_fix(
    source: &str,
    import_range: &std::ops::Range<usize>,
    total_aliases: usize,
    unused_indexes: &[usize],
) -> Option<Fix> {
    // If only one alias total, or all are unused, delete the entire line
    if total_aliases == 1 || unused_indexes.len() == total_aliases {
        return Some(delete_import_line_fix(source, import_range));
    }

    // Otherwise, remove just this alias from the braces
    let import_text = &source[import_range.clone()];
    let brace_open = import_text.find('{')?;
    let brace_close = import_text.find('}')?;
    let braces_content = &import_text[brace_open + 1..brace_close];

    // Split by comma, trim each, filter out the unused one
    let aliases: Vec<&str> = braces_content.split(',').map(|s| s.trim()).collect();
    let remaining: Vec<&str> = aliases
        .into_iter()
        .enumerate()
        .filter(|(idx, _)| !unused_indexes.contains(idx))
        .map(|(_, alias)| alias)
        .collect();

    if remaining.is_empty() {
        return Some(delete_import_line_fix(source, import_range));
    }

    let new_braces = format!("{{{}}}", remaining.join(", "));
    let abs_brace_start = import_range.start + brace_open;
    let abs_brace_end = import_range.start + brace_close + 1;

    Some(Fix::safe(
        "Remove unused import",
        vec![TextEdit::replace(
            abs_brace_start..abs_brace_end,
            new_braces,
        )],
    ))
}

/// Simple check if a position is inside a line comment.
fn is_in_line_comment(source: &str, pos: usize) -> bool {
    let before = &source[..pos];
    if let Some(last_newline) = before.rfind('\n') {
        let line = &before[last_newline..];
        if let Some(comment_pos) = line.find("//") {
            return last_newline + comment_pos < pos;
        }
    } else if let Some(comment_pos) = before.find("//") {
        return comment_pos < pos;
    }
    false
}
