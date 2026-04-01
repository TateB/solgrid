use serde::Deserialize;
use solgrid_config::Config;
use solgrid_parser::solar_ast::{FunctionKind, ItemFunction};

const DEFAULT_INITIALIZATION_FUNCTIONS: &[&str] = &[
    "constructor",
    "supportsInterface",
    "supportsFeature",
    "initialize",
];

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct InitializationSettings {
    initialization_functions: Vec<String>,
}

pub(crate) fn configured_initialization_functions(config: &Config) -> Vec<String> {
    let settings: InitializationSettings = config.rule_settings("style/category-headers");
    settings.initialization_functions
}

pub(crate) fn is_initialization_function(
    func: &ItemFunction<'_>,
    configured_names: &[String],
) -> bool {
    if matches!(func.kind, FunctionKind::Constructor) {
        return true;
    }

    let name = match func.header.name {
        Some(name) => name.as_str().to_string(),
        None => return false,
    };

    if name.starts_with("_init") || name.starts_with("__init") {
        return true;
    }

    if configured_names.is_empty() {
        DEFAULT_INITIALIZATION_FUNCTIONS.contains(&name.as_str())
    } else {
        configured_names
            .iter()
            .any(|configured_name| configured_name == &name)
    }
}
