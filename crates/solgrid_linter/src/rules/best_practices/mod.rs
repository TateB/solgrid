//! Best practices rules.

mod code_complexity;
mod constructor_syntax;
mod custom_errors;
mod explicit_types;
mod function_max_lines;
mod imports_on_top;
mod max_states_count;
pub(crate) mod natspec_helpers;
mod natspec_params;
mod natspec_returns;
mod no_console;
mod no_empty_blocks;
mod no_floating_pragma;
mod no_global_import;
mod no_unused_error;
mod no_unused_event;
mod no_unused_imports;
mod no_unused_state;
mod no_unused_vars;
mod one_contract_per_file;
mod reason_string;
mod visibility_modifier_order;

use crate::registry::RuleRegistry;

pub fn register(registry: &mut RuleRegistry) {
    registry.register(Box::new(no_console::NoConsoleRule));
    registry.register(Box::new(explicit_types::ExplicitTypesRule));
    registry.register(Box::new(no_empty_blocks::NoEmptyBlocksRule));
    registry.register(Box::new(function_max_lines::FunctionMaxLinesRule));
    registry.register(Box::new(max_states_count::MaxStatesCountRule));
    registry.register(Box::new(one_contract_per_file::OneContractPerFileRule));
    registry.register(Box::new(no_global_import::NoGlobalImportRule));
    registry.register(Box::new(reason_string::ReasonStringRule));
    registry.register(Box::new(custom_errors::CustomErrorsRule));
    registry.register(Box::new(no_floating_pragma::NoFloatingPragmaRule));
    registry.register(Box::new(imports_on_top::ImportsOnTopRule));
    registry.register(Box::new(code_complexity::CodeComplexityRule));
    registry.register(Box::new(no_unused_error::NoUnusedErrorRule));
    registry.register(Box::new(no_unused_event::NoUnusedEventRule));
    registry.register(Box::new(constructor_syntax::ConstructorSyntaxRule));
    registry.register(Box::new(natspec_params::NatspecParamsRule));
    registry.register(Box::new(natspec_returns::NatspecReturnsRule));
    registry.register(Box::new(
        visibility_modifier_order::VisibilityModifierOrderRule,
    ));
    registry.register(Box::new(no_unused_imports::NoUnusedImportsRule));
    registry.register(Box::new(no_unused_state::NoUnusedStateRule));
    registry.register(Box::new(no_unused_vars::NoUnusedVarsRule));
}
