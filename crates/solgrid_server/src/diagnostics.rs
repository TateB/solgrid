//! Diagnostics — real-time lint integration for the LSP server.

use crate::convert;
use crate::resolve::ImportResolver;
use crate::symbols;
use solgrid_config::Config;
use solgrid_diagnostics::FileResult;
use solgrid_linter::LintEngine;
use std::path::Path;
use tower_lsp_server::ls_types;

/// Run the linter on source text and return LSP diagnostics.
pub fn lint_to_lsp_diagnostics(
    engine: &LintEngine,
    source: &str,
    path: &Path,
    config: &Config,
) -> Vec<ls_types::Diagnostic> {
    let result = engine.lint_source(source, path, config);
    file_result_to_lsp_diagnostics(source, &result)
}

/// Convert a FileResult to LSP diagnostics.
pub fn file_result_to_lsp_diagnostics(
    source: &str,
    result: &FileResult,
) -> Vec<ls_types::Diagnostic> {
    result
        .diagnostics
        .iter()
        .map(|d| convert::diagnostic_to_lsp(source, d))
        .collect()
}

/// Produce diagnostics for import paths that cannot be resolved.
pub fn unresolved_import_diagnostics(
    source: &str,
    path: &Path,
    resolver: &ImportResolver,
) -> Vec<ls_types::Diagnostic> {
    let table = match symbols::build_symbol_table(source, &path.to_string_lossy()) {
        Some(t) => t,
        None => return Vec::new(),
    };

    table
        .imports
        .iter()
        .filter(|import| resolver.resolve(&import.path, path).is_none())
        .map(|import| ls_types::Diagnostic {
            range: convert::span_to_range(source, &import.path_span),
            severity: Some(ls_types::DiagnosticSeverity::ERROR),
            code: Some(ls_types::NumberOrString::String("unresolved-import".into())),
            code_description: None,
            source: Some("solgrid".into()),
            message: format!("cannot resolve import \"{}\"", import.path),
            related_information: None,
            tags: None,
            data: None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolve::ImportResolver;
    use std::fs;

    #[test]
    fn test_lint_to_lsp_diagnostics_detects_issues() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function bad() public {
        require(tx.origin == msg.sender);
    }
}
"#;
        let engine = LintEngine::new();
        let config = Config::default();
        let diagnostics = lint_to_lsp_diagnostics(&engine, source, Path::new("test.sol"), &config);

        // Should detect at least the tx.origin usage
        assert!(
            !diagnostics.is_empty(),
            "should detect diagnostics in source with known issues"
        );

        // Verify LSP diagnostic structure
        let first = &diagnostics[0];
        assert_eq!(first.source, Some("solgrid".into()));
        assert!(first.severity.is_some());
        assert!(first.code.is_some());
    }

    #[test]
    fn test_lint_to_lsp_diagnostics_clean_file() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity 0.8.20;
contract Test {
    function good() public pure returns (uint256) {
        return 42;
    }
}
"#;
        let engine = LintEngine::new();
        let mut config = Config::default();
        // Disable some rules that might fire on this simple example
        config
            .lint
            .rules
            .insert("docs/natspec".into(), solgrid_config::RuleLevel::Off);
        let diagnostics = lint_to_lsp_diagnostics(&engine, source, Path::new("clean.sol"), &config);
        // Should not detect any security/naming issues on clean code
        let security_diags: Vec<_> = diagnostics
            .iter()
            .filter(|d| {
                matches!(&d.code, Some(ls_types::NumberOrString::String(id)) if id.starts_with("security/"))
            })
            .collect();
        assert!(
            security_diags.is_empty(),
            "clean source should have no security diagnostics, found: {:?}",
            security_diags.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_unresolved_import_produces_diagnostic() {
        let dir = tempfile::tempdir().unwrap();
        let importing = dir.path().join("Main.sol");
        fs::write(&importing, "").unwrap();

        let source = r#"import "./NonExistent.sol";"#;
        let resolver = ImportResolver::new(Some(dir.path().to_path_buf()));
        let diags = unresolved_import_diagnostics(source, &importing, &resolver);

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].severity, Some(ls_types::DiagnosticSeverity::ERROR));
        assert_eq!(
            diags[0].code,
            Some(ls_types::NumberOrString::String("unresolved-import".into()))
        );
        assert!(diags[0].message.contains("NonExistent.sol"));
    }

    #[test]
    fn test_resolved_import_no_diagnostic() {
        let dir = tempfile::tempdir().unwrap();
        let token_file = dir.path().join("Token.sol");
        fs::write(&token_file, "contract Token {}").unwrap();
        let importing = dir.path().join("Main.sol");
        fs::write(&importing, "").unwrap();

        let source = r#"import "./Token.sol";"#;
        let resolver = ImportResolver::new(Some(dir.path().to_path_buf()));
        let diags = unresolved_import_diagnostics(source, &importing, &resolver);

        assert!(diags.is_empty());
    }

    #[test]
    fn test_mixed_resolved_and_unresolved_imports() {
        let dir = tempfile::tempdir().unwrap();
        let token_file = dir.path().join("Token.sol");
        fs::write(&token_file, "contract Token {}").unwrap();
        let importing = dir.path().join("Main.sol");
        fs::write(&importing, "").unwrap();

        let source = "import \"./Token.sol\";\nimport \"./Missing.sol\";";
        let resolver = ImportResolver::new(Some(dir.path().to_path_buf()));
        let diags = unresolved_import_diagnostics(source, &importing, &resolver);

        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Missing.sol"));
    }

    #[test]
    fn test_unresolved_import_parse_failure_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let importing = dir.path().join("Bad.sol");
        fs::write(&importing, "").unwrap();

        let source = "this is not valid solidity {{{";
        let resolver = ImportResolver::new(Some(dir.path().to_path_buf()));
        let diags = unresolved_import_diagnostics(source, &importing, &resolver);

        assert!(diags.is_empty());
    }
}
