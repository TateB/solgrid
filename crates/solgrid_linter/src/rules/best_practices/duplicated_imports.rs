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
    name: String,
    alias: Option<String>,
    span: Range<usize>,
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
            .map(|(name, alias)| ImportedObject {
                name: name.as_str().to_string(),
                alias: alias.map(|ident| ident.as_str().to_string()),
                span: solgrid_ast::span_to_range(name.span),
            })
            .collect(),
        ImportItems::Plain(alias) => vec![ImportedObject {
            name: object_name_from_path(&path),
            alias: alias.map(|ident| ident.as_str().to_string()),
            span: item_span,
        }],
        ImportItems::Glob(alias) => vec![ImportedObject {
            name: object_name_from_path(&path),
            alias: Some(alias.as_str().to_string()),
            span: item_span,
        }],
    };

    ImportStatementData { path, objects }
}

fn inline_duplicates(imports: &[ImportStatementData]) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for import in imports {
        let mut seen = HashSet::new();
        let mut reported = HashSet::new();

        for object in &import.objects {
            if !seen.insert(object.name.as_str()) && reported.insert(object.name.as_str()) {
                diagnostics.push(Diagnostic::new(
                    META.id,
                    format!(
                        "`{}` is imported more than once in the same import statement",
                        object.name
                    ),
                    META.default_severity,
                    object.span.clone(),
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
                .entry(object.name.clone())
                .or_insert_with(|| object.span.clone());
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
            if object.alias.is_none() {
                names_in_statement
                    .entry(object.name.clone())
                    .or_insert_with(|| object.span.clone());
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
                    format!("`{name}` is imported from multiple paths without an alias"),
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
