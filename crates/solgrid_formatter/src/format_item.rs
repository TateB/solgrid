//! Item formatting — converts Solar AST top-level items to FormatChunk IR.
//!
//! Handles pragma, imports, contracts, functions, structs, enums, events,
//! errors, UDVTs, using-for, and variable declarations.

use crate::comments::CommentStore;
use crate::format_expr::format_expr;
use crate::format_stmt::{format_block, format_params};
use crate::format_ty::{
    format_data_location, format_state_mutability, format_type, format_visibility,
};
use crate::ir::*;
use solar_ast::*;
use solgrid_ast::{span_text, span_to_range};
use solgrid_config::{ContractBodySpacing, FormatConfig};

/// Format a top-level item.
pub fn format_item(
    item: &Item<'_>,
    source: &str,
    config: &FormatConfig,
    comments: &mut CommentStore,
) -> FormatChunk {
    match &item.kind {
        ItemKind::Pragma(pragma) => format_pragma(pragma, item.span, source),
        ItemKind::Import(import) => format_import(import, source, config),
        ItemKind::Contract(contract) => {
            format_contract(contract, item.span, source, config, comments)
        }
        ItemKind::Function(func) => format_function(func, source, config, comments),
        ItemKind::Variable(var) => format_variable_def(var, source, config),
        ItemKind::Struct(s) => format_struct(s, source, config),
        ItemKind::Enum(e) => format_enum(e),
        ItemKind::Udvt(udvt) => format_udvt(udvt, source, config),
        ItemKind::Event(event) => format_event(event, source, config),
        ItemKind::Error(error) => format_error(error, source, config),
        ItemKind::Using(_) => {
            // Using directives are complex — emit from source for now.
            text(span_text(source, item.span))
        }
    }
}

/// Format a pragma directive.
fn format_pragma(
    _pragma: &PragmaDirective<'_>,
    span: solar_interface::Span,
    source: &str,
) -> FormatChunk {
    // Pragma is tricky since PragmaTokens is complex — use source text.
    text(span_text(source, span))
}

/// Format an import directive.
fn format_import(import: &ImportDirective<'_>, source: &str, config: &FormatConfig) -> FormatChunk {
    let path_str = span_text(source, import.path.span);

    match &import.items {
        ImportItems::Plain(alias) => {
            let mut parts = vec![text("import "), text(path_str)];
            if let Some(alias) = alias {
                parts.push(text(" as "));
                parts.push(text(alias.as_str()));
            }
            parts.push(text(";"));
            concat(parts)
        }
        ImportItems::Aliases(aliases) => {
            if aliases.is_empty() {
                return concat(vec![text("import {} from "), text(path_str), text(";")]);
            }

            let items: Vec<FormatChunk> = aliases
                .iter()
                .map(|(name, alias)| {
                    if let Some(alias) = alias {
                        concat(vec![
                            text(name.as_str()),
                            text(" as "),
                            text(alias.as_str()),
                        ])
                    } else {
                        text(name.as_str())
                    }
                })
                .collect();

            let inner = join(items, concat(vec![text(","), line()]));

            if config.bracket_spacing {
                group(vec![
                    text("import {"),
                    indent(vec![line(), inner]),
                    line(),
                    text("} from "),
                    text(path_str),
                    text(";"),
                ])
            } else {
                group(vec![
                    text("import {"),
                    indent(vec![softline(), inner]),
                    softline(),
                    text("} from "),
                    text(path_str),
                    text(";"),
                ])
            }
        }
        ImportItems::Glob(alias) => concat(vec![
            text("import * as "),
            text(alias.as_str()),
            text(" from "),
            text(path_str),
            text(";"),
        ]),
    }
}

/// Format a contract/interface/library definition.
fn format_contract(
    contract: &ItemContract<'_>,
    _span: solar_interface::Span,
    source: &str,
    config: &FormatConfig,
    comments: &mut CommentStore,
) -> FormatChunk {
    let kind_str = match contract.kind {
        ContractKind::Contract => "contract",
        ContractKind::AbstractContract => "abstract contract",
        ContractKind::Interface => "interface",
        ContractKind::Library => "library",
    };

    let mut header = vec![text(kind_str), space(), text(contract.name.as_str())];

    // Inheritance
    if !contract.bases.is_empty() {
        header.push(text(" is"));
        let bases: Vec<FormatChunk> = contract
            .bases
            .iter()
            .map(|base| {
                let path: Vec<FormatChunk> = base
                    .name
                    .segments()
                    .iter()
                    .map(|seg| text(seg.as_str()))
                    .collect();
                let path_chunk = join(path, text("."));
                if base.arguments.is_empty() {
                    path_chunk
                } else {
                    let args: Vec<FormatChunk> = base
                        .arguments
                        .exprs()
                        .map(|a| format_expr(a, source, config))
                        .collect();
                    concat(vec![
                        path_chunk,
                        text("("),
                        join(args, text(", ")),
                        text(")"),
                    ])
                }
            })
            .collect();

        if config.inheritance_brace_new_line {
            // Group inheritance + brace so if_flat controls brace placement:
            // flat: `contract Foo is Bar, Baz {`
            // broken: `contract Foo is\n    Bar,\n    Baz\n{`
            header.push(group(vec![
                indent(vec![
                    line(),
                    join(bases, concat(vec![text(","), line()])),
                ]),
                if_flat(text(" {"), concat(vec![hardline(), text("{")])),
            ]));
        } else {
            header.push(group(vec![indent(vec![
                line(),
                join(bases, concat(vec![text(","), line()])),
            ])]));
            header.push(text(" {"));
        }
    } else {
        header.push(text(" {"));
    }

    // Body
    if contract.body.is_empty() {
        header.push(text("}"));
        return concat(header);
    }

    let mut body_parts = Vec::new();
    let mut prev_kind: Option<ItemCategory> = None;
    let mut prev_multiline = false;
    let mut prev_item_end: usize = 0;

    for item in contract.body.iter() {
        let item_range = span_to_range(item.span);
        let current_kind = categorize_item(item);
        let current_multiline = is_multiline_item(item);

        // Determine if we need a blank line before this item.
        // Comments between items do NOT count as a whitespace gap.
        let need_blank_line = if prev_kind.is_some() {
            match config.contract_body_spacing {
                ContractBodySpacing::Preserve => {
                    has_blank_line_between(source, prev_item_end, item_range.start)
                }
                ContractBodySpacing::Single => true,
                ContractBodySpacing::Compact => prev_multiline || current_multiline,
            }
        } else {
            false
        };

        // Add blank line before leading comments if needed
        if need_blank_line {
            body_parts.push(hardline());
        }

        // Emit leading comments for this body item
        let leading = comments.take_leading(item_range.start);
        for comment in &leading {
            body_parts.push(hardline());
            body_parts.push(FormatChunk::Comment(comment.kind, comment.content.clone()));
        }

        body_parts.push(hardline());
        body_parts.push(format_item(item, source, config, comments));

        // Trailing comments on the same line
        let trailing = comments.take_trailing(source, item_range.end);
        for comment in &trailing {
            body_parts.push(space());
            body_parts.push(FormatChunk::Comment(comment.kind, comment.content.clone()));
        }

        prev_item_end = item_range.end;
        prev_kind = current_kind;
        prev_multiline = current_multiline;
    }

    let mut result = header;
    result.push(indent(body_parts));
    result.push(hardline());
    result.push(text("}"));
    concat(result)
}

/// Categories of items within a contract for spacing purposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ItemCategory {
    Using,
    StateVar,
    Event,
    Error,
    Struct,
    Enum,
    Modifier,
    Function,
}

/// Whether an item inherently spans multiple lines (and thus needs blank line separation).
fn is_multiline_item(item: &Item<'_>) -> bool {
    match &item.kind {
        ItemKind::Function(f) => f.body.is_some(),
        ItemKind::Struct(s) => !s.fields.is_empty(),
        ItemKind::Enum(e) => !e.variants.is_empty(),
        _ => false,
    }
}

fn categorize_item(item: &Item<'_>) -> Option<ItemCategory> {
    match &item.kind {
        ItemKind::Using(_) => Some(ItemCategory::Using),
        ItemKind::Variable(_) => Some(ItemCategory::StateVar),
        ItemKind::Event(_) => Some(ItemCategory::Event),
        ItemKind::Error(_) => Some(ItemCategory::Error),
        ItemKind::Struct(_) => Some(ItemCategory::Struct),
        ItemKind::Enum(_) => Some(ItemCategory::Enum),
        ItemKind::Function(f) if f.kind == FunctionKind::Modifier => Some(ItemCategory::Modifier),
        ItemKind::Function(_) => Some(ItemCategory::Function),
        _ => None,
    }
}

/// Format a function definition.
fn format_function(
    func: &ItemFunction<'_>,
    source: &str,
    config: &FormatConfig,
    comments: &mut CommentStore,
) -> FormatChunk {
    let kind_str = match func.kind {
        FunctionKind::Constructor => "constructor",
        FunctionKind::Function => "function",
        FunctionKind::Fallback => "fallback",
        FunctionKind::Receive => "receive",
        FunctionKind::Modifier => "modifier",
    };

    let mut header_parts = vec![text(kind_str)];

    if let Some(name) = &func.header.name {
        header_parts.push(space());
        header_parts.push(text(name.as_str()));
    }

    // Parameters
    let params = format_params(&func.header.parameters, source, config);
    header_parts.push(text("("));
    if !func.header.parameters.is_empty() {
        header_parts.push(params);
    }
    header_parts.push(text(")"));

    // Build attributes list
    let mut attrs = Vec::new();

    if let Some(vis) = &func.header.visibility {
        let vis_str = format_visibility(vis.data);
        if !vis_str.is_empty() {
            attrs.push(text(vis_str));
        }
    }

    if let Some(sm) = &func.header.state_mutability {
        let sm_str = format_state_mutability(sm.data);
        if !sm_str.is_empty() {
            attrs.push(text(sm_str));
        }
    }

    if func.header.virtual_.is_some() {
        attrs.push(text("virtual"));
    }

    if let Some(override_) = &func.header.override_ {
        if override_.paths.is_empty() {
            attrs.push(text("override"));
        } else {
            let path_chunks: Vec<FormatChunk> = override_
                .paths
                .iter()
                .map(|p| {
                    let segs: Vec<&str> = p.segments().iter().map(|s| s.as_str()).collect();
                    text(segs.join("."))
                })
                .collect();
            attrs.push(group(vec![
                text("override("),
                indent(vec![
                    softline(),
                    join(path_chunks, concat(vec![text(","), line()])),
                ]),
                softline(),
                text(")"),
            ]));
        }
    }

    // Modifiers
    for modifier in func.header.modifiers.iter() {
        let mod_path: Vec<String> = modifier
            .name
            .segments()
            .iter()
            .map(|s| s.as_str().to_string())
            .collect();
        let mod_name = mod_path.join(".");
        if modifier.arguments.is_empty() {
            attrs.push(text(mod_name));
        } else {
            let args: Vec<FormatChunk> = modifier
                .arguments
                .exprs()
                .map(|a| format_expr(a, source, config))
                .collect();
            attrs.push(concat(vec![
                text(mod_name),
                text("("),
                join(args, text(", ")),
                text(")"),
            ]));
        }
    }

    // Returns
    let mut returns_chunk = None;
    if let Some(returns) = &func.header.returns {
        if !returns.is_empty() {
            let ret_params = format_params(returns, source, config);
            returns_chunk = Some(concat(vec![text("returns ("), ret_params, text(")")]));
        }
    }

    // Combine header based on multiline_func_header config
    let all_attr_chunks: Vec<FormatChunk> = attrs;
    let mut signature = header_parts;

    if !all_attr_chunks.is_empty() || returns_chunk.is_some() {
        // Use soft breaks between attributes with indent so long signatures wrap properly
        let mut attr_parts = Vec::new();
        for attr in &all_attr_chunks {
            attr_parts.push(line());
            attr_parts.push(attr.clone());
        }
        if let Some(ret) = &returns_chunk {
            attr_parts.push(line());
            attr_parts.push(ret.clone());
        }
        signature.push(indent(attr_parts));
    }

    // Body
    match &func.body {
        Some(body) => {
            signature.push(space());
            signature.push(format_block(body, source, config, comments));
            group(signature)
        }
        None => {
            signature.push(text(";"));
            group(signature)
        }
    }
}

/// Format a variable definition (state variable or local).
fn format_variable_def(
    var: &VariableDefinition<'_>,
    source: &str,
    config: &FormatConfig,
) -> FormatChunk {
    let mut parts = vec![format_type(&var.ty, source, config)];

    if let Some(vis) = &var.visibility {
        parts.push(space());
        parts.push(text(format_visibility(*vis)));
    }

    if let Some(mutability) = &var.mutability {
        parts.push(space());
        parts.push(text(mutability.to_str()));
    }

    if let Some(loc) = &var.data_location {
        parts.push(space());
        parts.push(text(format_data_location(*loc)));
    }

    if let Some(override_) = &var.override_ {
        parts.push(space());
        if override_.paths.is_empty() {
            parts.push(text("override"));
        } else {
            let path_chunks: Vec<FormatChunk> = override_
                .paths
                .iter()
                .map(|p| {
                    let segs: Vec<&str> = p.segments().iter().map(|s| s.as_str()).collect();
                    text(segs.join("."))
                })
                .collect();
            parts.push(group(vec![
                text("override("),
                indent(vec![
                    softline(),
                    join(path_chunks, concat(vec![text(","), line()])),
                ]),
                softline(),
                text(")"),
            ]));
        }
    }

    if let Some(name) = &var.name {
        parts.push(space());
        parts.push(text(name.as_str()));
    }

    if let Some(init) = &var.initializer {
        parts.push(text(" = "));
        parts.push(format_expr(init, source, config));
    }

    parts.push(text(";"));
    group(parts)
}

/// Format a struct definition.
fn format_struct(s: &ItemStruct<'_>, source: &str, config: &FormatConfig) -> FormatChunk {
    let mut parts = vec![text("struct "), text(s.name.as_str()), text(" {")];

    if s.fields.is_empty() {
        parts.push(text("}"));
        return concat(parts);
    }

    let mut body = Vec::new();
    for field in s.fields.iter() {
        body.push(hardline());
        let mut field_parts = vec![format_type(&field.ty, source, config)];
        if let Some(name) = &field.name {
            field_parts.push(space());
            field_parts.push(text(name.as_str()));
        }
        field_parts.push(text(";"));
        body.push(concat(field_parts));
    }

    parts.push(indent(body));
    parts.push(hardline());
    parts.push(text("}"));
    concat(parts)
}

/// Format an enum definition.
fn format_enum(e: &ItemEnum<'_>) -> FormatChunk {
    let mut parts = vec![text("enum "), text(e.name.as_str()), text(" {")];

    if e.variants.is_empty() {
        parts.push(text("}"));
        return concat(parts);
    }

    let variants: Vec<FormatChunk> = e.variants.iter().map(|v| text(v.as_str())).collect();

    let body = join(variants, text(","));
    parts.push(indent(vec![hardline(), body]));
    parts.push(hardline());
    parts.push(text("}"));
    concat(parts)
}

/// Format a UDVT (User-Defined Value Type) definition.
fn format_udvt(udvt: &ItemUdvt<'_>, source: &str, config: &FormatConfig) -> FormatChunk {
    concat(vec![
        text("type "),
        text(udvt.name.as_str()),
        text(" is "),
        format_type(&udvt.ty, source, config),
        text(";"),
    ])
}

/// Format an event definition.
fn format_event(event: &ItemEvent<'_>, source: &str, config: &FormatConfig) -> FormatChunk {
    let mut parts = vec![text("event "), text(event.name.as_str()), text("(")];

    if !event.parameters.is_empty() {
        let params: Vec<FormatChunk> = event
            .parameters
            .iter()
            .map(|p| {
                let mut param_parts = vec![format_type(&p.ty, source, config)];
                if p.indexed {
                    param_parts.push(text(" indexed"));
                }
                if let Some(name) = &p.name {
                    param_parts.push(space());
                    param_parts.push(text(name.as_str()));
                }
                concat(param_parts)
            })
            .collect();
        parts.push(join(params, concat(vec![text(","), line()])));
    }

    parts.push(text(")"));
    if event.anonymous {
        parts.push(text(" anonymous"));
    }
    parts.push(text(";"));
    group(parts)
}

/// Format a custom error definition.
fn format_error(error: &ItemError<'_>, source: &str, config: &FormatConfig) -> FormatChunk {
    let mut parts = vec![text("error "), text(error.name.as_str()), text("(")];

    if !error.parameters.is_empty() {
        let params: Vec<FormatChunk> = error
            .parameters
            .iter()
            .map(|p| {
                let mut param_parts = vec![format_type(&p.ty, source, config)];
                if let Some(name) = &p.name {
                    param_parts.push(space());
                    param_parts.push(text(name.as_str()));
                }
                concat(param_parts)
            })
            .collect();
        parts.push(join(params, concat(vec![text(","), line()])));
    }

    parts.push(text(");"));
    group(parts)
}

/// Check if there's a blank line in the source between two byte positions.
/// A blank line is one that contains only whitespace — comments don't count
/// as whitespace, so a comment between items doesn't create or suppress a gap.
fn has_blank_line_between(source: &str, start: usize, end: usize) -> bool {
    if start >= end || end > source.len() {
        return false;
    }
    let between = &source[start..end];
    let mut prev_was_newline = false;
    for ch in between.chars() {
        if ch == '\n' {
            if prev_was_newline {
                return true;
            }
            prev_was_newline = true;
        } else if ch == '\r' || ch == ' ' || ch == '\t' {
            // Whitespace — continue without resetting
        } else {
            prev_was_newline = false;
        }
    }
    false
}

/// Sort import items if sort_imports is enabled.
pub fn sort_imports(items: &[&Item<'_>], source: &str) -> Vec<usize> {
    let mut indexed: Vec<(usize, &str)> = items
        .iter()
        .enumerate()
        .filter_map(|(i, item)| {
            if let ItemKind::Import(import) = &item.kind {
                Some((i, span_text(source, import.path.span)))
            } else {
                None
            }
        })
        .collect();
    indexed.sort_by(|a, b| a.1.cmp(b.1));
    indexed.into_iter().map(|(i, _)| i).collect()
}
