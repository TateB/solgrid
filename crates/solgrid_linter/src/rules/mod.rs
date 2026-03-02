//! Lint rule implementations organized by category.

pub mod best_practices;
pub mod naming;
pub mod security;

use crate::registry::RuleRegistry;

/// Register all built-in rules.
pub fn register_all(registry: &mut RuleRegistry) {
    security::register(registry);
    best_practices::register(registry);
    naming::register(registry);
}
