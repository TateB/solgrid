//! Rule registry — manages all available lint rules.

use crate::rule::Rule;
use crate::rules;
use solgrid_config::canonical_rule_id;
use solgrid_config::Config;
use solgrid_diagnostics::RuleMeta;
use std::collections::{HashMap, HashSet};

/// Registry of all available lint rules.
pub struct RuleRegistry {
    rules: Vec<Box<dyn Rule>>,
    index: HashMap<String, usize>,
}

impl RuleRegistry {
    /// Create a new registry with all built-in rules.
    pub fn new() -> Self {
        let mut registry = Self {
            rules: Vec::new(),
            index: HashMap::new(),
        };
        rules::register_all(&mut registry);
        registry
    }

    /// Register a rule.
    pub fn register(&mut self, rule: Box<dyn Rule>) {
        let id = rule.meta().id.to_string();
        let idx = self.rules.len();
        self.rules.push(rule);
        self.index.insert(id, idx);
    }

    /// Get all rules.
    pub fn rules(&self) -> &[Box<dyn Rule>] {
        &self.rules
    }

    /// Get all rule metadata.
    pub fn all_meta(&self) -> Vec<&RuleMeta> {
        self.rules.iter().map(|r| r.meta()).collect()
    }

    /// Get a rule by ID.
    pub fn get(&self, id: &str) -> Option<&dyn Rule> {
        self.index
            .get(canonical_rule_id(id))
            .map(|&idx| self.rules[idx].as_ref())
    }

    /// Get enabled rules based on config.
    pub fn enabled_rules(&self, config: &Config) -> Vec<&dyn Rule> {
        let enabled_ids: HashSet<&'static str> = self
            .rules
            .iter()
            .filter(|rule| {
                let meta = rule.meta();
                config.lint.is_rule_enabled(meta.id, meta.category)
            })
            .map(|rule| rule.meta().id)
            .collect();

        self.rules
            .iter()
            .filter(|rule| {
                let meta = rule.meta();
                config.lint.is_rule_enabled(meta.id, meta.category)
                    && meta
                        .suppressed_by()
                        .iter()
                        .all(|id| !enabled_ids.contains(canonical_rule_id(id)))
            })
            .map(|r| r.as_ref())
            .collect()
    }

    /// Number of registered rules.
    pub fn len(&self) -> usize {
        self.rules.len()
    }

    /// Returns `true` if no rules are registered.
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }
}

impl Default for RuleRegistry {
    fn default() -> Self {
        Self::new()
    }
}
