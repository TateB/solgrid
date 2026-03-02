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
                            for alias in aliases.iter() {
                                let imported_name = if let Some(alias_name) = alias.1 {
                                    // import {Foo as Bar} — check usage of "Bar"
                                    alias_name.as_str().to_string()
                                } else {
                                    // import {Foo} — check usage of "Foo"
                                    alias.0.as_str().to_string()
                                };
                                let imported_name = imported_name.as_str();

                                let import_range = solgrid_ast::span_to_range(item.span);
                                let import_end = import_range.end;

                                // Search in source text after the import statement
                                if import_end < ctx.source.len() {
                                    let rest = &ctx.source[import_end..];
                                    if !is_identifier_used(rest, imported_name) {
                                        let name_range = solgrid_ast::span_to_range(alias.0.span);
                                        let diag = Diagnostic::new(
                                            META.id,
                                            format!(
                                                "imported symbol `{imported_name}` is unused"
                                            ),
                                            META.default_severity,
                                            name_range,
                                        );
                                        diagnostics.push(diag);
                                    }
                                }
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
                                    diagnostics.push(Diagnostic::new(
                                        META.id,
                                        format!(
                                            "imported symbol `{alias_name}` is unused"
                                        ),
                                        META.default_severity,
                                        name_range,
                                    ));
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
