//! Rule: best-practices/visibility-modifier-order
//!
//! Enforce Solidity style guide order for function modifiers:
//! visibility, mutability, virtual/override, custom modifiers.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::{FunctionKind, ItemKind};
use solgrid_parser::with_parsed_ast_sequential;

static META: RuleMeta = RuleMeta {
    id: "best-practices/visibility-modifier-order",
    name: "visibility-modifier-order",
    category: RuleCategory::BestPractices,
    default_severity: Severity::Warning,
    description: "function modifiers should follow Solidity style guide order",
    fix_availability: FixAvailability::Available(FixSafety::Safe),
};

pub struct VisibilityModifierOrderRule;

/// Modifier category for ordering purposes.
/// Solidity style guide order: visibility, mutability, virtual, override, custom modifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ModCategory {
    Visibility, // public, external, internal, private
    Mutability, // pure, view, payable
    Virtual,    // virtual
    Override,   // override
    Custom,     // user-defined modifiers
}

fn classify_keyword(word: &str) -> ModCategory {
    match word {
        "public" | "external" | "internal" | "private" => ModCategory::Visibility,
        "pure" | "view" | "payable" => ModCategory::Mutability,
        "virtual" => ModCategory::Virtual,
        "override" => ModCategory::Override,
        _ => ModCategory::Custom,
    }
}

impl Rule for VisibilityModifierOrderRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let filename = ctx.path.to_string_lossy().to_string();
        let result = with_parsed_ast_sequential(ctx.source, &filename, |source_unit| {
            let mut diagnostics = Vec::new();
            for item in source_unit.items.iter() {
                if let ItemKind::Contract(contract) = &item.kind {
                    for body_item in contract.body.iter() {
                        if let ItemKind::Function(func) = &body_item.kind {
                            // Skip fallback and receive (they have fixed signatures)
                            if matches!(func.kind, FunctionKind::Fallback | FunctionKind::Receive) {
                                continue;
                            }

                            let func_text = solgrid_ast::span_text(ctx.source, body_item.span);

                            // Extract the modifier area: between the closing ) of params and { or ;
                            let modifier_area = extract_modifier_area(func_text);
                            if modifier_area.is_empty() {
                                continue;
                            }

                            // Parse modifiers into (word, category) pairs, skipping returns clause
                            let modifiers = parse_modifiers(&modifier_area);
                            if modifiers.len() < 2 {
                                continue;
                            }

                            // Check if modifiers are in correct order
                            let mut is_ordered = true;
                            for i in 1..modifiers.len() {
                                if modifiers[i].1 < modifiers[i - 1].1 {
                                    is_ordered = false;
                                    break;
                                }
                            }

                            if !is_ordered {
                                let func_name = func
                                    .header
                                    .name
                                    .map(|n| n.as_str().to_string())
                                    .unwrap_or_else(|| func.kind.to_str().to_string());
                                let range = solgrid_ast::item_name_range(body_item);

                                // Build the correctly ordered modifier list
                                let mut sorted = modifiers.clone();
                                sorted.sort_by_key(|m| m.1);
                                let correct_order: Vec<&str> =
                                    sorted.iter().map(|(word, _)| word.as_str()).collect();

                                // Build fix: replace the modifier area with sorted version
                                let func_range = solgrid_ast::span_to_range(body_item.span);
                                let fix = build_modifier_fix(ctx.source, func_range, &sorted);

                                let mut diag = Diagnostic::new(
                                    META.id,
                                    format!(
                                        "function `{func_name}` modifiers should be ordered: {}",
                                        correct_order.join(", ")
                                    ),
                                    META.default_severity,
                                    range,
                                );

                                if let Some(fix) = fix {
                                    diag = diag.with_fix(fix);
                                }

                                diagnostics.push(diag);
                            }
                        }
                    }
                }
            }
            diagnostics
        });

        result.unwrap_or_default()
    }
}

/// Extract the modifier area from a function declaration.
/// This is the text between the closing `)` of parameters and the `{` or `;`.
fn extract_modifier_area(func_text: &str) -> String {
    // Find the closing paren of the parameter list
    // We need to handle nested parens (e.g., in return types)
    let mut depth = 0i32;
    let mut first_close = None;
    for (i, ch) in func_text.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    first_close = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }

    let start = match first_close {
        Some(pos) => pos + 1,
        None => return String::new(),
    };

    let rest = &func_text[start..];

    // Find the body start `{` or statement end `;`
    let end = rest
        .find('{')
        .or_else(|| rest.find(';'))
        .unwrap_or(rest.len());

    let area = &rest[..end];

    // Remove the `returns (...)` clause if present
    let area = remove_returns_clause(area);

    area.trim().to_string()
}

/// Remove `returns (...)` clause from modifier area.
fn remove_returns_clause(area: &str) -> &str {
    if let Some(pos) = area.find("returns") {
        let before = area[..pos].trim();
        // The returns clause goes to the end of the area
        before
    } else {
        area
    }
}

/// Parse modifier keywords from the modifier area text.
fn parse_modifiers(area: &str) -> Vec<(String, ModCategory)> {
    let mut modifiers = Vec::new();

    // Handle override(...) specially by removing the parens content
    let cleaned = remove_override_params(area);

    for word in cleaned.split_whitespace() {
        // Skip parenthetical parts (e.g., from override(ISomething))
        if word.starts_with('(') || word.ends_with(')') || word.contains('(') {
            continue;
        }

        let category = classify_keyword(word);
        modifiers.push((word.to_string(), category));
    }

    modifiers
}

/// Build a fix that reorders the modifier area in a function declaration.
fn build_modifier_fix(
    source: &str,
    func_range: std::ops::Range<usize>,
    sorted_modifiers: &[(String, ModCategory)],
) -> Option<Fix> {
    let func_text = &source[func_range.clone()];

    // Find the modifier area within the function text
    // The modifier area is between the closing ) of params and { or ;
    let mut depth = 0i32;
    let mut first_close = None;
    for (i, ch) in func_text.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    first_close = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }

    let params_end = first_close? + 1;
    let rest = &func_text[params_end..];

    // Find the body start `{` or statement end `;`
    let body_start = rest.find('{').or_else(|| rest.find(';'))?;

    // Find the returns clause position in rest
    let returns_pos = rest.find("returns");
    let modifier_end = returns_pos.unwrap_or(body_start);

    // The modifier area range in absolute source coordinates
    let area_start = func_range.start + params_end;
    let area_end = func_range.start + params_end + modifier_end;

    let original_area = &source[area_start..area_end];

    // Preserve leading/trailing whitespace from the original area
    let leading_ws = &original_area[..original_area.len() - original_area.trim_start().len()];
    let trailing_ws = &original_area[original_area.trim_end().len()..];

    // Rebuild sorted modifier text, preserving override(...) params
    let original_func_text_after_params = &func_text[params_end..params_end + modifier_end];
    let override_params = extract_override_params(original_func_text_after_params);

    let sorted_text: Vec<String> = sorted_modifiers
        .iter()
        .map(|(word, cat)| {
            if *cat == ModCategory::Override {
                if let Some(ref params) = override_params {
                    return format!("{word}({params})");
                }
            }
            word.clone()
        })
        .collect();

    let replacement = format!("{}{}{}", leading_ws, sorted_text.join(" "), trailing_ws);

    Some(Fix::safe(
        "Reorder function modifiers",
        vec![TextEdit::replace(area_start..area_end, replacement)],
    ))
}

/// Extract the parenthetical content after `override`, if any.
fn extract_override_params(area: &str) -> Option<String> {
    if let Some(pos) = area.find("override") {
        let after = &area[pos + 8..];
        let trimmed = after.trim_start();
        if trimmed.starts_with('(') {
            let mut depth = 0;
            for (i, ch) in trimmed.char_indices() {
                match ch {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            return Some(trimmed[1..i].to_string());
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    None
}

/// Remove parenthetical content after `override` keyword.
fn remove_override_params(area: &str) -> String {
    let mut result = String::new();
    let chars = area.chars().peekable();
    let mut in_override_parens = false;
    let mut depth = 0;

    for ch in chars {
        if in_override_parens {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        in_override_parens = false;
                    }
                }
                _ => {}
            }
            continue;
        }

        // Check if this is the start of override(...)
        if ch == '(' && result.trim_end().ends_with("override") {
            in_override_parens = true;
            depth = 1;
            continue;
        }

        result.push(ch);
    }

    result
}
