//! Formatter orchestrator — walks the AST, handles comments and directives,
//! and produces the final FormatChunk IR tree.

use crate::comments::CommentStore;
use crate::directives::{compute_disabled_ranges, is_disabled, parse_directives};
use crate::format_item::{format_item, has_blank_line_between};
use crate::ir::*;
use regex::Regex;
use solar_ast::{ItemKind, SourceUnit};
use solgrid_ast::span_to_range;
use solgrid_config::FormatConfig;

/// Format a parsed SourceUnit into a FormatChunk IR tree.
pub fn format_source_unit(
    source: &str,
    ast: &SourceUnit<'_>,
    config: &FormatConfig,
) -> FormatChunk {
    let directives = parse_directives(source);
    let disabled_ranges = compute_disabled_ranges(source, &directives);
    let mut comment_store = CommentStore::new(source);
    let import_order = compile_import_order(config);

    let mut chunks = Vec::new();

    // Track groups for sorted imports
    let mut import_group: Vec<(usize, &solar_ast::Item<'_>)> = Vec::new();

    for (idx, item) in ast.items.iter().enumerate() {
        let item_range = span_to_range(item.span);

        // Check if this node is in a disabled region
        if is_disabled(item_range.start, &disabled_ranges) {
            // Emit source text verbatim
            let raw = &source[item_range.start..item_range.end];
            if !chunks.is_empty() {
                chunks.push(hardline());
            }
            chunks.push(text(raw));
            continue;
        }

        // Handle imports with sorting
        if config.sort_imports && matches!(item.kind, ItemKind::Import(_)) {
            // Emit leading comments before collecting into import group
            let leading = comment_store.take_leading(item_range.start);
            for comment in &leading {
                if !chunks.is_empty() {
                    chunks.push(hardline());
                }
                chunks.push(FormatChunk::Comment(comment.kind, comment.content.clone()));
            }
            import_group.push((idx, item));
            continue;
        }

        // Flush any pending sorted imports before non-import items
        if config.sort_imports && !import_group.is_empty() {
            flush_sorted_imports(
                &mut chunks,
                &import_group,
                source,
                config,
                &mut comment_store,
                &import_order,
            );
            import_group.clear();
        }

        // Separate items with blank lines (before leading comments so blank line
        // appears before the doc comment block, not between comment and item)
        let mut extra_blanks = 0;
        if !chunks.is_empty() {
            chunks.push(hardline());
            extra_blanks = blank_lines_between(
                if idx > 0 {
                    Some(&ast.items[idx - 1])
                } else {
                    None
                },
                item,
                source,
                &import_order,
            );
            for _ in 0..extra_blanks {
                chunks.push(hardline());
            }
        }

        // Emit leading comments
        let leading = comment_store.take_leading(item_range.start);
        for (i, comment) in leading.iter().enumerate() {
            // Skip hardline before the first comment when blank_lines_between
            // already inserted separation — otherwise blank lines stack and we
            // get one more blank line than intended.
            if (i > 0 || extra_blanks == 0) && !chunks.is_empty() {
                chunks.push(hardline());
            }
            if i > 0
                && has_blank_line_between(source, leading[i - 1].range.end, comment.range.start)
            {
                chunks.push(hardline());
            }
            chunks.push(FormatChunk::Comment(comment.kind, comment.content.clone()));
        }

        // Need a hardline between leading comments and the item
        if !leading.is_empty() {
            chunks.push(hardline());
            if leading.last().is_some_and(|comment| {
                has_blank_line_between(source, comment.range.end, item_range.start)
            }) {
                chunks.push(hardline());
            }
        }

        chunks.push(format_item(item, source, config, &mut comment_store));

        // Trailing comments
        let trailing = comment_store.take_trailing(source, item_range.end);
        for comment in &trailing {
            chunks.push(space());
            chunks.push(FormatChunk::Comment(comment.kind, comment.content.clone()));
        }
    }

    // Flush remaining sorted imports
    if config.sort_imports && !import_group.is_empty() {
        flush_sorted_imports(
            &mut chunks,
            &import_group,
            source,
            config,
            &mut comment_store,
            &import_order,
        );
    }

    // Emit any remaining comments
    let remaining = comment_store.take_remaining();
    for comment in &remaining {
        chunks.push(hardline());
        chunks.push(FormatChunk::Comment(comment.kind, comment.content.clone()));
    }

    // Ensure trailing newline
    chunks.push(hardline());

    concat(chunks)
}

/// Flush sorted imports into the chunks list.
fn flush_sorted_imports(
    chunks: &mut Vec<FormatChunk>,
    import_group: &[(usize, &solar_ast::Item<'_>)],
    source: &str,
    config: &FormatConfig,
    comments: &mut crate::comments::CommentStore,
    import_order: &[Regex],
) {
    let mut sorted: Vec<&solar_ast::Item<'_>> =
        import_group.iter().map(|(_, item)| *item).collect();
    sorted.sort_by(|a, b| compare_import_items(a, b, source, import_order));

    let mut previous_group = None;
    for item in sorted {
        if !chunks.is_empty() {
            chunks.push(hardline());
        }
        let group = import_item_group(item, source, import_order);
        if previous_group.is_some_and(|prev| prev != group) {
            chunks.push(hardline());
        }
        chunks.push(format_item(item, source, config, comments));
        previous_group = Some(group);
    }
}

/// Determine how many extra blank lines to insert between two top-level items.
///
/// Per the Solidity style guide, each top-level declaration is surrounded by one
/// blank line on each side. Between two declarations, these stack to produce two
/// blank lines.
fn blank_lines_between(
    prev: Option<&solar_ast::Item<'_>>,
    current: &solar_ast::Item<'_>,
    source: &str,
    import_order: &[Regex],
) -> usize {
    let Some(prev) = prev else {
        return 0;
    };

    let prev_cat = top_level_category(prev);
    let curr_cat = top_level_category(current);

    match (prev_cat, curr_cat) {
        (Some(TopLevelCategory::Import), Some(TopLevelCategory::Import)) => usize::from(
            import_item_group(prev, source, import_order)
                != import_item_group(current, source, import_order),
        ),
        // Contracts remain visually separated, but file-scope declarations such
        // as constants and UDVTs stay grouped with a single blank line.
        (Some(TopLevelCategory::Contract), Some(TopLevelCategory::Contract)) => 2,
        (Some(a), Some(b)) if is_declaration(&a) && is_declaration(&b) => 1,
        // Directive -> declaration or different directive types: 1 blank line
        (Some(a), Some(b)) if a != b => 1,
        // Same directive type (e.g. import -> import): no blank line
        _ => 0,
    }
}

fn compile_import_order(config: &FormatConfig) -> Vec<Regex> {
    config
        .import_order
        .iter()
        .filter_map(|pattern| Regex::new(pattern).ok())
        .collect()
}

fn compare_import_items(
    left: &solar_ast::Item<'_>,
    right: &solar_ast::Item<'_>,
    source: &str,
    import_order: &[Regex],
) -> std::cmp::Ordering {
    let left_path = import_path(left, source);
    let right_path = import_path(right, source);

    import_item_group(left, source, import_order)
        .cmp(&import_item_group(right, source, import_order))
        .then_with(|| {
            left_path
                .to_ascii_lowercase()
                .cmp(&right_path.to_ascii_lowercase())
        })
}

fn import_item_group(item: &solar_ast::Item<'_>, source: &str, import_order: &[Regex]) -> usize {
    import_group_index(import_path(item, source), import_order)
}

fn import_group_index(path: &str, import_order: &[Regex]) -> usize {
    if import_order.is_empty() {
        return default_import_group(path);
    }

    import_order
        .iter()
        .position(|pattern| pattern.is_match(path))
        .unwrap_or(import_order.len())
}

fn default_import_group(path: &str) -> usize {
    if path.starts_with("./") {
        2
    } else if path.starts_with("../") || path.starts_with('.') {
        1
    } else {
        0
    }
}

fn import_path<'a>(item: &'a solar_ast::Item<'_>, _source: &'a str) -> &'a str {
    if let ItemKind::Import(import) = &item.kind {
        import.path.value.as_str()
    } else {
        ""
    }
}

fn is_declaration(cat: &TopLevelCategory) -> bool {
    matches!(cat, TopLevelCategory::Contract | TopLevelCategory::Other)
}

#[derive(PartialEq, Eq)]
enum TopLevelCategory {
    Pragma,
    Import,
    Contract,
    Other,
}

fn top_level_category(item: &solar_ast::Item<'_>) -> Option<TopLevelCategory> {
    match &item.kind {
        ItemKind::Pragma(_) => Some(TopLevelCategory::Pragma),
        ItemKind::Import(_) => Some(TopLevelCategory::Import),
        ItemKind::Contract(_) => Some(TopLevelCategory::Contract),
        _ => Some(TopLevelCategory::Other),
    }
}
