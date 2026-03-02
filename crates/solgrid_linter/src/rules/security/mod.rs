//! Security rules.

mod avoid_sha3;
mod avoid_suicide;
mod low_level_calls;
mod no_inline_assembly;
mod state_visibility;
mod tx_origin;

use crate::registry::RuleRegistry;

pub fn register(registry: &mut RuleRegistry) {
    registry.register(Box::new(tx_origin::TxOriginRule));
    registry.register(Box::new(avoid_sha3::AvoidSha3Rule));
    registry.register(Box::new(avoid_suicide::AvoidSuicideRule));
    registry.register(Box::new(state_visibility::StateVisibilityRule));
    registry.register(Box::new(no_inline_assembly::NoInlineAssemblyRule));
    registry.register(Box::new(low_level_calls::LowLevelCallsRule));
}
