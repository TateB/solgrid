//! Type formatting — converts Solar AST `Type` nodes to FormatChunk IR.

use crate::format_expr::format_expr;
use crate::ir::*;
use solar_ast::*;
use solgrid_config::{FormatConfig, UintType};

/// Format a Solidity type.
pub fn format_type(ty: &Type<'_>, source: &str, config: &FormatConfig) -> FormatChunk {
    match &ty.kind {
        TypeKind::Elementary(elem) => format_elementary(elem, config),
        TypeKind::Custom(path) => {
            let parts: Vec<FormatChunk> = path
                .segments()
                .iter()
                .map(|seg| text(seg.as_str()))
                .collect();
            join(parts, text("."))
        }
        TypeKind::Array(arr) => {
            let base = format_type(&arr.element, source, config);
            match &arr.size {
                Some(size_expr) => concat(vec![
                    base,
                    text("["),
                    format_expr(size_expr, source, config),
                    text("]"),
                ]),
                None => concat(vec![base, text("[]")]),
            }
        }
        TypeKind::Mapping(mapping) => {
            let key = format_type(&mapping.key, source, config);
            let key_name = mapping
                .key_name
                .as_ref()
                .map(|n| concat(vec![space(), text(n.as_str())]))
                .unwrap_or(concat(vec![]));
            let value = format_type(&mapping.value, source, config);
            let value_name = mapping
                .value_name
                .as_ref()
                .map(|n| concat(vec![space(), text(n.as_str())]))
                .unwrap_or(concat(vec![]));
            concat(vec![
                text("mapping("),
                key,
                key_name,
                text(" => "),
                value,
                value_name,
                text(")"),
            ])
        }
        TypeKind::Function(func_ty) => {
            let mut parts = vec![text("function(")];
            let params = format_param_types(&func_ty.parameters, source, config);
            parts.push(params);
            parts.push(text(")"));

            if let Some(vis) = &func_ty.visibility {
                parts.push(space());
                parts.push(text(format_visibility(vis.data)));
            }
            if let Some(sm) = &func_ty.state_mutability {
                parts.push(space());
                parts.push(text(format_state_mutability(sm.data)));
            }
            if let Some(returns) = &func_ty.returns {
                parts.push(text(" returns("));
                parts.push(format_param_types(returns, source, config));
                parts.push(text(")"));
            }
            concat(parts)
        }
    }
}

/// Format an elementary type, applying uint_type config option.
fn format_elementary(elem: &ElementaryType, config: &FormatConfig) -> FormatChunk {
    match elem {
        ElementaryType::Address(payable) => {
            if *payable {
                text("address payable")
            } else {
                text("address")
            }
        }
        ElementaryType::Bool => text("bool"),
        ElementaryType::String => text("string"),
        ElementaryType::Bytes => text("bytes"),
        ElementaryType::Int(size) => {
            let bits = size.bits();
            match config.uint_type {
                UintType::Long => text(format!("int{bits}")),
                UintType::Short if bits == 256 => text("int"),
                _ => text(format!("int{bits}")),
            }
        }
        ElementaryType::UInt(size) => {
            let bits = size.bits();
            match config.uint_type {
                UintType::Long => text(format!("uint{bits}")),
                UintType::Short if bits == 256 => text("uint"),
                _ => text(format!("uint{bits}")),
            }
        }
        ElementaryType::FixedBytes(size) => {
            let n = size.bytes();
            text(format!("bytes{n}"))
        }
        ElementaryType::Fixed(size, frac) => {
            let bits = size.bits();
            let decimals = frac.get();
            text(format!("fixed{bits}x{decimals}"))
        }
        ElementaryType::UFixed(size, frac) => {
            let bits = size.bits();
            let decimals = frac.get();
            text(format!("ufixed{bits}x{decimals}"))
        }
    }
}

/// Format a parameter list's types only (for function type syntax).
fn format_param_types(
    params: &ParameterList<'_>,
    source: &str,
    config: &FormatConfig,
) -> FormatChunk {
    let items: Vec<FormatChunk> = params
        .iter()
        .map(|p| {
            let mut parts = vec![format_type(&p.ty, source, config)];
            if let Some(loc) = &p.data_location {
                parts.push(space());
                parts.push(text(format_data_location(*loc)));
            }
            concat(parts)
        })
        .collect();
    join(items, text(", "))
}

/// Format visibility keyword.
pub fn format_visibility(vis: Visibility) -> &'static str {
    match vis {
        Visibility::Public => "public",
        Visibility::External => "external",
        Visibility::Internal => "internal",
        Visibility::Private => "private",
    }
}

/// Format state mutability keyword.
pub fn format_state_mutability(sm: StateMutability) -> &'static str {
    match sm {
        StateMutability::Pure => "pure",
        StateMutability::View => "view",
        StateMutability::Payable => "payable",
        StateMutability::NonPayable => "",
    }
}

/// Format data location keyword.
pub fn format_data_location(loc: DataLocation) -> &'static str {
    match loc {
        DataLocation::Memory => "memory",
        DataLocation::Storage => "storage",
        DataLocation::Calldata => "calldata",
        DataLocation::Transient => "transient",
    }
}
