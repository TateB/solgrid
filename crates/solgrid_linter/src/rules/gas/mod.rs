//! Gas optimization rules.

mod bool_storage;
mod cache_array_length;
mod calldata_parameters;
mod custom_errors;
mod increment_by_one;
mod indexed_events;
mod named_return_values;
mod no_redundant_sload;
mod small_strings;
mod struct_packing;
mod tight_variable_packing;
mod unchecked_increment;
mod use_bytes32;
mod use_constant;
mod use_immutable;

use crate::registry::RuleRegistry;

pub fn register(registry: &mut RuleRegistry) {
    registry.register(Box::new(calldata_parameters::CalldataParametersRule));
    registry.register(Box::new(custom_errors::GasCustomErrorsRule));
    registry.register(Box::new(increment_by_one::IncrementByOneRule));
    registry.register(Box::new(indexed_events::IndexedEventsRule));
    registry.register(Box::new(named_return_values::NamedReturnValuesRule));
    registry.register(Box::new(small_strings::SmallStringsRule));
    registry.register(Box::new(struct_packing::StructPackingRule));
    registry.register(Box::new(cache_array_length::CacheArrayLengthRule));
    registry.register(Box::new(use_immutable::UseImmutableRule));
    registry.register(Box::new(use_constant::UseConstantRule));
    registry.register(Box::new(unchecked_increment::UncheckedIncrementRule));
    registry.register(Box::new(no_redundant_sload::NoRedundantSloadRule));
    registry.register(Box::new(bool_storage::BoolStorageRule));
    registry.register(Box::new(tight_variable_packing::TightVariablePackingRule));
    registry.register(Box::new(use_bytes32::UseBytes32Rule));
}

/// Get the byte size for a Solidity elementary type.
/// Returns None for dynamic types (string, bytes, arrays, mappings).
pub fn type_byte_size(type_name: &str) -> Option<usize> {
    let t = type_name.trim();
    match t {
        "bool" => Some(1),
        "address" | "address payable" => Some(20),
        "bytes1" | "byte" => Some(1),
        "bytes2" => Some(2),
        "bytes3" => Some(3),
        "bytes4" => Some(4),
        "bytes5" => Some(5),
        "bytes6" => Some(6),
        "bytes7" => Some(7),
        "bytes8" => Some(8),
        "bytes9" => Some(9),
        "bytes10" => Some(10),
        "bytes11" => Some(11),
        "bytes12" => Some(12),
        "bytes13" => Some(13),
        "bytes14" => Some(14),
        "bytes15" => Some(15),
        "bytes16" => Some(16),
        "bytes17" => Some(17),
        "bytes18" => Some(18),
        "bytes19" => Some(19),
        "bytes20" => Some(20),
        "bytes21" => Some(21),
        "bytes22" => Some(22),
        "bytes23" => Some(23),
        "bytes24" => Some(24),
        "bytes25" => Some(25),
        "bytes26" => Some(26),
        "bytes27" => Some(27),
        "bytes28" => Some(28),
        "bytes29" => Some(29),
        "bytes30" => Some(30),
        "bytes31" => Some(31),
        "bytes32" => Some(32),
        _ if t.starts_with("uint") => {
            let bits: usize = t[4..].parse().unwrap_or(256);
            Some(bits / 8)
        }
        _ if t.starts_with("int") && !t.starts_with("interface") => {
            let bits: usize = t[3..].parse().unwrap_or(256);
            Some(bits / 8)
        }
        // Enums default to uint8 size
        _ if t.starts_with("enum ") => Some(1),
        _ => None, // dynamic types: string, bytes, arrays, mappings, structs
    }
}
