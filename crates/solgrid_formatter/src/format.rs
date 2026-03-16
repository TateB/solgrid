//! Formatter orchestrator — walks the AST, handles comments and directives,
//! and produces the final FormatChunk IR tree.

use crate::comments::CommentStore;
use crate::directives::{compute_disabled_ranges, is_disabled, parse_directives};
use crate::format_item::{format_item, sort_imports};
use crate::ir::*;
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

    let mut chunks = Vec::new();

    // Collect import items for potential sorting
    let import_indices: Vec<usize> = if config.sort_imports {
        let import_items: Vec<&solar_ast::Item<'_>> = ast
            .items
            .iter()
            .filter(|item| matches!(item.kind, ItemKind::Import(_)))
            .collect();
        sort_imports(&import_items, source)
    } else {
        Vec::new()
    };

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
                &import_indices,
                source,
                config,
                &mut comment_store,
            );
            import_group.clear();
        }

        // Separate items with blank lines (before leading comments so blank line
        // appears before the doc comment block, not between comment and item)
        if !chunks.is_empty() {
            chunks.push(hardline());
            let extra = blank_lines_between(
                if idx > 0 {
                    Some(&ast.items[idx - 1])
                } else {
                    None
                },
                item,
            );
            for _ in 0..extra {
                chunks.push(hardline());
            }
        }

        // Emit leading comments
        let leading = comment_store.take_leading(item_range.start);
        for comment in &leading {
            if !chunks.is_empty() {
                chunks.push(hardline());
            }
            chunks.push(FormatChunk::Comment(comment.kind, comment.content.clone()));
        }

        // Need a hardline between leading comments and the item
        if !leading.is_empty() {
            chunks.push(hardline());
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
            &import_indices,
            source,
            config,
            &mut comment_store,
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
    _sorted_indices: &[usize],
    source: &str,
    config: &FormatConfig,
    comments: &mut crate::comments::CommentStore,
) {
    // Sort imports by their path string
    let mut sorted: Vec<&solar_ast::Item<'_>> =
        import_group.iter().map(|(_, item)| *item).collect();
    sorted.sort_by(|a, b| {
        let a_path = if let ItemKind::Import(imp) = &a.kind {
            solgrid_ast::span_text(source, imp.path.span)
        } else {
            ""
        };
        let b_path = if let ItemKind::Import(imp) = &b.kind {
            solgrid_ast::span_text(source, imp.path.span)
        } else {
            ""
        };
        a_path.cmp(b_path)
    });

    for item in sorted {
        if !chunks.is_empty() {
            chunks.push(hardline());
        }
        chunks.push(format_item(item, source, config, comments));
    }
}

/// Determine how many extra blank lines to insert between two top-level items.
///
/// Per the Solidity style guide, each top-level declaration is surrounded by one
/// blank line on each side. Between two declarations, these stack to produce two
/// blank lines.
fn blank_lines_between(prev: Option<&solar_ast::Item<'_>>, current: &solar_ast::Item<'_>) -> usize {
    let Some(prev) = prev else {
        return 0;
    };

    let prev_cat = top_level_category(prev);
    let curr_cat = top_level_category(current);

    match (prev_cat, curr_cat) {
        // Between two declarations: 2 blank lines (1 below prev + 1 above current)
        (Some(a), Some(b)) if is_declaration(&a) && is_declaration(&b) => 2,
        // Directive -> declaration or different directive types: 1 blank line
        (Some(a), Some(b)) if a != b => 1,
        // Same directive type (e.g. import -> import): no blank line
        _ => 0,
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
