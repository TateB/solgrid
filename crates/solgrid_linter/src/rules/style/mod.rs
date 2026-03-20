//! Style rules — code layout and consistency.

mod contract_layout;
mod eol_last;
mod file_name_format;
mod func_order;
mod imports_ordering;
mod max_line_length;
mod no_multiple_empty_lines;
mod no_trailing_whitespace;
mod ordering;
mod prefer_remappings;

use crate::registry::RuleRegistry;

pub fn register(registry: &mut RuleRegistry) {
    registry.register(Box::new(func_order::FuncOrderRule));
    registry.register(Box::new(ordering::OrderingRule));
    registry.register(Box::new(imports_ordering::ImportsOrderingRule));
    registry.register(Box::new(max_line_length::MaxLineLengthRule));
    registry.register(Box::new(no_trailing_whitespace::NoTrailingWhitespaceRule));
    registry.register(Box::new(eol_last::EolLastRule));
    registry.register(Box::new(no_multiple_empty_lines::NoMultipleEmptyLinesRule));
    registry.register(Box::new(contract_layout::ContractLayoutRule));
    registry.register(Box::new(prefer_remappings::PreferRemappingsRule));
    registry.register(Box::new(file_name_format::FileNameFormatRule));
}
