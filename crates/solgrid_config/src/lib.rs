//! Configuration parsing for solgrid.
//!
//! Handles `solgrid.toml` config files with support for hierarchical
//! config resolution and foundry.toml fallback.

use serde::{Deserialize, Serialize};
use solgrid_diagnostics::{RuleCategory, Severity};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Top-level solgrid configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub lint: LintConfig,
    pub format: FormatConfig,
    pub global: GlobalConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            lint: LintConfig::default(),
            format: FormatConfig::default(),
            global: GlobalConfig::default(),
        }
    }
}

/// Rule severity level in configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleLevel {
    Error,
    Warn,
    Info,
    Off,
}

impl From<RuleLevel> for Option<Severity> {
    fn from(level: RuleLevel) -> Self {
        match level {
            RuleLevel::Error => Some(Severity::Error),
            RuleLevel::Warn => Some(Severity::Warning),
            RuleLevel::Info => Some(Severity::Info),
            RuleLevel::Off => None,
        }
    }
}

/// Rule preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RulePreset {
    /// All rules enabled at their default severity.
    All,
    /// Recommended rules only (default).
    Recommended,
    /// Security rules only.
    SecurityOnly,
}

impl Default for RulePreset {
    fn default() -> Self {
        Self::Recommended
    }
}

/// Lint configuration section.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LintConfig {
    /// Rule preset.
    pub preset: RulePreset,
    /// Per-rule severity overrides.
    #[serde(default)]
    pub rules: HashMap<String, RuleLevel>,
    /// Per-rule settings.
    #[serde(default)]
    pub settings: HashMap<String, toml::Value>,
}

impl Default for LintConfig {
    fn default() -> Self {
        Self {
            preset: RulePreset::Recommended,
            rules: HashMap::new(),
            settings: HashMap::new(),
        }
    }
}

impl LintConfig {
    /// Get the configured severity for a rule, or None if the rule is disabled.
    pub fn rule_severity(&self, rule_id: &str, category: RuleCategory) -> Option<Severity> {
        if let Some(level) = self.rules.get(rule_id) {
            return (*level).into();
        }
        // Use default severity from category
        Some(category.default_severity())
    }

    /// Check if a specific rule is enabled.
    pub fn is_rule_enabled(&self, rule_id: &str, _category: RuleCategory) -> bool {
        if let Some(level) = self.rules.get(rule_id) {
            return *level != RuleLevel::Off;
        }
        // Enabled by default
        true
    }
}

/// Formatter configuration section.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct FormatConfig {
    pub line_length: usize,
    pub tab_width: usize,
    pub use_tabs: bool,
    pub single_quote: bool,
    pub bracket_spacing: bool,
    pub number_underscore: NumberUnderscore,
    pub uint_type: UintType,
    pub override_spacing: bool,
    pub wrap_comments: bool,
    pub sort_imports: bool,
    pub multiline_func_header: MultilineFuncHeader,
    pub contract_new_lines: bool,
}

impl Default for FormatConfig {
    fn default() -> Self {
        Self {
            line_length: 120,
            tab_width: 4,
            use_tabs: false,
            single_quote: false,
            bracket_spacing: false,
            number_underscore: NumberUnderscore::Preserve,
            uint_type: UintType::Long,
            override_spacing: true,
            wrap_comments: false,
            sort_imports: false,
            multiline_func_header: MultilineFuncHeader::AttributesFirst,
            contract_new_lines: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NumberUnderscore {
    Preserve,
    Thousands,
    Remove,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UintType {
    /// Use `uint256` (long form).
    Long,
    /// Use `uint` (short form).
    Short,
    /// Don't change.
    Preserve,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MultilineFuncHeader {
    AttributesFirst,
    ParamsFirst,
    All,
}

/// Global configuration section.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GlobalConfig {
    pub solidity_version: Option<String>,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub respect_gitignore: bool,
    pub threads: usize,
    pub cache_dir: String,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            solidity_version: None,
            include: vec![
                "src/**/*.sol".into(),
                "test/**/*.sol".into(),
                "script/**/*.sol".into(),
            ],
            exclude: vec![
                "lib/**".into(),
                "node_modules/**".into(),
                "out/**".into(),
            ],
            respect_gitignore: true,
            threads: 0,
            cache_dir: ".solgrid_cache".into(),
        }
    }
}

/// Load configuration from a TOML file.
pub fn load_config(path: &Path) -> Result<Config, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    toml::from_str(&content)
        .map_err(|e| format!("failed to parse {}: {e}", path.display()))
}

/// Discover and load config by walking up the filesystem from `start_dir`.
/// Returns default config if no config file is found.
pub fn resolve_config(start_dir: &Path) -> Config {
    if let Some(path) = find_config_file(start_dir) {
        match load_config(&path) {
            Ok(config) => return config,
            Err(e) => {
                eprintln!("warning: {e}, using defaults");
            }
        }
    }
    Config::default()
}

/// Find the nearest `solgrid.toml` by walking up from `start_dir`.
pub fn find_config_file(start_dir: &Path) -> Option<PathBuf> {
    let mut current = start_dir.to_path_buf();
    loop {
        let config_path = current.join("solgrid.toml");
        if config_path.exists() {
            return Some(config_path);
        }
        if !current.pop() {
            break;
        }
    }
    None
}
