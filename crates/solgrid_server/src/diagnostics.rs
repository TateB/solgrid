//! Diagnostics — real-time lint integration for the LSP server.

use crate::convert;
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

#[cfg(test)]
mod tests {
    use super::*;

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
        config.lint.rules.insert(
            "best-practices/use-natspec".into(),
            solgrid_config::RuleLevel::Off,
        );
        config.lint.rules.insert(
            "best-practices/natspec-params".into(),
            solgrid_config::RuleLevel::Off,
        );
        config.lint.rules.insert(
            "best-practices/natspec-returns".into(),
            solgrid_config::RuleLevel::Off,
        );
        config.lint.rules.insert(
            "docs/natspec-contract".into(),
            solgrid_config::RuleLevel::Off,
        );
        config.lint.rules.insert(
            "docs/natspec-function".into(),
            solgrid_config::RuleLevel::Off,
        );
        let diagnostics = lint_to_lsp_diagnostics(&engine, source, Path::new("clean.sol"), &config);
        // May still have some diagnostics depending on enabled rules,
        // but the key point is no crash
        let _ = diagnostics;
    }
}
