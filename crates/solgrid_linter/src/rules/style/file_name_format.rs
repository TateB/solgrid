//! Rule: style/file-name-format
//!
//! File names must match the primary contract name (PascalCase.sol).

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::{ContractKind, ItemKind};
use solgrid_parser::with_parsed_ast_sequential;

static META: RuleMeta = RuleMeta {
    id: "style/file-name-format",
    name: "file-name-format",
    category: RuleCategory::Style,
    default_severity: Severity::Info,
    description: "file name should match the primary contract name (PascalCase.sol)",
    fix_availability: FixAvailability::None,
};

pub struct FileNameFormatRule;

impl Rule for FileNameFormatRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let file_stem = match ctx.path.file_stem().and_then(|s| s.to_str()) {
            Some(s) => s.to_string(),
            None => return Vec::new(),
        };

        let filename = ctx.path.to_string_lossy().to_string();
        let result = with_parsed_ast_sequential(ctx.source, &filename, |source_unit| {
            let mut diagnostics = Vec::new();

            // Find the primary contract/interface/library (first non-abstract contract, or first interface)
            let mut primary_name: Option<String> = None;
            for item in source_unit.items.iter() {
                if let ItemKind::Contract(contract) = &item.kind {
                    let name = contract.name.as_str();
                    match contract.kind {
                        ContractKind::Contract | ContractKind::Interface | ContractKind::Library => {
                            primary_name = Some(name.to_string());
                            break;
                        }
                        ContractKind::AbstractContract => {
                            // Use abstract contract if no concrete one found
                            if primary_name.is_none() {
                                primary_name = Some(name.to_string());
                            }
                        }
                    }
                }
            }

            if let Some(expected_name) = primary_name {
                if file_stem != expected_name {
                    diagnostics.push(Diagnostic::new(
                        META.id,
                        format!(
                            "file name `{file_stem}.sol` should match the primary contract name `{expected_name}`"
                        ),
                        META.default_severity,
                        0..0, // file-level diagnostic
                    ));
                }
            }

            diagnostics
        });

        result.unwrap_or_default()
    }
}
