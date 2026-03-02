//! Security rules.

mod arbitrary_send_eth;
mod avoid_selfdestruct;
mod avoid_sha3;
mod avoid_suicide;
mod compiler_version;
mod divide_before_multiply;
mod low_level_calls;
mod msg_value_in_loop;
mod multiple_sends;
mod no_delegatecall_in_loop;
mod no_inline_assembly;
mod not_rely_on_block_hash;
mod not_rely_on_time;
mod payable_fallback;
mod state_visibility;
mod tx_origin;
mod unchecked_transfer;

use crate::registry::RuleRegistry;

pub fn register(registry: &mut RuleRegistry) {
    registry.register(Box::new(tx_origin::TxOriginRule));
    registry.register(Box::new(avoid_sha3::AvoidSha3Rule));
    registry.register(Box::new(avoid_suicide::AvoidSuicideRule));
    registry.register(Box::new(avoid_selfdestruct::AvoidSelfdestructRule));
    registry.register(Box::new(compiler_version::CompilerVersionRule));
    registry.register(Box::new(state_visibility::StateVisibilityRule));
    registry.register(Box::new(no_inline_assembly::NoInlineAssemblyRule));
    registry.register(Box::new(low_level_calls::LowLevelCallsRule));
    registry.register(Box::new(no_delegatecall_in_loop::NoDelegatecallInLoopRule));
    registry.register(Box::new(unchecked_transfer::UncheckedTransferRule));
    registry.register(Box::new(msg_value_in_loop::MsgValueInLoopRule));
    registry.register(Box::new(arbitrary_send_eth::ArbitrarySendEthRule));
    registry.register(Box::new(divide_before_multiply::DivideBeforeMultiplyRule));
    registry.register(Box::new(not_rely_on_block_hash::NotRelyOnBlockHashRule));
    registry.register(Box::new(not_rely_on_time::NotRelyOnTimeRule));
    registry.register(Box::new(multiple_sends::MultipleSendsRule));
    registry.register(Box::new(payable_fallback::PayableFallbackRule));
}
