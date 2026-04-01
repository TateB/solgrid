//! Configuration parsing for solgrid.
//!
//! Handles `solgrid.toml` config files with support for hierarchical
//! config resolution and foundry.toml fallback.

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use solgrid_diagnostics::{RuleCategory, Severity};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Top-level solgrid configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub lint: LintConfig,
    pub format: FormatConfig,
    pub global: GlobalConfig,
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
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RulePreset {
    /// All rules enabled at their default severity.
    All,
    /// Recommended rules only (default).
    #[default]
    Recommended,
    /// Security rules only.
    SecurityOnly,
}

/// Canonicalize deprecated or aliased rule IDs.
pub fn canonical_rule_id(rule_id: &str) -> &str {
    match rule_id {
        "best-practices/use-natspec"
        | "best-practices/natspec-params"
        | "best-practices/natspec-returns"
        | "docs/natspec-contract"
        | "docs/natspec-error"
        | "docs/natspec-event"
        | "docs/natspec-function"
        | "docs/natspec-interface"
        | "docs/natspec-param-mismatch" => "docs/natspec",
        _ => rule_id,
    }
}

fn aliased_rule_ids(rule_id: &str) -> &'static [&'static str] {
    match rule_id {
        "docs/natspec" => &[
            "best-practices/use-natspec",
            "best-practices/natspec-params",
            "best-practices/natspec-returns",
            "docs/natspec-contract",
            "docs/natspec-error",
            "docs/natspec-event",
            "docs/natspec-function",
            "docs/natspec-interface",
            "docs/natspec-param-mismatch",
        ],
        _ => &[],
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
    fn rule_level(&self, rule_id: &str) -> Option<RuleLevel> {
        let canonical = canonical_rule_id(rule_id);
        if let Some(level) = self.rules.get(canonical) {
            return Some(*level);
        }
        if canonical != rule_id {
            if let Some(level) = self.rules.get(rule_id) {
                return Some(*level);
            }
        }
        aliased_rule_ids(canonical)
            .iter()
            .find_map(|alias| self.rules.get(*alias).copied())
    }

    fn setting_value(&self, rule_id: &str) -> Option<&toml::Value> {
        let canonical = canonical_rule_id(rule_id);
        self.settings
            .get(canonical)
            .or_else(|| {
                if canonical != rule_id {
                    self.settings.get(rule_id)
                } else {
                    None
                }
            })
            .or_else(|| {
                aliased_rule_ids(canonical)
                    .iter()
                    .find_map(|alias| self.settings.get(*alias))
            })
    }

    fn table_setting(&self, rule_id: &str, key: &str) -> Option<&toml::Value> {
        self.setting_value(rule_id)
            .and_then(|value| value.as_table())
            .and_then(|table| table.get(key))
    }

    fn integer_setting(&self, rule_id: &str, key: &str) -> Option<usize> {
        self.table_setting(rule_id, key)?
            .as_integer()
            .and_then(|value| usize::try_from(value).ok())
    }

    fn string_setting(&self, rule_id: &str, key: &str) -> Option<String> {
        self.table_setting(rule_id, key)?
            .as_str()
            .map(ToOwned::to_owned)
    }

    /// Decode typed settings for a specific rule, falling back to defaults on
    /// missing or invalid configuration.
    pub fn rule_settings<T>(&self, rule_id: &str) -> T
    where
        T: DeserializeOwned + Default,
    {
        self.setting_value(rule_id)
            .and_then(|value| value.clone().try_into::<T>().ok())
            .unwrap_or_default()
    }

    /// Get the configured severity for a rule, or None if the rule is disabled.
    pub fn rule_severity(&self, rule_id: &str, default_severity: Severity) -> Option<Severity> {
        if let Some(level) = self.rule_level(rule_id) {
            return level.into();
        }
        Some(default_severity)
    }

    /// Check if a specific rule is enabled.
    pub fn is_rule_enabled(&self, rule_id: &str, category: RuleCategory) -> bool {
        if let Some(level) = self.rule_level(rule_id) {
            return level != RuleLevel::Off;
        }

        match self.preset {
            RulePreset::All => true,
            RulePreset::Recommended => matches!(
                category,
                RuleCategory::Security | RuleCategory::BestPractices | RuleCategory::Naming
            ),
            RulePreset::SecurityOnly => matches!(category, RuleCategory::Security),
        }
    }

    pub fn code_complexity_threshold(&self) -> usize {
        self.integer_setting("best-practices/code-complexity", "threshold")
            .unwrap_or(10)
    }

    pub fn function_max_lines(&self) -> usize {
        self.integer_setting("best-practices/function-max-lines", "max_lines")
            .unwrap_or(50)
    }

    pub fn max_states_count(&self) -> usize {
        self.integer_setting("best-practices/max-states-count", "max_count")
            .unwrap_or(15)
    }

    pub fn foundry_test_function_pattern(&self) -> Option<String> {
        self.string_setting("naming/foundry-test-functions", "pattern")
    }

    pub fn max_line_length(&self) -> usize {
        self.integer_setting("style/max-line-length", "limit")
            .unwrap_or(120)
    }

    pub fn compiler_version_allowed(&self) -> Result<Option<Vec<VersionRequirement>>, String> {
        let Some(value) = self.table_setting("security/compiler-version", "allowed") else {
            return Ok(None);
        };

        let allowed = value
            .as_array()
            .ok_or_else(|| "expected an array".to_string())?;

        let mut requirements = Vec::with_capacity(allowed.len());
        for raw in allowed {
            let raw = raw
                .as_str()
                .ok_or_else(|| "all entries must be strings".to_string())?;
            requirements.push(VersionRequirement::parse(raw)?);
        }

        Ok(Some(requirements))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SolidityVersion {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl SolidityVersion {
    pub fn parse(input: &str) -> Option<Self> {
        let trimmed = input.trim();
        let parts: Vec<_> = trimmed.split('.').collect();
        if parts.len() != 3 {
            return None;
        }

        Some(Self {
            major: parts[0].parse().ok()?,
            minor: parts[1].parse().ok()?,
            patch: parts[2].parse().ok()?,
        })
    }

    pub fn cmp_key(self) -> (u64, u64, u64) {
        (self.major, self.minor, self.patch)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionOperator {
    GreaterThan,
    GreaterThanOrEqual,
    LessThan,
    LessThanOrEqual,
    Equal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VersionRequirement {
    pub operator: VersionOperator,
    pub version: SolidityVersion,
}

impl VersionRequirement {
    pub fn parse(input: &str) -> Result<Self, String> {
        let trimmed = input.trim();
        let (operator, version_str) = if let Some(rest) = trimmed.strip_prefix(">=") {
            (VersionOperator::GreaterThanOrEqual, rest)
        } else if let Some(rest) = trimmed.strip_prefix("<=") {
            (VersionOperator::LessThanOrEqual, rest)
        } else if let Some(rest) = trimmed.strip_prefix('>') {
            (VersionOperator::GreaterThan, rest)
        } else if let Some(rest) = trimmed.strip_prefix('<') {
            (VersionOperator::LessThan, rest)
        } else if let Some(rest) = trimmed.strip_prefix('=') {
            (VersionOperator::Equal, rest)
        } else {
            return Err(format!("invalid comparator `{trimmed}`"));
        };

        let version = SolidityVersion::parse(version_str)
            .ok_or_else(|| format!("invalid Solidity version `{version_str}`"))?;

        Ok(Self { operator, version })
    }

    pub fn matches(self, candidate: SolidityVersion) -> bool {
        match self.operator {
            VersionOperator::GreaterThan => candidate.cmp_key() > self.version.cmp_key(),
            VersionOperator::GreaterThanOrEqual => candidate.cmp_key() >= self.version.cmp_key(),
            VersionOperator::LessThan => candidate.cmp_key() < self.version.cmp_key(),
            VersionOperator::LessThanOrEqual => candidate.cmp_key() <= self.version.cmp_key(),
            VersionOperator::Equal => candidate.cmp_key() == self.version.cmp_key(),
        }
    }
}

impl Config {
    /// Decode typed settings for a specific rule, falling back to defaults on
    /// missing or invalid configuration.
    pub fn rule_settings<T>(&self, rule_id: &str) -> T
    where
        T: DeserializeOwned + Default,
    {
        self.lint.rule_settings(rule_id)
    }
}

/// Formatter configuration section.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct FormatConfig {
    /// Maximum line length before wrapping (default: 120).
    pub line_length: usize,
    /// Number of spaces per indentation level (default: 4).
    pub tab_width: usize,
    /// Use tabs instead of spaces for indentation.
    pub use_tabs: bool,
    /// Use single quotes instead of double quotes for strings.
    pub single_quote: bool,
    /// Add spaces inside curly braces `{ }`.
    pub bracket_spacing: bool,
    /// How to handle underscores in number literals.
    pub number_underscore: NumberUnderscore,
    /// How to normalize uint/int type aliases.
    pub uint_type: UintType,
    /// Add space in override specifiers.
    pub override_spacing: bool,
    /// Wrap comments to fit within line length.
    pub wrap_comments: bool,
    /// Sort import statements alphabetically.
    pub sort_imports: bool,
    /// Style for multiline function headers.
    pub multiline_func_header: MultilineFuncHeader,
    /// Spacing between declarations inside contract bodies.
    pub contract_body_spacing: ContractBodySpacing,
    /// Put opening brace on new line when inheritance list wraps (default: true).
    pub inheritance_brace_new_line: bool,
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
            contract_body_spacing: ContractBodySpacing::Preserve,
            inheritance_brace_new_line: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NumberUnderscore {
    /// Keep underscores as-is.
    Preserve,
    /// Insert underscores every three digits.
    Thousands,
    /// Remove all underscores from number literals.
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
    /// Break after attributes (visibility, modifiers) first.
    AttributesFirst,
    /// Break after parameters first.
    ParamsFirst,
    /// Break after each element.
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContractBodySpacing {
    /// Preserve blank lines from the original source (default).
    Preserve,
    /// Always add a single blank line between declarations.
    Single,
    /// No blank lines between declarations (compact).
    Compact,
}

/// Global configuration section.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GlobalConfig {
    /// Solidity version hint (auto-detected from pragma if omitted).
    pub solidity_version: Option<String>,
    /// File glob patterns to include.
    pub include: Vec<String>,
    /// File glob patterns to exclude.
    pub exclude: Vec<String>,
    /// Whether to honor `.gitignore` patterns.
    pub respect_gitignore: bool,
    /// Number of parallel threads (0 = auto-detect).
    pub threads: usize,
    /// Directory for the incremental cache.
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
            exclude: vec!["lib/**".into(), "node_modules/**".into(), "out/**".into()],
            respect_gitignore: true,
            threads: 0,
            cache_dir: ".solgrid_cache".into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConfigResolver {
    explicit: Option<Arc<Config>>,
    cache: HashMap<PathBuf, Arc<Config>>,
}

impl ConfigResolver {
    pub fn new(explicit: Option<Config>) -> Self {
        Self {
            explicit: explicit.map(Arc::new),
            cache: HashMap::new(),
        }
    }

    pub fn resolve_for_path(&mut self, path: &Path) -> Arc<Config> {
        if let Some(config) = &self.explicit {
            return config.clone();
        }

        let dir = if path.is_dir() {
            path.to_path_buf()
        } else {
            path.parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("."))
        };

        if let Some(config) = self.cache.get(&dir) {
            return config.clone();
        }

        let config = Arc::new(resolve_config(&dir));
        self.cache.insert(dir, config.clone());
        config
    }
}

fn normalize_rule_aliases(config: &mut Config) {
    let legacy_rule_keys: Vec<_> = config
        .lint
        .rules
        .keys()
        .filter(|key| canonical_rule_id(key.as_str()) != key.as_str())
        .cloned()
        .collect();
    for key in legacy_rule_keys {
        if let Some(level) = config.lint.rules.remove(&key) {
            config
                .lint
                .rules
                .entry(canonical_rule_id(&key).to_string())
                .or_insert(level);
        }
    }

    let legacy_setting_keys: Vec<_> = config
        .lint
        .settings
        .keys()
        .filter(|key| canonical_rule_id(key.as_str()) != key.as_str())
        .cloned()
        .collect();
    for key in legacy_setting_keys {
        if let Some(value) = config.lint.settings.remove(&key) {
            config
                .lint
                .settings
                .entry(canonical_rule_id(&key).to_string())
                .or_insert(value);
        }
    }
}

fn warn_for_invalid_settings(config: &Config, path: &Path) {
    if let Err(error) = config.lint.compiler_version_allowed() {
        eprintln!(
            "warning: invalid setting for `security/compiler-version.allowed` in {}: {error}; falling back to default compiler-version rule behavior",
            path.display()
        );
    }
}

/// Load configuration from a TOML file.
pub fn load_config(path: &Path) -> Result<Config, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    let mut config: Config =
        toml::from_str(&content).map_err(|e| format!("failed to parse {}: {e}", path.display()))?;
    normalize_rule_aliases(&mut config);
    warn_for_invalid_settings(&config, path);
    Ok(config)
}

/// Discover and load config by walking up the filesystem from `start_dir`.
/// Falls back to foundry.toml `[fmt]` section if no solgrid.toml is found.
/// Returns default config if no config file is found.
pub fn resolve_config(start_dir: &Path) -> Config {
    let search_root = if start_dir.is_dir() {
        start_dir.to_path_buf()
    } else {
        start_dir
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    };

    if let Some(path) = find_config_file(&search_root) {
        match load_config(&path) {
            Ok(config) => return config,
            Err(e) => {
                eprintln!("warning: {e}, using defaults");
            }
        }
    }

    // Fallback: try foundry.toml
    if let Some(path) = find_foundry_toml(&search_root) {
        match load_foundry_fmt_config(&path) {
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

/// Find the nearest `foundry.toml` by walking up from `start_dir`.
fn find_foundry_toml(start_dir: &Path) -> Option<PathBuf> {
    let mut current = start_dir.to_path_buf();
    loop {
        let config_path = current.join("foundry.toml");
        if config_path.exists() {
            return Some(config_path);
        }
        if !current.pop() {
            break;
        }
    }
    None
}

/// Load format config from a foundry.toml `[fmt]` section.
fn load_foundry_fmt_config(path: &Path) -> Result<Config, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    let table: toml::Table =
        toml::from_str(&content).map_err(|e| format!("failed to parse {}: {e}", path.display()))?;

    let mut config = Config::default();

    if let Some(fmt) = table.get("fmt").and_then(|v| v.as_table()) {
        if let Some(v) = fmt.get("line_length").and_then(|v| v.as_integer()) {
            config.format.line_length = v as usize;
        }
        if let Some(v) = fmt.get("tab_width").and_then(|v| v.as_integer()) {
            config.format.tab_width = v as usize;
        }
        if let Some(v) = fmt.get("bracket_spacing").and_then(|v| v.as_bool()) {
            config.format.bracket_spacing = v;
        }
        if let Some(v) = fmt.get("quote_style").and_then(|v| v.as_str()) {
            config.format.single_quote = v == "single";
        }
        if let Some(v) = fmt.get("int_types").and_then(|v| v.as_str()) {
            config.format.uint_type = match v {
                "long" => UintType::Long,
                "short" => UintType::Short,
                "preserve" => UintType::Preserve,
                _ => UintType::Long,
            };
        }
        if let Some(v) = fmt.get("number_underscore").and_then(|v| v.as_str()) {
            config.format.number_underscore = match v {
                "thousands" => NumberUnderscore::Thousands,
                "remove" => NumberUnderscore::Remove,
                "preserve" => NumberUnderscore::Preserve,
                _ => NumberUnderscore::Preserve,
            };
        }
        if let Some(v) = fmt.get("multiline_func_header").and_then(|v| v.as_str()) {
            config.format.multiline_func_header = match v {
                "attributes_first" => MultilineFuncHeader::AttributesFirst,
                "params_first" => MultilineFuncHeader::ParamsFirst,
                "all" => MultilineFuncHeader::All,
                _ => MultilineFuncHeader::AttributesFirst,
            };
        }
        if let Some(v) = fmt.get("sort_imports").and_then(|v| v.as_bool()) {
            config.format.sort_imports = v;
        }
        if let Some(v) = fmt.get("contract_body_spacing").and_then(|v| v.as_str()) {
            config.format.contract_body_spacing = match v {
                "preserve" => ContractBodySpacing::Preserve,
                "single" => ContractBodySpacing::Single,
                "compact" => ContractBodySpacing::Compact,
                _ => ContractBodySpacing::Preserve,
            };
        } else if let Some(v) = fmt.get("contract_new_lines").and_then(|v| v.as_bool()) {
            // Backwards compatibility: contract_new_lines maps to single/compact
            config.format.contract_body_spacing = if v {
                ContractBodySpacing::Single
            } else {
                ContractBodySpacing::Compact
            };
        }
        if let Some(v) = fmt
            .get("inheritance_brace_new_line")
            .and_then(|v| v.as_bool())
        {
            config.format.inheritance_brace_new_line = v;
        }
        if let Some(v) = fmt.get("override_spacing").and_then(|v| v.as_bool()) {
            config.format.override_spacing = v;
        }
        if let Some(v) = fmt.get("wrap_comments").and_then(|v| v.as_bool()) {
            config.format.wrap_comments = v;
        }
    }

    Ok(config)
}

/// Find the workspace root by walking up from `start_dir` looking for
/// `foundry.toml` or `remappings.txt`. Returns the directory containing the file.
pub fn find_workspace_root(start_dir: &Path) -> Option<PathBuf> {
    let mut current = start_dir.to_path_buf();
    loop {
        if current.join("foundry.toml").exists() || current.join("remappings.txt").exists() {
            return Some(current);
        }
        if !current.pop() {
            break;
        }
    }
    None
}

/// Parse remapping lines of the form `[context:]prefix=target`.
pub fn parse_remappings(content: &str, workspace_root: &Path) -> Vec<(String, PathBuf)> {
    let mut result = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Strip optional `context:` prefix.
        let mapping = if let Some(colon_pos) = line.find(':') {
            // Only treat as context prefix if `=` comes after the colon.
            if line[colon_pos..].contains('=') {
                &line[colon_pos + 1..]
            } else {
                line
            }
        } else {
            line
        };

        if let Some(eq_pos) = mapping.find('=') {
            let prefix = mapping[..eq_pos].to_string();
            let target = &mapping[eq_pos + 1..];
            let target_path = if Path::new(target).is_absolute() {
                PathBuf::from(target)
            } else {
                workspace_root.join(target)
            };
            result.push((prefix, target_path));
        }
    }
    result
}

/// Load remappings from `remappings.txt` or `foundry.toml` at the workspace root.
///
/// Format: `prefix=target` per line. Optional `context:` prefix is ignored.
pub fn load_remappings(workspace_root: &Path) -> Vec<(String, PathBuf)> {
    // Try remappings.txt first.
    let remappings_file = workspace_root.join("remappings.txt");
    if let Ok(content) = std::fs::read_to_string(&remappings_file) {
        return parse_remappings(&content, workspace_root);
    }

    // Try foundry.toml [profile.default.remappings].
    let foundry_file = workspace_root.join("foundry.toml");
    if let Ok(content) = std::fs::read_to_string(&foundry_file) {
        if let Ok(table) = content.parse::<toml::Table>() {
            if let Some(remappings) = table
                .get("profile")
                .and_then(|p| p.get("default"))
                .and_then(|d| d.get("remappings"))
                .and_then(|r| r.as_array())
            {
                let text: String = remappings
                    .iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join("\n");
                return parse_remappings(&text, workspace_root);
            }
        }
    }

    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
    struct TestRuleSettings {
        enabled: bool,
        names: Vec<String>,
    }

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.lint.preset, RulePreset::Recommended);
        assert_eq!(config.format.line_length, 120);
        assert_eq!(config.format.tab_width, 4);
        assert!(!config.format.use_tabs);
        assert!(!config.format.single_quote);
        assert!(!config.format.bracket_spacing);
        assert!(config.global.respect_gitignore);
        assert_eq!(config.global.threads, 0);
    }

    #[test]
    fn test_parse_minimal_toml() {
        let toml_str = r#"
[lint]
preset = "recommended"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.lint.preset, RulePreset::Recommended);
        assert_eq!(config.format.line_length, 120); // default
    }

    #[test]
    fn test_parse_full_toml() {
        let toml_str = r#"
[lint]
preset = "all"

[lint.rules]
"security/tx-origin" = "error"
"gas/custom-errors" = "off"

[format]
line_length = 80
tab_width = 2
use_tabs = true
single_quote = true
bracket_spacing = true

[global]
exclude = ["lib/**"]
respect_gitignore = false
threads = 4
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.lint.preset, RulePreset::All);
        assert_eq!(
            config.lint.rules.get("security/tx-origin"),
            Some(&RuleLevel::Error)
        );
        assert_eq!(
            config.lint.rules.get("gas/custom-errors"),
            Some(&RuleLevel::Off)
        );
        assert_eq!(config.format.line_length, 80);
        assert_eq!(config.format.tab_width, 2);
        assert!(config.format.use_tabs);
        assert!(config.format.single_quote);
        assert!(config.format.bracket_spacing);
        assert!(!config.global.respect_gitignore);
        assert_eq!(config.global.threads, 4);
    }

    #[test]
    fn test_rule_severity_default() {
        let config = LintConfig::default();
        assert_eq!(
            config.rule_severity("security/tx-origin", Severity::Error),
            Some(Severity::Error)
        );
        assert_eq!(
            config.rule_severity("best-practices/no-console", Severity::Warning),
            Some(Severity::Warning)
        );
    }

    #[test]
    fn test_rule_severity_override() {
        let mut config = LintConfig::default();
        config
            .rules
            .insert("security/tx-origin".to_string(), RuleLevel::Warn);
        assert_eq!(
            config.rule_severity("security/tx-origin", Severity::Error),
            Some(Severity::Warning)
        );
    }

    #[test]
    fn test_rule_disabled() {
        let mut config = LintConfig::default();
        config
            .rules
            .insert("security/tx-origin".to_string(), RuleLevel::Off);
        assert!(!config.is_rule_enabled("security/tx-origin", RuleCategory::Security));
        assert_eq!(
            config.rule_severity("security/tx-origin", Severity::Error),
            None
        );
    }

    #[test]
    fn test_lint_rule_settings_decode_typed_value() {
        let mut config = LintConfig::default();
        config.settings.insert(
            "docs/natspec".to_string(),
            toml::Value::Table(
                [
                    ("enabled".to_string(), toml::Value::Boolean(true)),
                    (
                        "names".to_string(),
                        toml::Value::Array(vec![toml::Value::String("notice".into())]),
                    ),
                ]
                .into_iter()
                .collect(),
            ),
        );

        let settings: TestRuleSettings = config.rule_settings("docs/natspec");
        assert_eq!(
            settings,
            TestRuleSettings {
                enabled: true,
                names: vec!["notice".into()],
            }
        );
    }

    #[test]
    fn test_lint_rule_settings_missing_value_uses_default() {
        let config = LintConfig::default();
        let settings: TestRuleSettings = config.rule_settings("docs/natspec");
        assert_eq!(settings, TestRuleSettings::default());
    }

    #[test]
    fn test_lint_rule_settings_invalid_value_uses_default() {
        let mut config = LintConfig::default();
        config.settings.insert(
            "docs/natspec".to_string(),
            toml::Value::String("not-an-object".into()),
        );

        let settings: TestRuleSettings = config.rule_settings("docs/natspec");
        assert_eq!(settings, TestRuleSettings::default());
    }

    #[test]
    fn test_config_rule_settings_delegates_to_lint_settings() {
        let mut config = Config::default();
        config.lint.settings.insert(
            "docs/natspec".to_string(),
            toml::Value::Table(
                [
                    ("enabled".to_string(), toml::Value::Boolean(true)),
                    (
                        "names".to_string(),
                        toml::Value::Array(vec![toml::Value::String("dev".into())]),
                    ),
                ]
                .into_iter()
                .collect(),
            ),
        );

        let settings: TestRuleSettings = config.rule_settings("docs/natspec");
        assert_eq!(
            settings,
            TestRuleSettings {
                enabled: true,
                names: vec!["dev".into()],
            }
        );
    }

    #[test]
    fn test_format_config_defaults() {
        let config = FormatConfig::default();
        assert_eq!(config.line_length, 120);
        assert_eq!(config.tab_width, 4);
        assert!(!config.use_tabs);
        assert!(!config.single_quote);
        assert!(!config.bracket_spacing);
        assert_eq!(config.number_underscore, NumberUnderscore::Preserve);
        assert_eq!(config.uint_type, UintType::Long);
        assert!(config.override_spacing);
        assert!(!config.wrap_comments);
        assert!(!config.sort_imports);
        assert_eq!(
            config.multiline_func_header,
            MultilineFuncHeader::AttributesFirst
        );
        assert_eq!(config.contract_body_spacing, ContractBodySpacing::Preserve);
        assert!(config.inheritance_brace_new_line);
    }

    #[test]
    fn test_config_roundtrip() {
        let config = Config::default();
        let toml_str = toml::to_string(&config).unwrap();
        let parsed: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.format.line_length, config.format.line_length);
        assert_eq!(parsed.format.tab_width, config.format.tab_width);
    }

    #[test]
    fn test_recommended_preset_membership() {
        let config = LintConfig::default();
        assert!(config.is_rule_enabled("security/tx-origin", RuleCategory::Security));
        assert!(config.is_rule_enabled("best-practices/no-console", RuleCategory::BestPractices));
        assert!(config.is_rule_enabled("naming/func-name-mixedcase", RuleCategory::Naming));
        assert!(!config.is_rule_enabled("docs/natspec", RuleCategory::Docs));
        assert!(!config.is_rule_enabled("gas/custom-errors", RuleCategory::Gas));
        assert!(!config.is_rule_enabled("style/max-line-length", RuleCategory::Style));
    }

    #[test]
    fn test_rule_override_can_enable_outside_preset() {
        let mut config = LintConfig::default();
        config.rules.insert("docs/natspec".into(), RuleLevel::Warn);
        assert!(config.is_rule_enabled("docs/natspec", RuleCategory::Docs));
        assert_eq!(
            config.rule_severity("docs/natspec", Severity::Info),
            Some(Severity::Warning)
        );
    }

    #[test]
    fn test_rule_alias_lookup_uses_canonical_rule() {
        let mut config = LintConfig::default();
        config
            .rules
            .insert("best-practices/use-natspec".into(), RuleLevel::Off);
        assert!(!config.is_rule_enabled("docs/natspec", RuleCategory::Docs));
        assert_eq!(config.rule_severity("docs/natspec", Severity::Info), None);
    }

    #[test]
    fn test_compiler_version_allowed_parsing() {
        let mut config = LintConfig::default();
        config.settings.insert(
            "security/compiler-version".into(),
            toml::Value::Table(
                [(
                    "allowed".into(),
                    toml::Value::Array(vec![
                        toml::Value::String(">=0.8.19".into()),
                        toml::Value::String("<0.9.0".into()),
                    ]),
                )]
                .into_iter()
                .collect(),
            ),
        );

        let allowed = config.compiler_version_allowed().unwrap().unwrap();
        assert_eq!(allowed.len(), 2);
        assert!(allowed[0].matches(SolidityVersion::parse("0.8.24").unwrap()));
        assert!(!allowed[0].matches(SolidityVersion::parse("0.8.18").unwrap()));
    }

    #[test]
    fn test_config_resolver_uses_nearest_config() {
        let root = std::env::temp_dir().join(format!(
            "solgrid_config_resolver_{}_{}",
            std::process::id(),
            1
        ));
        let nested = root.join("packages/project/src");
        fs::create_dir_all(&nested).unwrap();
        fs::write(
            root.join("solgrid.toml"),
            "[lint]\npreset = \"security-only\"\n",
        )
        .unwrap();
        fs::write(
            root.join("packages/project/solgrid.toml"),
            "[lint]\npreset = \"all\"\n",
        )
        .unwrap();

        let file = nested.join("Token.sol");
        fs::write(&file, "contract Token {}").unwrap();

        let mut resolver = ConfigResolver::new(None);
        let config = resolver.resolve_for_path(&file);
        assert_eq!(config.lint.preset, RulePreset::All);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn test_load_config_normalizes_deprecated_rule_alias() {
        let root =
            std::env::temp_dir().join(format!("solgrid_config_alias_{}_{}", std::process::id(), 2));
        fs::create_dir_all(&root).unwrap();
        let config_path = root.join("solgrid.toml");
        fs::write(
            &config_path,
            "[lint.rules]\n\"best-practices/use-natspec\" = \"off\"\n",
        )
        .unwrap();

        let config = load_config(&config_path).unwrap();
        assert_eq!(config.lint.rules.get("docs/natspec"), Some(&RuleLevel::Off));
        assert!(!config.lint.rules.contains_key("best-practices/use-natspec"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn test_load_config_normalizes_deprecated_natspec_settings_alias() {
        let root = std::env::temp_dir().join(format!(
            "solgrid_config_alias_settings_{}_{}",
            std::process::id(),
            3
        ));
        fs::create_dir_all(&root).unwrap();
        let config_path = root.join("solgrid.toml");
        fs::write(
            &config_path,
            "[lint.settings.\"docs/natspec-function\"]\ncomment_style = \"either\"\n",
        )
        .unwrap();

        let config = load_config(&config_path).unwrap();
        assert!(config.lint.settings.contains_key("docs/natspec"));
        assert!(!config.lint.settings.contains_key("docs/natspec-function"));

        let _ = fs::remove_dir_all(root);
    }

    /// Finding #5: empty include list is a valid config value. Verify that
    /// loading a config with `include = []` results in an empty include vec
    /// (rather than falling back to defaults).
    #[test]
    fn test_empty_include_is_preserved_not_defaulted() {
        let root = std::env::temp_dir().join(format!(
            "solgrid_config_empty_include_{}_{}",
            std::process::id(),
            1
        ));
        fs::create_dir_all(&root).unwrap();
        let config_path = root.join("solgrid.toml");
        fs::write(&config_path, "[global]\ninclude = []\n").unwrap();

        let config = load_config(&config_path).unwrap();
        assert!(
            config.global.include.is_empty(),
            "empty include should be preserved, not replaced with defaults"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn test_parse_remappings_basic() {
        let content = "@openzeppelin/=lib/openzeppelin-contracts/\nforge-std/=lib/forge-std/src/\n";
        let result = parse_remappings(content, Path::new("/project"));

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].0, "@openzeppelin/");
        assert_eq!(
            result[0].1,
            PathBuf::from("/project/lib/openzeppelin-contracts/")
        );
        assert_eq!(result[1].0, "forge-std/");
        assert_eq!(result[1].1, PathBuf::from("/project/lib/forge-std/src/"));
    }

    #[test]
    fn test_parse_remappings_with_context() {
        let content = "ds-test:ds-test/=lib/ds-test/src/\n";
        let result = parse_remappings(content, Path::new("/project"));

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, "ds-test/");
    }

    #[test]
    fn test_parse_remappings_empty_and_comments() {
        let content = "# comment\n\n  \n@oz/=lib/oz/\n";
        let result = parse_remappings(content, Path::new("/project"));

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, "@oz/");
    }

    #[test]
    fn test_parse_remappings_absolute_target() {
        let content = "@oz/=/absolute/path/to/oz/\n";
        let result = parse_remappings(content, Path::new("/project"));

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].1, PathBuf::from("/absolute/path/to/oz/"));
    }

    #[test]
    fn test_find_workspace_root_with_remappings_txt() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("src").join("contracts");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(dir.path().join("remappings.txt"), "@oz/=lib/oz/\n").unwrap();

        let result = find_workspace_root(&sub);
        assert_eq!(result, Some(dir.path().to_path_buf()));
    }

    #[test]
    fn test_find_workspace_root_with_foundry_toml() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("src");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(dir.path().join("foundry.toml"), "[profile.default]\n").unwrap();

        let result = find_workspace_root(&sub);
        assert_eq!(result, Some(dir.path().to_path_buf()));
    }

    #[test]
    fn test_find_workspace_root_none() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("empty");
        std::fs::create_dir_all(&sub).unwrap();

        let result = find_workspace_root(&sub);
        // No foundry.toml or remappings.txt anywhere up the tree in the tempdir
        // (may find one in a parent of the tempdir in CI, so just check it doesn't
        // return the sub directory itself)
        assert_ne!(result, Some(sub));
    }

    #[test]
    fn test_load_remappings_from_remappings_txt() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("remappings.txt"),
            "@oz/=lib/oz/\nforge-std/=lib/forge-std/src/\n",
        )
        .unwrap();

        let result = load_remappings(dir.path());
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].0, "@oz/");
        assert_eq!(result[1].0, "forge-std/");
    }

    #[test]
    fn test_load_remappings_from_foundry_toml() {
        let dir = tempfile::tempdir().unwrap();
        let toml_content = r#"
[profile.default]
remappings = [
    "@oz/=lib/oz/",
    "forge-std/=lib/forge-std/src/",
]
"#;
        std::fs::write(dir.path().join("foundry.toml"), toml_content).unwrap();

        let result = load_remappings(dir.path());
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].0, "@oz/");
        assert_eq!(result[1].0, "forge-std/");
    }

    #[test]
    fn test_load_remappings_prefers_remappings_txt_over_foundry_toml() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("remappings.txt"), "@txt/=lib/txt/\n").unwrap();
        std::fs::write(
            dir.path().join("foundry.toml"),
            "[profile.default]\nremappings = [\"@toml/=lib/toml/\"]\n",
        )
        .unwrap();

        let result = load_remappings(dir.path());
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, "@txt/");
    }

    #[test]
    fn test_load_remappings_empty_when_no_files() {
        let dir = tempfile::tempdir().unwrap();
        let result = load_remappings(dir.path());
        assert!(result.is_empty());
    }
}
