//! Rule: naming/named-parameters-mapping
//!
//! Require named parameters on mapping definitions.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::{ItemKind, TypeKind, VariableDefinition};
use solgrid_parser::with_parsed_ast_sequential;

static META: RuleMeta = RuleMeta {
    id: "naming/named-parameters-mapping",
    name: "named-parameters-mapping",
    category: RuleCategory::Naming,
    default_severity: Severity::Warning,
    description: "mapping key and value parameters should be named",
    fix_availability: FixAvailability::None,
};

pub struct NamedParametersMappingRule;

impl Rule for NamedParametersMappingRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let filename = ctx.path.to_string_lossy().to_string();
        with_parsed_ast_sequential(ctx.source, &filename, |source_unit| {
            let mut diagnostics = Vec::new();

            for item in source_unit.items.iter() {
                match &item.kind {
                    ItemKind::Variable(variable) => {
                        check_mapping_variable(variable, &mut diagnostics);
                    }
                    ItemKind::Contract(contract) => {
                        for body_item in contract.body.iter() {
                            if let ItemKind::Variable(variable) = &body_item.kind {
                                check_mapping_variable(variable, &mut diagnostics);
                            }
                        }
                    }
                    _ => {}
                }
            }

            diagnostics
        })
        .unwrap_or_default()
    }
}

fn check_mapping_variable(variable: &VariableDefinition<'_>, diagnostics: &mut Vec<Diagnostic>) {
    let Some(name) = variable.name else {
        return;
    };
    let TypeKind::Mapping(mapping) = &variable.ty.kind else {
        return;
    };

    if mapping.key_name.is_none() {
        diagnostics.push(Diagnostic::new(
            META.id,
            format!(
                "main key parameter in mapping `{}` is not named",
                name.as_str()
            ),
            META.default_severity,
            solgrid_ast::span_to_range(mapping.key.span),
        ));
    }

    let is_nested = matches!(mapping.value.kind, TypeKind::Mapping(_));
    if !is_nested && mapping.value_name.is_none() {
        diagnostics.push(Diagnostic::new(
            META.id,
            format!(
                "value parameter in mapping `{}` is not named",
                name.as_str()
            ),
            META.default_severity,
            solgrid_ast::span_to_range(mapping.value.span),
        ));
    }
}
