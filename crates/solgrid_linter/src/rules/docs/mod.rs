//! Documentation rules — NatSpec and documentation completeness.

mod license_identifier;
mod natspec_contract;
mod natspec_error;
mod natspec_event;
mod natspec_function;
mod natspec_interface;
mod natspec_modifier;
mod natspec_param_mismatch;

use crate::registry::RuleRegistry;

pub fn register(registry: &mut RuleRegistry) {
    registry.register(Box::new(natspec_contract::NatspecContractRule));
    registry.register(Box::new(natspec_interface::NatspecInterfaceRule));
    registry.register(Box::new(natspec_function::NatspecFunctionRule));
    registry.register(Box::new(natspec_event::NatspecEventRule));
    registry.register(Box::new(natspec_error::NatspecErrorRule));
    registry.register(Box::new(natspec_modifier::NatspecModifierRule));
    registry.register(Box::new(
        natspec_param_mismatch::NatspecParamMismatchRule,
    ));
    registry.register(Box::new(license_identifier::LicenseIdentifierRule));
}
