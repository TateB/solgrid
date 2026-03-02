//! Naming convention rules.

mod const_name_snakecase;
mod contract_name_capwords;
mod func_name_mixedcase;

use crate::registry::RuleRegistry;

pub fn register(registry: &mut RuleRegistry) {
    registry.register(Box::new(contract_name_capwords::ContractNameCapwordsRule));
    registry.register(Box::new(func_name_mixedcase::FuncNameMixedcaseRule));
    registry.register(Box::new(const_name_snakecase::ConstNameSnakecaseRule));
}
