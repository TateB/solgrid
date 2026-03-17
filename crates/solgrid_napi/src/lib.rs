//! NAPI-RS bindings for solgrid's Solidity formatter.
//!
//! Exposes `parse()`, `format()`, and `check()` functions to Node.js
//! for use by the `prettier-plugin-solgrid` npm package.

use napi_derive::napi;
use solgrid_config::{
    ContractBodySpacing, FormatConfig, MultilineFuncHeader, NumberUnderscore, UintType,
};

/// JavaScript-friendly formatting options.
///
/// Maps Prettier conventions to solgrid's internal `FormatConfig`:
/// - `print_width` → `line_length`
/// - `tab_width` → `tab_width`
/// - `use_tabs` → `use_tabs`
/// - `single_quote` → `single_quote`
/// - `bracket_spacing` → `bracket_spacing`
#[napi(object)]
pub struct FormatOptions {
    /// Maps to solgrid `line_length`. Prettier default: 80.
    pub print_width: Option<u32>,
    /// Maps to solgrid `tab_width`. Prettier default: 2.
    pub tab_width: Option<u32>,
    /// Maps to solgrid `use_tabs`. Prettier default: false.
    pub use_tabs: Option<bool>,
    /// Maps to solgrid `single_quote`. Prettier default: false.
    pub single_quote: Option<bool>,
    /// Maps to solgrid `bracket_spacing`. Prettier default: true.
    pub bracket_spacing: Option<bool>,
    /// solgrid-specific: "preserve", "thousands", or "remove".
    pub number_underscore: Option<String>,
    /// solgrid-specific: "uint256"/"long", "uint"/"short", or "preserve".
    pub uint_type: Option<String>,
    /// solgrid-specific: add space in override specifiers.
    pub override_spacing: Option<bool>,
    /// solgrid-specific: wrap comments to fit within line length.
    pub wrap_comments: Option<bool>,
    /// solgrid-specific: sort import statements alphabetically.
    pub sort_imports: Option<bool>,
    /// solgrid-specific: "attributes_first", "params_first", or "all".
    pub multiline_func_header: Option<String>,
    /// solgrid-specific: spacing between contract body declarations.
    /// "preserve" (default), "single", or "compact".
    pub contract_body_spacing: Option<String>,
    /// solgrid-specific: put opening brace on new line for multiline inheritance.
    pub inheritance_brace_new_line: Option<bool>,
}

/// Convert JavaScript format options to solgrid's internal `FormatConfig`.
pub fn map_options(options: Option<FormatOptions>) -> FormatConfig {
    let Some(opts) = options else {
        return FormatConfig::default();
    };

    let mut config = FormatConfig::default();

    if let Some(pw) = opts.print_width {
        config.line_length = pw as usize;
    }
    if let Some(tw) = opts.tab_width {
        config.tab_width = tw as usize;
    }
    if let Some(ut) = opts.use_tabs {
        config.use_tabs = ut;
    }
    if let Some(sq) = opts.single_quote {
        config.single_quote = sq;
    }
    if let Some(bs) = opts.bracket_spacing {
        config.bracket_spacing = bs;
    }
    if let Some(ref nu) = opts.number_underscore {
        config.number_underscore = match nu.as_str() {
            "thousands" => NumberUnderscore::Thousands,
            "remove" => NumberUnderscore::Remove,
            _ => NumberUnderscore::Preserve,
        };
    }
    if let Some(ref ut) = opts.uint_type {
        config.uint_type = match ut.as_str() {
            "uint256" | "long" => UintType::Long,
            "uint" | "short" => UintType::Short,
            _ => UintType::Preserve,
        };
    }
    if let Some(os) = opts.override_spacing {
        config.override_spacing = os;
    }
    if let Some(wc) = opts.wrap_comments {
        config.wrap_comments = wc;
    }
    if let Some(si) = opts.sort_imports {
        config.sort_imports = si;
    }
    if let Some(ref mf) = opts.multiline_func_header {
        config.multiline_func_header = match mf.as_str() {
            "params_first" => MultilineFuncHeader::ParamsFirst,
            "all" => MultilineFuncHeader::All,
            _ => MultilineFuncHeader::AttributesFirst,
        };
    }
    if let Some(ref cbs) = opts.contract_body_spacing {
        config.contract_body_spacing = match cbs.as_str() {
            "single" => ContractBodySpacing::Single,
            "compact" => ContractBodySpacing::Compact,
            _ => ContractBodySpacing::Preserve,
        };
    }
    if let Some(ibn) = opts.inheritance_brace_new_line {
        config.inheritance_brace_new_line = ibn;
    }

    config
}

/// Validate that the source is syntactically valid Solidity.
///
/// Returns `true` if the source parses without errors, `false` otherwise.
#[napi]
pub fn parse(source: String) -> napi::Result<bool> {
    match solgrid_parser::check_syntax(&source, "<stdin>") {
        Ok(()) => Ok(true),
        Err(_) => Ok(false),
    }
}

/// Format Solidity source code with the given options.
///
/// Returns the formatted source string.
/// Throws on syntax errors.
#[napi]
pub fn format(source: String, options: Option<FormatOptions>) -> napi::Result<String> {
    let config = map_options(options);
    solgrid_formatter::format_source(&source, &config).map_err(napi::Error::from_reason)
}

/// Check if source is already formatted with the given options.
///
/// Returns `true` if the source matches what the formatter would produce.
/// Throws on syntax errors.
#[napi]
pub fn check(source: String, options: Option<FormatOptions>) -> napi::Result<bool> {
    let config = map_options(options);
    solgrid_formatter::check_formatted(&source, &config).map_err(napi::Error::from_reason)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_options_mapping() {
        let config = map_options(None);
        assert_eq!(config.line_length, 120);
        assert_eq!(config.tab_width, 4);
        assert!(!config.use_tabs);
        assert!(!config.single_quote);
        assert!(!config.bracket_spacing);
    }

    #[test]
    fn test_prettier_options_mapping() {
        let opts = FormatOptions {
            print_width: Some(80),
            tab_width: Some(2),
            use_tabs: Some(true),
            single_quote: Some(true),
            bracket_spacing: Some(true),
            number_underscore: Some("thousands".into()),
            uint_type: Some("uint256".into()),
            override_spacing: None,
            wrap_comments: None,
            sort_imports: None,
            multiline_func_header: None,
            contract_body_spacing: None,
            inheritance_brace_new_line: None,
        };
        let config = map_options(Some(opts));
        assert_eq!(config.line_length, 80);
        assert_eq!(config.tab_width, 2);
        assert!(config.use_tabs);
        assert!(config.single_quote);
        assert!(config.bracket_spacing);
        assert_eq!(config.number_underscore, NumberUnderscore::Thousands);
        assert_eq!(config.uint_type, UintType::Long);
    }

    #[test]
    fn test_partial_options() {
        let opts = FormatOptions {
            print_width: Some(100),
            tab_width: None,
            use_tabs: None,
            single_quote: None,
            bracket_spacing: None,
            number_underscore: None,
            uint_type: Some("short".into()),
            override_spacing: None,
            wrap_comments: None,
            sort_imports: Some(true),
            multiline_func_header: Some("params_first".into()),
            contract_body_spacing: None,
            inheritance_brace_new_line: None,
        };
        let config = map_options(Some(opts));
        assert_eq!(config.line_length, 100);
        assert_eq!(config.tab_width, 4); // default
        assert!(!config.use_tabs); // default
        assert_eq!(config.uint_type, UintType::Short);
        assert!(config.sort_imports);
        assert_eq!(
            config.multiline_func_header,
            MultilineFuncHeader::ParamsFirst
        );
    }

    #[test]
    fn test_number_underscore_variants() {
        for (input, expected) in [
            ("preserve", NumberUnderscore::Preserve),
            ("thousands", NumberUnderscore::Thousands),
            ("remove", NumberUnderscore::Remove),
            ("invalid", NumberUnderscore::Preserve),
        ] {
            let opts = FormatOptions {
                print_width: None,
                tab_width: None,
                use_tabs: None,
                single_quote: None,
                bracket_spacing: None,
                number_underscore: Some(input.into()),
                uint_type: None,
                override_spacing: None,
                wrap_comments: None,
                sort_imports: None,
                multiline_func_header: None,
                contract_body_spacing: None,
            inheritance_brace_new_line: None,
            };
            let config = map_options(Some(opts));
            assert_eq!(config.number_underscore, expected, "input: {input}");
        }
    }

    #[test]
    fn test_uint_type_variants() {
        for (input, expected) in [
            ("uint256", UintType::Long),
            ("long", UintType::Long),
            ("uint", UintType::Short),
            ("short", UintType::Short),
            ("preserve", UintType::Preserve),
            ("invalid", UintType::Preserve),
        ] {
            let opts = FormatOptions {
                print_width: None,
                tab_width: None,
                use_tabs: None,
                single_quote: None,
                bracket_spacing: None,
                number_underscore: None,
                uint_type: Some(input.into()),
                override_spacing: None,
                wrap_comments: None,
                sort_imports: None,
                multiline_func_header: None,
                contract_body_spacing: None,
            inheritance_brace_new_line: None,
            };
            let config = map_options(Some(opts));
            assert_eq!(config.uint_type, expected, "input: {input}");
        }
    }
}
