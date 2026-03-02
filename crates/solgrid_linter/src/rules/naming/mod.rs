//! Naming convention rules.

mod const_name_snakecase;
mod contract_name_capwords;
mod enum_name_capwords;
mod error_name_capwords;
mod event_name_capwords;
mod foundry_test_functions;
mod func_name_mixedcase;
mod immutable_name_snakecase;
mod interface_starts_with_i;
mod library_name_capwords;
mod modifier_name_mixedcase;
mod param_name_mixedcase;
mod private_vars_underscore;
mod struct_name_capwords;
mod type_name_capwords;
mod var_name_mixedcase;

use crate::registry::RuleRegistry;

pub fn register(registry: &mut RuleRegistry) {
    registry.register(Box::new(contract_name_capwords::ContractNameCapwordsRule));
    registry.register(Box::new(func_name_mixedcase::FuncNameMixedcaseRule));
    registry.register(Box::new(const_name_snakecase::ConstNameSnakecaseRule));
    registry.register(Box::new(interface_starts_with_i::InterfaceStartsWithIRule));
    registry.register(Box::new(library_name_capwords::LibraryNameCapwordsRule));
    registry.register(Box::new(struct_name_capwords::StructNameCapwordsRule));
    registry.register(Box::new(enum_name_capwords::EnumNameCapwordsRule));
    registry.register(Box::new(event_name_capwords::EventNameCapwordsRule));
    registry.register(Box::new(error_name_capwords::ErrorNameCapwordsRule));
    registry.register(Box::new(param_name_mixedcase::ParamNameMixedcaseRule));
    registry.register(Box::new(var_name_mixedcase::VarNameMixedcaseRule));
    registry.register(Box::new(immutable_name_snakecase::ImmutableNameSnakecaseRule));
    registry.register(Box::new(private_vars_underscore::PrivateVarsUnderscoreRule));
    registry.register(Box::new(modifier_name_mixedcase::ModifierNameMixedcaseRule));
    registry.register(Box::new(type_name_capwords::TypeNameCapwordsRule));
    registry.register(Box::new(foundry_test_functions::FoundryTestFunctionsRule));
}
