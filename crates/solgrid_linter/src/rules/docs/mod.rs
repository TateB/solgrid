//! Documentation rules — NatSpec and documentation completeness.

mod license_identifier;
mod natspec;
mod natspec_modifier;
mod selector_tags;

use crate::registry::RuleRegistry;

pub fn register(registry: &mut RuleRegistry) {
    registry.register(Box::new(natspec::NatspecRule));
    registry.register(Box::new(selector_tags::SelectorTagsRule));
    registry.register(Box::new(natspec_modifier::NatspecModifierRule));
    registry.register(Box::new(license_identifier::LicenseIdentifierRule));
}
