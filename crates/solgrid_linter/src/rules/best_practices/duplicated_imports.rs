//! Rule: best-practices/duplicated-imports
//!
//! Detect duplicate imports in the same file.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::{ImportItems, ItemKind};
use solgrid_parser::with_parsed_ast_sequential;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::path::Path;

static META: RuleMeta = RuleMeta {
    id: "best-practices/duplicated-imports",
    name: "duplicated-imports",
    category: RuleCategory::BestPractices,
    default_severity: Severity::Warning,
    description: "imported symbols should not be duplicated in the same file",
    fix_availability: FixAvailability::None,
};

pub struct DuplicatedImportsRule;

#[derive(Clone)]
struct ImportedObject {
    source_name: String,
    source_span: Range<usize>,
    local_name: Option<String>,
    local_span: Option<Range<usize>>,
}

struct ImportStatementData {
    path: String,
    objects: Vec<ImportedObject>,
}

impl Rule for DuplicatedImportsRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let filename = ctx.path.to_string_lossy().to_string();
        with_parsed_ast_sequential(ctx.source, &filename, |source_unit| {
            let imports = source_unit
                .items
                .iter()
                .filter_map(|item| match &item.kind {
                    ItemKind::Import(import) => Some(import_statement(item, import)),
                    _ => None,
                })
                .collect::<Vec<_>>();

            let mut diagnostics = inline_duplicates(&imports);
            diagnostics.extend(global_same_path_duplicates(&imports));
            diagnostics.extend(global_diff_path_duplicates(&imports));
            diagnostics
        })
        .unwrap_or_default()
    }
}

fn import_statement(
    item: &solgrid_parser::solar_ast::Item<'_>,
    import: &solgrid_parser::solar_ast::ImportDirective<'_>,
) -> ImportStatementData {
    let path = normalize_path(import.path.value.as_str());
    let item_span = solgrid_ast::span_to_range(item.span);

    let objects = match &import.items {
        ImportItems::Aliases(aliases) => aliases
            .iter()
            .map(|(name, alias)| {
                let source_name = name.as_str().to_string();
                let local_name = alias
                    .map(|ident| ident.as_str().to_string())
                    .unwrap_or_else(|| source_name.clone());
                let source_span = solgrid_ast::span_to_range(name.span);
                let local_span = alias
                    .map(|ident| solgrid_ast::span_to_range(ident.span))
                    .unwrap_or_else(|| source_span.clone());

                ImportedObject {
                    source_name,
                    source_span,
                    local_name: Some(local_name),
                    local_span: Some(local_span),
                }
            })
            .collect(),
        ImportItems::Plain(alias) => vec![ImportedObject {
            source_name: object_name_from_path(&path),
            source_span: item_span.clone(),
            local_name: Some(
                alias
                    .map(|ident| ident.as_str().to_string())
                    .unwrap_or_else(|| object_name_from_path(&path)),
            ),
            local_span: Some(
                alias
                    .map(|ident| solgrid_ast::span_to_range(ident.span))
                    .unwrap_or(item_span),
            ),
        }],
        // Solhint does not consider namespace imports in this rule.
        ImportItems::Glob(_) => Vec::new(),
    };

    ImportStatementData { path, objects }
}

fn inline_duplicates(imports: &[ImportStatementData]) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for import in imports {
        let mut seen = HashSet::new();
        let mut reported = HashSet::new();

        for object in &import.objects {
            if !seen.insert(object.source_name.as_str())
                && reported.insert(object.source_name.as_str())
            {
                diagnostics.push(Diagnostic::new(
                    META.id,
                    format!(
                        "`{}` is imported more than once in the same import statement",
                        object.source_name
                    ),
                    META.default_severity,
                    object.source_span.clone(),
                ));
            }
        }
    }

    diagnostics
}

fn global_same_path_duplicates(imports: &[ImportStatementData]) -> Vec<Diagnostic> {
    let mut occurrences: HashMap<(String, String), Vec<Range<usize>>> = HashMap::new();

    for import in imports {
        let mut names_in_statement = HashMap::new();
        for object in &import.objects {
            names_in_statement
                .entry(object.source_name.clone())
                .or_insert_with(|| object.source_span.clone());
        }

        for (name, span) in names_in_statement {
            occurrences
                .entry((import.path.clone(), name))
                .or_default()
                .push(span);
        }
    }

    occurrences
        .into_iter()
        .filter_map(|((path, name), spans)| {
            (spans.len() > 1).then(|| {
                Diagnostic::new(
                    META.id,
                    format!("`{name}` is imported more than once from `{path}`"),
                    META.default_severity,
                    spans[1].clone(),
                )
            })
        })
        .collect()
}

fn global_diff_path_duplicates(imports: &[ImportStatementData]) -> Vec<Diagnostic> {
    let mut occurrences: HashMap<String, Vec<(String, Range<usize>)>> = HashMap::new();

    for import in imports {
        let mut names_in_statement = HashMap::new();
        for object in &import.objects {
            if let Some(local_name) = &object.local_name {
                names_in_statement
                    .entry(local_name.clone())
                    .or_insert_with(|| {
                        object
                            .local_span
                            .clone()
                            .unwrap_or_else(|| object.source_span.clone())
                    });
            }
        }

        for (name, span) in names_in_statement {
            let entry = occurrences.entry(name).or_default();
            if !entry.iter().any(|(path, _)| path == &import.path) {
                entry.push((import.path.clone(), span));
            }
        }
    }

    occurrences
        .into_iter()
        .filter_map(|(name, entries)| {
            (entries.len() > 1).then(|| {
                Diagnostic::new(
                    META.id,
                    format!("`{name}` is imported from multiple paths"),
                    META.default_severity,
                    entries[1].1.clone(),
                )
            })
        })
        .collect()
}

fn normalize_path(path: &str) -> String {
    if path.starts_with("../") {
        format!("./{path}")
    } else {
        path.to_string()
    }
}

fn object_name_from_path(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(path)
        .to_string()
}
