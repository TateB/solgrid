//! Rule: style/imports-ordering
//!
//! Enforce grouped import ordering with blank-line separation.

use crate::context::LintContext;
use crate::rule::Rule;
use regex::Regex;
use serde::Deserialize;
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::{ImportItems, ItemKind};
use solgrid_parser::with_parsed_ast_sequential;
use std::cmp::Ordering;

static META: RuleMeta = RuleMeta {
    id: "style/imports-ordering",
    name: "imports-ordering",
    category: RuleCategory::Style,
    default_severity: Severity::Info,
    description: "imports should be ordered by group and path, with blank lines between groups",
    fix_availability: FixAvailability::Available(FixSafety::Safe),
};

pub struct ImportsOrderingRule;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct ImportsOrderingSettings {
    import_order: Vec<String>,
}

#[derive(Clone)]
struct ImportStatement {
    start: usize,
    end: usize,
    path: String,
    group: usize,
    sort_path: String,
    text: String,
}

impl Rule for ImportsOrderingRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let settings: ImportsOrderingSettings = ctx.config.rule_settings(META.id);
        let compiled_patterns: Vec<Regex> = settings
            .import_order
            .iter()
            .filter_map(|pattern| Regex::new(pattern).ok())
            .collect();

        let filename = ctx.path.to_string_lossy().to_string();
        with_parsed_ast_sequential(ctx.source, &filename, |source_unit| {
            let mut diagnostics = Vec::new();
            let mut import_block = Vec::new();

            for item in source_unit.items.iter() {
                match &item.kind {
                    ItemKind::Import(import) => {
                        import_block.push(import_statement(
                            ctx.source,
                            import,
                            item.span,
                            &compiled_patterns,
                        ));
                    }
                    _ => {
                        diagnostics.extend(check_import_block(ctx.source, &import_block));
                        import_block.clear();
                    }
                }
            }

            diagnostics.extend(check_import_block(ctx.source, &import_block));
            diagnostics
        })
        .unwrap_or_default()
    }
}

fn import_statement(
    source: &str,
    import: &solgrid_parser::solar_ast::ImportDirective<'_>,
    span: solgrid_parser::solar_interface::Span,
    compiled_patterns: &[Regex],
) -> ImportStatement {
    let span = solgrid_ast::span_to_range(span);
    let end = line_end(source, span.end);
    let path = import.path.value.to_string();
    let group = group_index(&path, compiled_patterns);
    ImportStatement {
        start: span.start,
        end,
        sort_path: path.to_ascii_lowercase(),
        text: reconstruct_import(import),
        path,
        group,
    }
}

fn check_import_block(source: &str, imports: &[ImportStatement]) -> Vec<Diagnostic> {
    if imports.len() < 2 {
        return Vec::new();
    }

    let mut sorted = imports.to_vec();
    sorted.sort_by(compare_imports);

    if imports_out_of_order(imports, &sorted) {
        let fix = import_rewrite_fix(source, imports, &sorted);

        return imports
            .iter()
            .zip(sorted.iter())
            .filter(|(actual, expected)| {
                actual.group != expected.group || actual.sort_path != expected.sort_path
            })
            .map(|(actual, expected)| {
                let diagnostic = Diagnostic::new(
                    META.id,
                    format!(
                        "import `{}` should appear before `{}`",
                        actual.path, expected.path
                    ),
                    META.default_severity,
                    actual.start..actual.end,
                );

                match &fix {
                    Some(fix) => diagnostic.with_fix(fix.clone()),
                    None => diagnostic,
                }
            })
            .collect();
    }

    spacing_diagnostics(source, imports)
}

fn import_rewrite_fix(
    source: &str,
    imports: &[ImportStatement],
    sorted: &[ImportStatement],
) -> Option<Fix> {
    // Avoid offering a destructive safe fix when comments or declarations are
    // interleaved between imports.
    if imports
        .windows(2)
        .any(|pair| !source[pair[0].end..pair[1].start].trim().is_empty())
    {
        return None;
    }

    let replacement = render_import_block(sorted);
    let full_range = imports[0].start
        ..imports
            .last()
            .map(|import| import.end)
            .unwrap_or(imports[0].end);

    Some(Fix::safe(
        "Rewrite import block with canonical ordering",
        vec![TextEdit::replace(full_range, replacement)],
    ))
}

fn group_index(path: &str, patterns: &[Regex]) -> usize {
    if patterns.is_empty() {
        return if path.starts_with('.') { 1 } else { 0 };
    }

    patterns
        .iter()
        .position(|pattern| pattern.is_match(path))
        .unwrap_or(patterns.len())
}

fn compare_imports(left: &ImportStatement, right: &ImportStatement) -> Ordering {
    left.group
        .cmp(&right.group)
        .then_with(|| left.sort_path.cmp(&right.sort_path))
}

fn imports_out_of_order(actual: &[ImportStatement], sorted: &[ImportStatement]) -> bool {
    actual
        .iter()
        .zip(sorted.iter())
        .any(|(left, right)| left.group != right.group || left.sort_path != right.sort_path)
}

fn render_import_block(imports: &[ImportStatement]) -> String {
    let mut rendered = String::new();

    for (index, import) in imports.iter().enumerate() {
        if index > 0 {
            if imports[index - 1].group != import.group {
                rendered.push_str("\n\n");
            } else {
                rendered.push('\n');
            }
        }
        rendered.push_str(&import.text);
    }

    rendered
}

fn spacing_diagnostics(source: &str, imports: &[ImportStatement]) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for pair in imports.windows(2) {
        let [previous, current] = pair else {
            continue;
        };
        if previous.group == current.group {
            continue;
        }

        let gap = &source[previous.end..current.start];
        if !gap.trim().is_empty() {
            continue;
        }
        let blank_lines = gap.matches('\n').count().saturating_sub(1);
        if blank_lines > 0 {
            continue;
        }

        diagnostics.push(
            Diagnostic::new(
                META.id,
                format!(
                    "import group for `{}` should be separated from the previous group by a blank line",
                    current.path
                ),
                META.default_severity,
                current.start..current.end,
            )
            .with_fix(Fix::safe(
                "Insert blank line between import groups",
                vec![TextEdit::insert(current.start, "\n")],
            )),
        );
    }

    diagnostics
}

fn reconstruct_import(import: &solgrid_parser::solar_ast::ImportDirective<'_>) -> String {
    match &import.items {
        ImportItems::Plain(alias) => match alias {
            Some(alias) => format!("import \"{}\" as {};", import.path.value, alias.as_str()),
            None => format!("import \"{}\";", import.path.value),
        },
        ImportItems::Aliases(aliases) => {
            let items = aliases
                .iter()
                .map(|(name, alias)| match alias {
                    Some(alias) => format!("{} as {}", name.as_str(), alias.as_str()),
                    None => name.as_str().to_string(),
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("import {{{items}}} from \"{}\";", import.path.value)
        }
        ImportItems::Glob(alias) => {
            format!(
                "import * as {} from \"{}\";",
                alias.as_str(),
                import.path.value
            )
        }
    }
}

fn line_end(source: &str, pos: usize) -> usize {
    source[pos..]
        .find('\n')
        .map(|offset| pos + offset)
        .unwrap_or(source.len())
}
