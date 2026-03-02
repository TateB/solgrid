//! Lint rule implementations organized by category.

pub mod best_practices;
pub mod docs;
pub mod gas;
pub mod naming;
pub mod security;
pub mod style;

use crate::registry::RuleRegistry;

/// Register all built-in rules.
pub fn register_all(registry: &mut RuleRegistry) {
    security::register(registry);
    best_practices::register(registry);
    naming::register(registry);
    gas::register(registry);
    style::register(registry);
    docs::register(registry);
}
