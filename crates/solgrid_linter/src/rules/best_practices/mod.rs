//! Best practices rules.

mod explicit_types;
mod no_console;
mod no_empty_blocks;

use crate::registry::RuleRegistry;

pub fn register(registry: &mut RuleRegistry) {
    registry.register(Box::new(no_console::NoConsoleRule));
    registry.register(Box::new(explicit_types::ExplicitTypesRule));
    registry.register(Box::new(no_empty_blocks::NoEmptyBlocksRule));
}
