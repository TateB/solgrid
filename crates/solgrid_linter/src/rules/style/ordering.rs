//! Rule: style/ordering
//!
//! Enforce canonical declaration ordering at the file level and inside
//! contract-like bodies.

use super::initialization;
use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::{
    ContractKind, FunctionKind, Item, ItemFunction, ItemKind, StateMutability, Visibility,
};
use solgrid_parser::with_parsed_ast_sequential;

static META: RuleMeta = RuleMeta {
    id: "style/ordering",
    name: "ordering",
    category: RuleCategory::Style,
    default_severity: Severity::Info,
    description: "declarations should follow the canonical file-level and contract-level ordering",
    fix_availability: FixAvailability::None,
};

pub struct OrderingRule;

struct WeightedItem {
    weight: u16,
    span: std::ops::Range<usize>,
    label: &'static str,
}

impl Rule for OrderingRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let filename = ctx.path.to_string_lossy().to_string();
        let initialization_functions =
            initialization::configured_initialization_functions(ctx.config);
        with_parsed_ast_sequential(ctx.source, &filename, |source_unit| {
            let mut diagnostics = Vec::new();

            if let Some(diag) = first_scope_violation(
                ctx,
                source_unit
                    .items
                    .iter()
                    .filter_map(|item| file_item_weight(ctx.source, item)),
            ) {
                diagnostics.push(diag);
            }

            for item in source_unit.items.iter() {
                let ItemKind::Contract(contract) = &item.kind else {
                    continue;
                };

                if let Some(diag) = first_scope_violation(
                    ctx,
                    contract.body.iter().filter_map(|body_item| {
                        contract_item_weight(
                            ctx.source,
                            contract.kind,
                            body_item,
                            &initialization_functions,
                        )
                    }),
                ) {
                    diagnostics.push(diag);
                }
            }

            diagnostics
        })
        .unwrap_or_default()
    }
}

fn first_scope_violation<I>(ctx: &LintContext<'_>, items: I) -> Option<Diagnostic>
where
    I: IntoIterator<Item = WeightedItem>,
{
    let mut max_weight = None;
    let mut previous = None;

    for item in items {
        match max_weight {
            Some(weight) if item.weight < weight => {
                let (prev_label, prev_line) = previous?;
                return Some(Diagnostic::new(
                    META.id,
                    format!(
                        "{} order is incorrect, {} can not go after {} (line {})",
                        item.label, item.label, prev_label, prev_line
                    ),
                    META.default_severity,
                    item.span,
                ));
            }
            _ => {
                max_weight = Some(item.weight);
                previous = Some((item.label, ctx.line_number(item.span.start)));
            }
        }
    }

    None
}

fn file_item_weight(source: &str, item: &Item<'_>) -> Option<WeightedItem> {
    let (weight, label) = match &item.kind {
        ItemKind::Pragma(_) => (0, "Pragma directive"),
        ItemKind::Import(_) => (10, "Import directive"),
        ItemKind::Variable(_) if is_constant_like(source, item) => (20, "File-level constant"),
        ItemKind::Enum(_) => (30, "Enum definition"),
        ItemKind::Struct(_) => (35, "Struct definition"),
        ItemKind::Error(_) => (40, "Custom error"),
        ItemKind::Function(_) => (50, "Free function definition"),
        ItemKind::Contract(contract) => match contract.kind {
            ContractKind::Interface => (60, "Interface"),
            ContractKind::Library => (70, "Library"),
            ContractKind::Contract | ContractKind::AbstractContract => (80, "Contract"),
        },
        _ => return None,
    };

    Some(WeightedItem {
        weight,
        span: solgrid_ast::item_name_range(item),
        label,
    })
}

fn contract_item_weight(
    source: &str,
    contract_kind: ContractKind,
    item: &Item<'_>,
    initialization_functions: &[String],
) -> Option<WeightedItem> {
    let (weight, label) = match &item.kind {
        ItemKind::Using(_) => (0, "Using declaration"),
        ItemKind::Enum(_) => (10, "Enum definition"),
        ItemKind::Struct(_) => (15, "Struct definition"),
        ItemKind::Variable(_) if is_constant_like(source, item) => (20, "Constant state variable"),
        ItemKind::Variable(_) if is_immutable_like(source, item) => {
            (22, "Immutable state variable")
        }
        ItemKind::Variable(_) => (25, "State variable"),
        ItemKind::Event(_) => (30, "Event definition"),
        ItemKind::Error(_) => (35, "Custom error"),
        ItemKind::Function(func) if func.kind == FunctionKind::Modifier => {
            (40, "Modifier definition")
        }
        ItemKind::Function(func)
            if initialization::is_initialization_function(func, initialization_functions) =>
        {
            (50, "Initialization function")
        }
        ItemKind::Function(func) => function_weight(contract_kind, func)?,
        _ => return None,
    };

    Some(WeightedItem {
        weight,
        span: solgrid_ast::item_name_range(item),
        label,
    })
}

fn function_weight(
    _contract_kind: ContractKind,
    func: &ItemFunction<'_>,
) -> Option<(u16, &'static str)> {
    match func.kind {
        FunctionKind::Constructor => Some((50, "Constructor")),
        FunctionKind::Receive => Some((60, "Receive function")),
        FunctionKind::Fallback => Some((70, "Fallback function")),
        FunctionKind::Modifier => None,
        FunctionKind::Function => {
            let base = match func.header.visibility() {
                Some(Visibility::External) => 80,
                Some(Visibility::Public) => 90,
                Some(Visibility::Internal) => 100,
                Some(Visibility::Private) => 110,
                None => 90,
            };
            let offset = match func.header.state_mutability() {
                StateMutability::View => 2,
                StateMutability::Pure => 4,
                StateMutability::Payable | StateMutability::NonPayable => 0,
            };

            Some((base + offset, "Function"))
        }
    }
}

fn is_constant_like(source: &str, item: &Item<'_>) -> bool {
    solgrid_ast::span_text(source, item.span).contains(" constant ")
        || solgrid_ast::span_text(source, item.span).contains(" constant;")
}

fn is_immutable_like(source: &str, item: &Item<'_>) -> bool {
    solgrid_ast::span_text(source, item.span).contains(" immutable ")
        || solgrid_ast::span_text(source, item.span).contains(" immutable;")
}
