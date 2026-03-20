//! Rule: style/category-headers
//!
//! Enforce canonical category header sections inside contract-like bodies.

use crate::context::LintContext;
use crate::rule::Rule;
use serde::Deserialize;
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::{
    ContractKind, FunctionKind, Item, ItemFunction, ItemKind, Visibility,
};
use solgrid_parser::with_parsed_ast_sequential;
use std::collections::{BTreeMap, BTreeSet, HashSet};

static META: RuleMeta = RuleMeta {
    id: "style/category-headers",
    name: "category-headers",
    category: RuleCategory::Style,
    default_severity: Severity::Info,
    description: "contract members should be grouped under canonical category headers",
    fix_availability: FixAvailability::Available(FixSafety::Suggestion),
};

pub struct CategoryHeadersRule;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct CategoryHeadersSettings {
    min_categories: usize,
    initialization_functions: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum CoarseCategory {
    Types,
    Constants,
    Storage,
    Events,
    Errors,
    Modifiers,
    Initialization,
    Functions,
}

const CANONICAL_CATEGORIES: [&str; 13] = [
    "Types",
    "Constants & Immutables",
    "Constants",
    "Immutables",
    "Storage",
    "Events",
    "Errors",
    "Modifiers",
    "Initialization",
    "Functions",
    "Implementation",
    "Internal Functions",
    "Private Functions",
];

impl Default for CategoryHeadersSettings {
    fn default() -> Self {
        Self {
            min_categories: 2,
            initialization_functions: Vec::new(),
        }
    }
}

impl Rule for CategoryHeadersRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let settings: CategoryHeadersSettings = ctx.config.rule_settings(META.id);
        let filename = ctx.path.to_string_lossy().to_string();

        with_parsed_ast_sequential(ctx.source, &filename, |source_unit| {
            let mut diagnostics = Vec::new();

            for item in source_unit.items.iter() {
                let ItemKind::Contract(contract) = &item.kind else {
                    continue;
                };

                let contract_range = solgrid_ast::span_to_range(item.span);
                let Some((body_start, body_end)) =
                    contract_body_bounds(ctx.source, contract_range.clone())
                else {
                    continue;
                };

                let headers = scan_headers(ctx.source, body_start, body_end);
                let unknown_headers: Vec<_> = headers
                    .iter()
                    .filter(|header| !is_known_category(&header.name))
                    .collect();

                let categorized_items: Vec<_> = contract
                    .body
                    .iter()
                    .filter_map(|body_item| {
                        categorize_item(ctx.source, contract.kind, body_item, &settings)
                    })
                    .collect();

                let coarse_categories: BTreeSet<_> =
                    categorized_items.iter().map(|item| item.coarse).collect();
                if coarse_categories.len() < settings.min_categories {
                    continue;
                }

                for header in &unknown_headers {
                    diagnostics.push(Diagnostic::new(
                        META.id,
                        format!("unknown category name `{}`", header.name),
                        META.default_severity,
                        header.label_span.clone(),
                    ));
                }

                if !unknown_headers.is_empty() {
                    continue;
                }

                let replacement = rebuild_contract_body(
                    ctx.source,
                    contract.kind,
                    contract.body.iter().collect(),
                    body_start,
                    body_end,
                    &headers,
                    &settings,
                );

                if ctx.source[body_start..body_end] == replacement {
                    continue;
                }

                diagnostics.push(
                    Diagnostic::new(
                        META.id,
                        "contract body should be organized under canonical category headers",
                        META.default_severity,
                        solgrid_ast::item_name_range(item),
                    )
                    .with_fix(Fix::suggestion(
                        "Rebuild category headers",
                        vec![TextEdit::replace(body_start..body_end, replacement)],
                    )),
                );
            }

            diagnostics
        })
        .unwrap_or_default()
    }
}

#[derive(Debug, Clone)]
struct HeaderBlock {
    name: String,
    label_span: std::ops::Range<usize>,
    start: usize,
    end: usize,
}

#[derive(Debug, Clone)]
struct CategorizedItem {
    category: &'static str,
    coarse: CoarseCategory,
}

fn rebuild_contract_body(
    source: &str,
    contract_kind: ContractKind,
    body_items: Vec<&Item<'_>>,
    body_start: usize,
    body_end: usize,
    headers: &[HeaderBlock],
    settings: &CategoryHeadersSettings,
) -> String {
    let header_ranges: Vec<_> = headers
        .iter()
        .map(|header| header.start..header.end)
        .collect();
    let indent = body_indent(source, body_start, body_end, &body_items, headers);

    let mut preamble = Vec::new();
    let mut sections: BTreeMap<&'static str, Vec<String>> = BTreeMap::new();

    for body_item in body_items {
        let chunk = item_chunk(source, body_item, body_start, &header_ranges);
        if let Some(categorized) = categorize_item(source, contract_kind, body_item, settings) {
            sections
                .entry(categorized.category)
                .or_default()
                .push(normalize_chunk(&chunk));
        } else {
            preamble.push(normalize_chunk(&chunk));
        }
    }

    let mut parts = Vec::new();
    if !preamble.is_empty() {
        parts.push(preamble.join("\n\n"));
    }

    for category in CANONICAL_CATEGORIES {
        let Some(chunks) = sections.get(category) else {
            continue;
        };

        if !parts.is_empty() {
            parts.push(String::new());
        }

        parts.push(render_header(&indent, category));
        parts.push(String::new());
        parts.push(chunks.join("\n\n"));
    }

    if parts.is_empty() {
        String::new()
    } else {
        format!("\n{}\n", parts.join("\n"))
    }
}

fn body_indent(
    source: &str,
    body_start: usize,
    body_end: usize,
    body_items: &[&Item<'_>],
    headers: &[HeaderBlock],
) -> String {
    for item in body_items {
        let start = solgrid_ast::span_to_range(item.span).start;
        let line_start = solgrid_ast::natspec::line_start(source, start);
        let line = &source[line_start..solgrid_ast::natspec::line_end(source, start)];
        let indent = line
            .chars()
            .take_while(|ch| ch.is_whitespace())
            .collect::<String>();
        if !indent.is_empty() {
            return indent;
        }
    }

    if let Some(header) = headers.first() {
        let line_start = solgrid_ast::natspec::line_start(source, header.start);
        let line = &source[line_start..solgrid_ast::natspec::line_end(source, header.start)];
        let indent = line
            .chars()
            .take_while(|ch| ch.is_whitespace())
            .collect::<String>();
        if !indent.is_empty() {
            return indent;
        }
    }

    let open_line_start = solgrid_ast::natspec::line_start(source, body_start.saturating_sub(1));
    let open_line = &source
        [open_line_start..solgrid_ast::natspec::line_end(source, body_start.saturating_sub(1))];
    let base_indent = open_line
        .chars()
        .take_while(|ch| ch.is_whitespace())
        .collect::<String>();
    let _ = body_end;
    format!("{base_indent}    ")
}

fn item_chunk(
    source: &str,
    item: &Item<'_>,
    body_start: usize,
    header_ranges: &[std::ops::Range<usize>],
) -> String {
    let span = solgrid_ast::span_to_range(item.span);
    let start = attached_chunk_start(source, span.start, body_start, header_ranges);
    let end = solgrid_ast::natspec::line_end(source, span.end);
    source[start..end].to_string()
}

fn attached_chunk_start(
    source: &str,
    item_start: usize,
    body_start: usize,
    header_ranges: &[std::ops::Range<usize>],
) -> usize {
    let mut start = solgrid_ast::natspec::line_start(source, item_start);

    while start > body_start {
        let Some((previous_start, previous_end)) =
            solgrid_ast::natspec::previous_line_bounds(source, start)
        else {
            break;
        };

        if header_ranges
            .iter()
            .any(|range| range.start <= previous_start && previous_end <= range.end)
        {
            break;
        }

        let line = &source[previous_start..previous_end];
        if line.trim().is_empty() {
            break;
        }

        if is_comment_line(line) {
            start = previous_start;
            continue;
        }

        break;
    }

    start.max(body_start)
}

fn is_comment_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("//")
        || trimmed.starts_with("/*")
        || trimmed.starts_with('*')
        || trimmed.ends_with("*/")
}

fn normalize_chunk(chunk: &str) -> String {
    let lines: Vec<_> = chunk.lines().collect();
    let first = lines
        .iter()
        .position(|line| !line.trim().is_empty())
        .unwrap_or(0);
    let last = lines
        .iter()
        .rposition(|line| !line.trim().is_empty())
        .unwrap_or(first);
    lines[first..=last].join("\n")
}

fn render_header(indent: &str, name: &str) -> String {
    let divider = "/".repeat(72);
    format!("{indent}{divider}\n{indent}// {name}\n{indent}{divider}")
}

fn scan_headers(source: &str, body_start: usize, body_end: usize) -> Vec<HeaderBlock> {
    let mut headers = Vec::new();
    let mut cursor = next_line_start(source, body_start);

    while cursor < body_end {
        let line1_end = solgrid_ast::natspec::line_end(source, cursor);
        let Some(line2_start) = next_line_start_checked(source, line1_end) else {
            break;
        };
        let line2_end = solgrid_ast::natspec::line_end(source, line2_start);
        let Some(line3_start) = next_line_start_checked(source, line2_end) else {
            break;
        };
        let line3_end = solgrid_ast::natspec::line_end(source, line3_start);

        if line3_end > body_end {
            break;
        }

        let line1 = &source[cursor..line1_end];
        let line2 = &source[line2_start..line2_end];
        let line3 = &source[line3_start..line3_end];

        if let Some((name, label_span)) =
            parse_header(cursor, line1, line2_start, line2, line3_start, line3)
        {
            headers.push(HeaderBlock {
                name,
                label_span,
                start: cursor,
                end: line3_end,
            });
            cursor = next_line_start(source, line3_end);
            continue;
        }

        cursor = next_line_start(source, line1_end);
    }

    headers
}

fn parse_header(
    _line1_start: usize,
    line1: &str,
    line2_start: usize,
    line2: &str,
    _line3_start: usize,
    line3: &str,
) -> Option<(String, std::ops::Range<usize>)> {
    let indent = line1
        .chars()
        .take_while(|ch| ch.is_whitespace())
        .collect::<String>();
    let divider = format!("{indent}{}", "/".repeat(72));
    if line1 != divider || line3 != divider {
        return None;
    }

    let label_prefix = format!("{indent}// ");
    if !line2.starts_with(&label_prefix) {
        return None;
    }

    let name = line2[label_prefix.len()..].trim().to_string();
    let label_start = line2_start + label_prefix.len();
    let label_end = line2_start + line2.len();
    Some((name, label_start..label_end))
}

fn next_line_start(source: &str, position: usize) -> usize {
    source[position..]
        .find('\n')
        .map(|offset| position + offset + 1)
        .unwrap_or(source.len())
}

fn next_line_start_checked(source: &str, position: usize) -> Option<usize> {
    let next = next_line_start(source, position);
    (next < source.len()).then_some(next)
}

fn contract_body_bounds(
    source: &str,
    contract_span: std::ops::Range<usize>,
) -> Option<(usize, usize)> {
    let text = &source[contract_span.clone()];
    let open = text.find('{')?;
    let close = text.rfind('}')?;
    Some((contract_span.start + open + 1, contract_span.start + close))
}

fn categorize_item(
    source: &str,
    contract_kind: ContractKind,
    item: &Item<'_>,
    settings: &CategoryHeadersSettings,
) -> Option<CategorizedItem> {
    let (category, coarse) = match &item.kind {
        ItemKind::Struct(_) | ItemKind::Enum(_) | ItemKind::Udvt(_) => {
            ("Types", CoarseCategory::Types)
        }
        ItemKind::Variable(_)
            if is_constant_like(source, item) || is_immutable_like(source, item) =>
        {
            ("Constants & Immutables", CoarseCategory::Constants)
        }
        ItemKind::Variable(_) => ("Storage", CoarseCategory::Storage),
        ItemKind::Event(_) => ("Events", CoarseCategory::Events),
        ItemKind::Error(_) => ("Errors", CoarseCategory::Errors),
        ItemKind::Function(func) if func.kind == FunctionKind::Modifier => {
            ("Modifiers", CoarseCategory::Modifiers)
        }
        ItemKind::Function(func) => {
            let category = function_category(source, contract_kind, func, settings);
            let coarse = if category == "Initialization" {
                CoarseCategory::Initialization
            } else {
                CoarseCategory::Functions
            };
            (category, coarse)
        }
        _ => return None,
    };

    Some(CategorizedItem { category, coarse })
}

fn function_category(
    source: &str,
    contract_kind: ContractKind,
    func: &ItemFunction<'_>,
    settings: &CategoryHeadersSettings,
) -> &'static str {
    if contract_kind == ContractKind::Interface {
        return "Functions";
    }

    if is_initialization_function(source, func, settings) {
        return "Initialization";
    }

    let base = match func.kind {
        FunctionKind::Fallback | FunctionKind::Receive => "Implementation",
        FunctionKind::Modifier => "Modifiers",
        FunctionKind::Constructor | FunctionKind::Function => match func.header.visibility() {
            Some(Visibility::Internal) => "Internal Functions",
            Some(Visibility::Private) => "Private Functions",
            Some(Visibility::External) | Some(Visibility::Public) | None => "Implementation",
        },
    };

    if contract_kind == ContractKind::Library && base == "Internal Functions" {
        "Implementation"
    } else {
        base
    }
}

fn is_initialization_function(
    _source: &str,
    func: &ItemFunction<'_>,
    settings: &CategoryHeadersSettings,
) -> bool {
    let name = if func.kind == FunctionKind::Constructor {
        "constructor".to_string()
    } else {
        match func.header.name {
            Some(name) => name.as_str().to_string(),
            None => String::new(),
        }
    };

    if name.starts_with("_init") || name.starts_with("__init") {
        return true;
    }

    let configured: HashSet<String> = if settings.initialization_functions.is_empty() {
        [
            "constructor",
            "supportsInterface",
            "supportsFeature",
            "initialize",
        ]
        .into_iter()
        .map(ToString::to_string)
        .collect()
    } else {
        settings.initialization_functions.iter().cloned().collect()
    };

    if configured.contains(&name) {
        return true;
    }
    false
}

fn is_constant_like(source: &str, item: &Item<'_>) -> bool {
    let text = solgrid_ast::span_text(source, item.span);
    text.contains(" constant ") || text.contains(" constant;")
}

fn is_immutable_like(source: &str, item: &Item<'_>) -> bool {
    let text = solgrid_ast::span_text(source, item.span);
    text.contains(" immutable ") || text.contains(" immutable;")
}

fn is_known_category(name: &str) -> bool {
    CANONICAL_CATEGORIES.contains(&name)
}
