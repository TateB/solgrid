//! solgrid linter — rule engine and lint rule implementations.

pub mod context;
pub mod engine;
pub mod fix;
pub mod registry;
pub mod rule;
pub mod rules;
pub mod source_utils;
pub mod suppression;

pub mod testing;

pub use context::LintContext;
pub use engine::LintEngine;
pub use registry::RuleRegistry;
pub use rule::Rule;
