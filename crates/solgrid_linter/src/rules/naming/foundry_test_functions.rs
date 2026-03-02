//! Rule: naming/foundry-test-functions
//!
//! Foundry test functions must follow naming patterns (e.g. testTransfer, test_transfer,
//! testFuzzTransfer, testFailTransfer, testForkTransfer).

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::{FunctionKind, ItemKind};
use solgrid_parser::with_parsed_ast_sequential;

static META: RuleMeta = RuleMeta {
    id: "naming/foundry-test-functions",
    name: "foundry-test-functions",
    category: RuleCategory::Naming,
    default_severity: Severity::Warning,
    description: "Foundry test function name should follow naming conventions",
    fix_availability: FixAvailability::None,
};

pub struct FoundryTestFunctionsRule;

/// Check if a function name starting with "test" follows valid Foundry naming patterns.
///
/// Valid patterns:
/// - Exactly "test" (no suffix)
/// - "test" followed by uppercase letter (e.g. testTransfer)
/// - "test_" prefix (e.g. test_transfer)
/// - "testFuzz" prefix (e.g. testFuzzTransfer, testFuzz_transfer)
/// - "testFail" prefix (e.g. testFailTransfer, testFail_transfer)
/// - "testFork" prefix (e.g. testForkTransfer, testFork_transfer)
fn is_valid_test_name(name: &str) -> bool {
    if !name.starts_with("test") {
        return true; // Not a test function, not our concern
    }

    let suffix = &name[4..];

    // Exactly "test" with no suffix
    if suffix.is_empty() {
        return true;
    }

    // "test_" prefix
    if suffix.starts_with('_') {
        return true;
    }

    // "test" followed by an uppercase letter (e.g. testTransfer)
    if let Some(first_char) = suffix.chars().next() {
        if first_char.is_uppercase() {
            // Check for special prefixes: testFuzz, testFail, testFork
            // These are all valid as-is, and they're already covered by "uppercase after test"
            return true;
        }
    }

    // If we get here, the character after "test" is lowercase and not '_'
    false
}

impl Rule for FoundryTestFunctionsRule {
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
                            if func.kind != FunctionKind::Function {
                                continue;
                            }
                            if let Some(name_ident) = func.header.name {
                                let name = name_ident.as_str();
                                if name.starts_with("test") && !is_valid_test_name(name) {
                                    let range = solgrid_ast::span_to_range(name_ident.span);
                                    diagnostics.push(Diagnostic::new(
                                        META.id,
                                        format!(
                                            "test function `{name}` should follow Foundry naming conventions (e.g. testTransfer, test_transfer, testFuzz*, testFail*, testFork*)"
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
