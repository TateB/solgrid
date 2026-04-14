//! Rule: naming/func-name-mixedcase
//!
//! Functions must use mixedCase (camelCase).

use crate::context::LintContext;
use crate::rule::Rule;
use regex::Regex;
use serde::Deserialize;
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::{FunctionKind, ItemKind, Visibility};
use solgrid_parser::with_parsed_ast_sequential;

static META: RuleMeta = RuleMeta {
    id: "naming/func-name-mixedcase",
    name: "func-name-mixedcase",
    category: RuleCategory::Naming,
    default_severity: Severity::Warning,
    description: "function name should use mixedCase (camelCase)",
    fix_availability: FixAvailability::None,
};

pub struct FuncNameMixedcaseRule;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct FuncNameMixedcaseSettings {
    allow: Vec<String>,
    allow_regex: Option<String>,
}

fn is_valid_function_name(
    name: &str,
    visibility: Option<Visibility>,
    settings: &FuncNameMixedcaseSettings,
    allow_regex: Option<&Regex>,
) -> bool {
    if settings.allow.iter().any(|allowed| allowed == name) {
        return true;
    }

    if allow_regex.is_some_and(|pattern| pattern.is_match(name)) {
        return true;
    }

    if solgrid_ast::is_camel_case(name) {
        return true;
    }

    matches!(
        visibility,
        Some(Visibility::Internal) | Some(Visibility::Private) | None
    ) && name
        .strip_prefix('_')
        .is_some_and(solgrid_ast::is_camel_case)
}

impl Rule for FuncNameMixedcaseRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let settings: FuncNameMixedcaseSettings = ctx.config.rule_settings(META.id);
        let allow_regex = settings
            .allow_regex
            .as_deref()
            .and_then(|pattern| Regex::new(pattern).ok());
        let filename = ctx.path.to_string_lossy().to_string();
        let result = with_parsed_ast_sequential(ctx.source, &filename, |source_unit| {
            let mut diagnostics = Vec::new();
            for item in source_unit.items.iter() {
                if let ItemKind::Contract(contract) = &item.kind {
                    for body_item in contract.body.iter() {
                        if let ItemKind::Function(func) = &body_item.kind {
                            // Only check regular functions (not constructors, fallback, receive)
                            if func.kind != FunctionKind::Function {
                                continue;
                            }
                            if let Some(name_ident) = func.header.name {
                                let name = name_ident.as_str();
                                if !is_valid_function_name(
                                    name,
                                    func.header.visibility(),
                                    &settings,
                                    allow_regex.as_ref(),
                                ) {
                                    let range = solgrid_ast::span_to_range(name_ident.span);
                                    diagnostics.push(Diagnostic::new(
                                        META.id,
                                        format!(
                                            "function name `{name}` should use mixedCase (camelCase)"
                                        ),
                                        META.default_severity,
                                        range,
                                    ));
                                }
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
