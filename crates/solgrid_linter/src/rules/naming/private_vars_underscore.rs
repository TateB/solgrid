//! Rule: naming/private-vars-underscore
//!
//! Private and internal state variables must start with an underscore.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::{ItemKind, VarMut, Visibility};
use solgrid_parser::with_parsed_ast_sequential;

static META: RuleMeta = RuleMeta {
    id: "naming/private-vars-underscore",
    name: "private-vars-underscore",
    category: RuleCategory::Naming,
    default_severity: Severity::Warning,
    description: "private/internal state variable name should start with underscore",
    fix_availability: FixAvailability::None,
};

pub struct PrivateVarsUnderscoreRule;

impl Rule for PrivateVarsUnderscoreRule {
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
                        if let ItemKind::Variable(var) = &body_item.kind {
                            // Skip constants and immutables (they have their own naming conventions)
                            if var.mutability == Some(VarMut::Constant)
                                || var.mutability == Some(VarMut::Immutable)
                            {
                                continue;
                            }

                            // Check if visibility is private, internal, or default (None = internal)
                            let is_private_or_internal = matches!(
                                var.visibility,
                                Some(Visibility::Private) | Some(Visibility::Internal) | None
                            );

                            if is_private_or_internal {
                                if let Some(name_ident) = var.name {
                                    let name = name_ident.as_str();
                                    if !name.starts_with('_') {
                                        let range = solgrid_ast::span_to_range(name_ident.span);
                                        diagnostics.push(Diagnostic::new(
                                            META.id,
                                            format!(
                                                "private/internal variable `{name}` should start with underscore"
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
            }
            diagnostics
        });

        result.unwrap_or_default()
    }
}
