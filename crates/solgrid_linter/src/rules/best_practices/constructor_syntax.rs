//! Rule: best-practices/constructor-syntax
//!
//! Use the `constructor` keyword instead of old-style named constructors
//! (functions named the same as the contract).

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::ItemKind;
use solgrid_parser::with_parsed_ast_sequential;

static META: RuleMeta = RuleMeta {
    id: "best-practices/constructor-syntax",
    name: "constructor-syntax",
    category: RuleCategory::BestPractices,
    default_severity: Severity::Warning,
    description: "use `constructor` keyword instead of old-style named constructor",
    fix_availability: FixAvailability::Available(FixSafety::Safe),
};

pub struct ConstructorSyntaxRule;

impl Rule for ConstructorSyntaxRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let filename = ctx.path.to_string_lossy().to_string();
        let result = with_parsed_ast_sequential(ctx.source, &filename, |source_unit| {
            let mut diagnostics = Vec::new();
            for item in source_unit.items.iter() {
                if let ItemKind::Contract(contract) = &item.kind {
                    let contract_name = contract.name.as_str();

                    for body_item in contract.body.iter() {
                        if let ItemKind::Function(func) = &body_item.kind {
                            // Check if a regular function has the same name as the contract
                            if matches!(
                                func.kind,
                                solgrid_parser::solar_ast::FunctionKind::Function
                            ) {
                                if let Some(name) = func.header.name {
                                    if name.as_str() == contract_name {
                                        let range = solgrid_ast::span_to_range(body_item.span);
                                        let func_text =
                                            solgrid_ast::span_text(ctx.source, body_item.span);

                                        let mut diag = Diagnostic::new(
                                            META.id,
                                            format!(
                                                "use `constructor` keyword instead of `function {contract_name}`"
                                            ),
                                            META.default_severity,
                                            range.clone(),
                                        );

                                        // Provide a safe fix: replace `function ContractName` with `constructor`
                                        let old_pattern = format!("function {contract_name}");
                                        if let Some(offset) = func_text.find(&old_pattern) {
                                            let abs_start = range.start + offset;
                                            let abs_end = abs_start + old_pattern.len();
                                            diag = diag.with_fix(Fix::safe(
                                                format!("replace `function {contract_name}` with `constructor`"),
                                                vec![TextEdit::replace(abs_start..abs_end, "constructor")],
                                            ));
                                        }

                                        diagnostics.push(diag);
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
