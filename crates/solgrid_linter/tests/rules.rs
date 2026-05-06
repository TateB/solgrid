//! Integration tests for lint rules.

use solgrid_config::{Config, RuleLevel, RulePreset};
use solgrid_linter::testing::{
    assert_diagnostic_count, assert_no_diagnostics, fix_source, fix_source_unsafe,
    fix_source_unsafe_with_config, fix_source_with_config, lint_source_for_rule,
    lint_source_for_rule_with_config, lint_source_with_config,
};
use solgrid_linter::LintEngine;
use std::fs;

fn load_test_config(toml_str: &str) -> Config {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("solgrid.toml");
    fs::write(&path, toml_str).unwrap();
    solgrid_config::load_config(&path).unwrap()
}

fn table(entries: &[(&str, toml::Value)]) -> toml::Value {
    toml::Value::Table(
        entries
            .iter()
            .map(|(key, value)| ((*key).to_string(), value.clone()))
            .collect(),
    )
}

fn strings(values: &[&str]) -> toml::Value {
    toml::Value::Array(
        values
            .iter()
            .map(|value| toml::Value::String((*value).to_string()))
            .collect(),
    )
}

fn migration_natspec_config() -> Config {
    let mut config = Config::default();
    config
        .lint
        .rules
        .insert("docs/natspec".into(), RuleLevel::Info);
    config.lint.settings.insert(
        "docs/natspec".into(),
        table(&[
            ("comment_style", toml::Value::String("triple_slash".into())),
            ("continuation_indent", toml::Value::String("padded".into())),
            (
                "tags",
                table(&[
                    (
                        "notice",
                        table(&[
                            (
                                "include",
                                strings(&[
                                    "function:public",
                                    "function:external",
                                    "function:default",
                                    "variable:public",
                                    "event",
                                    "contract:concrete",
                                ]),
                            ),
                            ("exclude", strings(&["function:library"])),
                        ]),
                    ),
                    (
                        "dev",
                        table(&[(
                            "include",
                            strings(&[
                                "function:internal",
                                "function:private",
                                "function:library",
                                "variable:internal",
                                "variable:private",
                                "contract:abstract",
                                "contract:library",
                            ]),
                        )]),
                    ),
                    (
                        "param",
                        table(&[(
                            "exclude",
                            strings(&["function:internal", "function:private", "function:library"]),
                        )]),
                    ),
                    (
                        "return",
                        table(&[(
                            "exclude",
                            strings(&["function:internal", "function:private", "function:library"]),
                        )]),
                    ),
                ]),
            ),
        ]),
    );
    config
}

// =============================================================================
// Security rules
// =============================================================================

#[test]
fn test_tx_origin_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function bad() public {
        require(tx.origin == msg.sender);
    }
}
"#;
    assert_diagnostic_count(source, "security/tx-origin", 1);
}

#[test]
fn test_tx_origin_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function good() public {
        require(msg.sender != address(0));
    }
}
"#;
    assert_no_diagnostics(source, "security/tx-origin");
}

#[test]
fn test_avoid_sha3_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function bad() public pure returns (bytes32) {
        return sha3("hello");
    }
}
"#;
    assert_diagnostic_count(source, "security/avoid-sha3", 1);
}

#[test]
fn test_avoid_suicide_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function bad() public {
        suicide(msg.sender);
    }
}
"#;
    assert_diagnostic_count(source, "security/avoid-suicide", 1);
}

#[test]
fn test_low_level_calls_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function bad(address target) public {
        target.call("");
        target.delegatecall("");
    }
}
"#;
    let diagnostics = lint_source_for_rule(source, "security/low-level-calls");
    assert_eq!(diagnostics.len(), 2);
}

#[test]
fn test_no_inline_assembly_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function bad() public view returns (uint256 size) {
        assembly {
            size := extcodesize(address())
        }
    }
}
"#;
    assert_diagnostic_count(source, "security/no-inline-assembly", 1);
}

// =============================================================================
// Best practices rules
// =============================================================================

#[test]
fn test_no_console_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function debug() public {
        console.log("hello");
    }
}
"#;
    assert_diagnostic_count(source, "best-practices/no-console", 1);
}

#[test]
fn test_no_console_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function good() public pure returns (uint256) {
        return 42;
    }
}
"#;
    assert_no_diagnostics(source, "best-practices/no-console");
}

#[test]
fn test_explicit_types_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    uint public balance;
}
"#;
    assert_diagnostic_count(source, "best-practices/explicit-types", 1);
}

#[test]
fn test_explicit_types_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    uint256 public balance;
}
"#;
    assert_no_diagnostics(source, "best-practices/explicit-types");
}

// =============================================================================
// Naming rules
// =============================================================================

#[test]
fn test_contract_name_capwords_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract bad_name {}
"#;
    assert_diagnostic_count(source, "naming/contract-name-capwords", 1);
}

#[test]
fn test_contract_name_capwords_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract GoodName {}
"#;
    assert_no_diagnostics(source, "naming/contract-name-capwords");
}

#[test]
fn test_func_name_mixedcase_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function BadName() public {}
}
"#;
    assert_diagnostic_count(source, "naming/func-name-mixedcase", 1);
}

#[test]
fn test_func_name_mixedcase_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function goodName() public {}
}
"#;
    assert_no_diagnostics(source, "naming/func-name-mixedcase");
}

#[test]
fn test_func_name_mixedcase_internal_underscore_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function _goodName() internal {}
    function _otherGoodName() private {}
}
"#;
    assert_no_diagnostics(source, "naming/func-name-mixedcase");
}

#[test]
fn test_func_name_mixedcase_public_underscore_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function _goodName() public {}
}
"#;
    assert_diagnostic_count(source, "naming/func-name-mixedcase", 1);
}

#[test]
fn test_func_name_mixedcase_allowlist_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
interface Test {
    function ROOT_RESOURCE() external view returns (uint256);
}
"#;
    let mut config = Config::default();
    config.lint.preset = RulePreset::All;
    config.lint.settings.insert(
        "naming/func-name-mixedcase".into(),
        toml::toml! {
            allow = ["ROOT_RESOURCE"]
        }
        .into(),
    );

    let diagnostics =
        lint_source_for_rule_with_config(source, "naming/func-name-mixedcase", &config);
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
}

#[test]
fn test_func_name_mixedcase_allow_regex_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
interface Test {
    function ABI(bytes32 node, uint256 contentTypes) external view returns (bytes memory);
}
"#;
    let mut config = Config::default();
    config.lint.preset = RulePreset::All;
    config.lint.settings.insert(
        "naming/func-name-mixedcase".into(),
        toml::toml! {
            allow_regex = "^[A-Z][A-Z0-9_]*$"
        }
        .into(),
    );

    let diagnostics =
        lint_source_for_rule_with_config(source, "naming/func-name-mixedcase", &config);
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
}

#[test]
fn test_func_name_mixedcase_public_abi_names_clean_by_default() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
interface ITest {
    function ROOT_RESOURCE() external view returns (uint256);
}
contract Test {
    function ABI(bytes32 node, uint256 contentTypes)
        external
        pure
        returns (uint256, bytes memory)
    {
        return (0, "");
    }
}
"#;
    assert_no_diagnostics(source, "naming/func-name-mixedcase");
}

#[test]
fn test_func_name_mixedcase_public_abi_names_can_be_disabled() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
interface ITest {
    function ROOT_RESOURCE() external view returns (uint256);
}
"#;
    let mut config = Config::default();
    config.lint.preset = RulePreset::All;
    config.lint.settings.insert(
        "naming/func-name-mixedcase".into(),
        toml::toml! {
            allow_public_abi = false
        }
        .into(),
    );

    let diagnostics =
        lint_source_for_rule_with_config(source, "naming/func-name-mixedcase", &config);
    assert_eq!(diagnostics.len(), 1, "{diagnostics:#?}");
}

#[test]
fn test_const_name_snakecase_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    uint256 constant badName = 42;
}
"#;
    assert_diagnostic_count(source, "naming/const-name-snakecase", 1);
}

#[test]
fn test_const_name_snakecase_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    uint256 constant MAX_SUPPLY = 42;
}
"#;
    assert_no_diagnostics(source, "naming/const-name-snakecase");
}

// =============================================================================
// Auto-fix tests
// =============================================================================

#[test]
fn test_fix_sha3_to_keccak256() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function hash() public pure returns (bytes32) {
        return sha3("hello");
    }
}
"#;
    let fixed = fix_source(source);
    assert!(
        fixed.contains("keccak256("),
        "Expected sha3 to be replaced with keccak256"
    );
    assert!(!fixed.contains("sha3("), "Expected sha3 to be removed");
}

#[test]
fn test_fix_suicide_to_selfdestruct() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function destroy() public {
        suicide(msg.sender);
    }
}
"#;
    let fixed = fix_source(source);
    assert!(
        fixed.contains("selfdestruct"),
        "Expected suicide to be replaced with selfdestruct"
    );
    assert!(
        !fixed.contains("suicide("),
        "Expected suicide to be removed"
    );
}

// =============================================================================
// Suppression tests
// =============================================================================

#[test]
fn test_suppression_next_line() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function bad() public {
        // solgrid-disable-next-line security/tx-origin
        require(tx.origin == msg.sender);
    }
}
"#;
    assert_no_diagnostics(source, "security/tx-origin");
}

// =============================================================================
// New Security rules (Chunk 5)
// =============================================================================

#[test]
fn test_avoid_selfdestruct_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function destroy(address payable to) public {
        selfdestruct(to);
    }
}
"#;
    assert_diagnostic_count(source, "security/avoid-selfdestruct", 1);
}

#[test]
fn test_avoid_selfdestruct_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function safe() public pure returns (uint256) {
        return 42;
    }
}
"#;
    assert_no_diagnostics(source, "security/avoid-selfdestruct");
}

#[test]
fn test_compiler_version_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.4.0;
contract Test {}
"#;
    assert_diagnostic_count(source, "security/compiler-version", 1);
}

#[test]
fn test_compiler_version_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {}
"#;
    assert_no_diagnostics(source, "security/compiler-version");
}

#[test]
fn test_not_rely_on_block_hash_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function bad() public view returns (bytes32) {
        return blockhash(block.number - 1);
    }
}
"#;
    assert_diagnostic_count(source, "security/not-rely-on-block-hash", 1);
}

#[test]
fn test_not_rely_on_block_hash_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function good() public pure returns (uint256) {
        return 42;
    }
}
"#;
    assert_no_diagnostics(source, "security/not-rely-on-block-hash");
}

#[test]
fn test_not_rely_on_time_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function bad() public view returns (uint256) {
        return block.timestamp;
    }
}
"#;
    assert_diagnostic_count(source, "security/not-rely-on-time", 1);
}

#[test]
fn test_not_rely_on_time_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function good() public pure returns (uint256) {
        return 42;
    }
}
"#;
    assert_no_diagnostics(source, "security/not-rely-on-time");
}

#[test]
fn test_multiple_sends_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function bad(address payable a, address payable b) public {
        a.send(1 ether);
        b.send(1 ether);
    }
}
"#;
    assert_diagnostic_count(source, "security/multiple-sends", 1);
}

#[test]
fn test_multiple_sends_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function good(address payable a) public {
        a.send(1 ether);
    }
}
"#;
    assert_no_diagnostics(source, "security/multiple-sends");
}

#[test]
fn test_payable_fallback_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    fallback() external {}
}
"#;
    assert_diagnostic_count(source, "security/payable-fallback", 1);
}

#[test]
fn test_payable_fallback_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    fallback() external payable {}
    receive() external payable {}
}
"#;
    assert_no_diagnostics(source, "security/payable-fallback");
}

#[test]
fn test_no_delegatecall_in_loop_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function bad(address[] calldata targets) public {
        for (uint256 i = 0; i < targets.length; i++) {
            targets[i].delegatecall("");
        }
    }
}
"#;
    assert_diagnostic_count(source, "security/no-delegatecall-in-loop", 1);
}

#[test]
fn test_no_delegatecall_in_loop_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function good(address target) public {
        target.delegatecall("");
    }
}
"#;
    assert_no_diagnostics(source, "security/no-delegatecall-in-loop");
}

#[test]
fn test_msg_value_in_loop_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function bad(address[] calldata targets) public payable {
        for (uint256 i = 0; i < targets.length; i++) {
            require(msg.value > 0);
        }
    }
}
"#;
    assert_diagnostic_count(source, "security/msg-value-in-loop", 1);
}

#[test]
fn test_msg_value_in_loop_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function good() public payable {
        require(msg.value > 0);
    }
}
"#;
    assert_no_diagnostics(source, "security/msg-value-in-loop");
}

#[test]
fn test_arbitrary_send_eth_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function bad(address payable to) public {
        to.transfer(1 ether);
    }
}
"#;
    let diagnostics = lint_source_for_rule(source, "security/arbitrary-send-eth");
    assert!(
        !diagnostics.is_empty(),
        "Expected at least 1 diagnostic for arbitrary-send-eth"
    );
}

#[test]
fn test_divide_before_multiply_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function bad(uint256 a, uint256 b, uint256 c) public pure returns (uint256) {
        return a / b * c;
    }
}
"#;
    assert_diagnostic_count(source, "security/divide-before-multiply", 1);
}

#[test]
fn test_divide_before_multiply_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function good(uint256 a, uint256 b, uint256 c) public pure returns (uint256) {
        return a * b / c;
    }
}
"#;
    assert_no_diagnostics(source, "security/divide-before-multiply");
}

// =============================================================================
// New Best Practices rules (Chunk 6)
// =============================================================================

#[test]
fn test_function_max_lines_detected() {
    // Create a function with more than 50 lines
    let mut body_lines = String::new();
    for i in 0..55 {
        body_lines.push_str(&format!("        uint256 x{i} = {i};\n"));
    }
    let source = format!(
        r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {{
    function tooLong() public pure {{
{body_lines}    }}
}}
"#
    );
    assert_diagnostic_count(&source, "best-practices/function-max-lines", 1);
}

#[test]
fn test_function_max_lines_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function shortFunc() public pure returns (uint256) {
        return 42;
    }
}
"#;
    assert_no_diagnostics(source, "best-practices/function-max-lines");
}

#[test]
fn test_max_states_count_detected() {
    let mut vars = String::new();
    for i in 0..20 {
        vars.push_str(&format!("    uint256 public var{i};\n"));
    }
    let source = format!(
        r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {{
{vars}}}
"#
    );
    assert_diagnostic_count(&source, "best-practices/max-states-count", 1);
}

#[test]
fn test_max_states_count_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    uint256 public a;
    uint256 public b;
    uint256 public c;
}
"#;
    assert_no_diagnostics(source, "best-practices/max-states-count");
}

#[test]
fn test_one_contract_per_file_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract First {}
contract Second {}
"#;
    assert_diagnostic_count(source, "best-practices/one-contract-per-file", 1);
}

#[test]
fn test_one_contract_per_file_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract OnlyOne {}
"#;
    assert_no_diagnostics(source, "best-practices/one-contract-per-file");
}

#[test]
fn test_no_global_import_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import "foo.sol";
contract Test {}
"#;
    assert_diagnostic_count(source, "best-practices/no-global-import", 1);
}

#[test]
fn test_no_global_import_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import {Foo} from "foo.sol";
contract Test {}
"#;
    assert_no_diagnostics(source, "best-practices/no-global-import");
}

#[test]
fn test_reason_string_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function bad(uint256 x) public pure {
        require(x > 0);
    }
}
"#;
    assert_diagnostic_count(source, "best-practices/reason-string", 1);
}

#[test]
fn test_reason_string_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function good(uint256 x) public pure {
        require(x > 0, "x must be positive");
    }
}
"#;
    assert_no_diagnostics(source, "best-practices/reason-string");
}

#[test]
fn test_custom_errors_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function bad(uint256 x) public pure {
        require(x > 0, "x must be positive");
    }
}
"#;
    assert_diagnostic_count(source, "best-practices/custom-errors", 1);
}

#[test]
fn test_custom_errors_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    error InvalidAmount();
    function good(uint256 x) public pure {
        if (x == 0) revert InvalidAmount();
    }
}
"#;
    assert_no_diagnostics(source, "best-practices/custom-errors");
}

#[test]
fn test_custom_errors_reports_once_with_default_config() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity 0.8.24;
contract Test {
    function bad(uint256 x) public pure {
        require(x > 0, "x must be positive");
    }
}
"#;
    let diagnostics = lint_source_with_config(source, &Config::default());
    assert_eq!(
        diagnostics
            .iter()
            .filter(|diag| diag.rule_id == "best-practices/custom-errors")
            .count(),
        1
    );
    assert_eq!(
        diagnostics
            .iter()
            .filter(|diag| diag.rule_id == "gas/custom-errors")
            .count(),
        0
    );
}

#[test]
fn test_no_floating_pragma_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {}
"#;
    assert_diagnostic_count(source, "best-practices/no-floating-pragma", 1);
}

#[test]
fn test_no_floating_pragma_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity 0.8.20;
contract Test {}
"#;
    assert_no_diagnostics(source, "best-practices/no-floating-pragma");
}

#[test]
fn test_imports_on_top_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract First {}
import "late.sol";
"#;
    assert_diagnostic_count(source, "best-practices/imports-on-top", 1);
}

#[test]
fn test_imports_on_top_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import "early.sol";
contract Test {}
"#;
    assert_no_diagnostics(source, "best-practices/imports-on-top");
}

#[test]
fn test_code_complexity_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function complex(uint256 a, uint256 b, uint256 c, uint256 d) public pure returns (uint256) {
        if (a > 0) {
            if (b > 0) {
                if (c > 0) {
                    if (d > 0) {
                        for (uint256 i = 0; i < a; i++) {
                            if (i > b) {
                                while (c > 0) {
                                    if (c > d) {
                                        c--;
                                    } else if (c == d) {
                                        c -= 2;
                                    } else {
                                        c -= 3;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        if (a == b) {
            if (c == d) {
                return b;
            }
        }
        return a;
    }
}
"#;
    assert_diagnostic_count(source, "best-practices/code-complexity", 1);
}

#[test]
fn test_code_complexity_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function simple(uint256 a) public pure returns (uint256) {
        if (a > 0) {
            return a;
        }
        return 0;
    }
}
"#;
    assert_no_diagnostics(source, "best-practices/code-complexity");
}

#[test]
fn test_no_unused_error_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    error UnusedError();
    function good() public pure {}
}
"#;
    assert_diagnostic_count(source, "best-practices/no-unused-error", 1);
}

#[test]
fn test_no_unused_error_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    error MyError();
    function bad() public pure {
        revert MyError();
    }
}
"#;
    assert_no_diagnostics(source, "best-practices/no-unused-error");
}

#[test]
fn test_no_unused_event_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    event UnusedEvent();
    function good() public pure {}
}
"#;
    assert_diagnostic_count(source, "best-practices/no-unused-event", 1);
}

#[test]
fn test_no_unused_event_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    event MyEvent();
    function good() public {
        emit MyEvent();
    }
}
"#;
    assert_no_diagnostics(source, "best-practices/no-unused-event");
}

// =============================================================================
// New Naming rules (Chunk 7)
// =============================================================================

#[test]
fn test_interface_starts_with_i_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
interface BadInterface {
    function foo() external;
}
"#;
    assert_diagnostic_count(source, "naming/interface-starts-with-i", 1);
}

#[test]
fn test_interface_starts_with_i_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
interface IGoodInterface {
    function foo() external;
}
"#;
    assert_no_diagnostics(source, "naming/interface-starts-with-i");
}

#[test]
fn test_library_name_capwords_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
library bad_lib {
    function foo() internal pure returns (uint256) { return 1; }
}
"#;
    assert_diagnostic_count(source, "naming/library-name-capwords", 1);
}

#[test]
fn test_library_name_capwords_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
library GoodLib {
    function foo() internal pure returns (uint256) { return 1; }
}
"#;
    assert_no_diagnostics(source, "naming/library-name-capwords");
}

#[test]
fn test_struct_name_capwords_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    struct bad_struct { uint256 x; }
}
"#;
    assert_diagnostic_count(source, "naming/struct-name-capwords", 1);
}

#[test]
fn test_struct_name_capwords_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    struct GoodStruct { uint256 x; }
}
"#;
    assert_no_diagnostics(source, "naming/struct-name-capwords");
}

#[test]
fn test_enum_name_capwords_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    enum bad_enum { A, B }
}
"#;
    assert_diagnostic_count(source, "naming/enum-name-capwords", 1);
}

#[test]
fn test_enum_name_capwords_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    enum GoodEnum { A, B }
}
"#;
    assert_no_diagnostics(source, "naming/enum-name-capwords");
}

#[test]
fn test_event_name_capwords_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    event bad_event(uint256 x);
}
"#;
    assert_diagnostic_count(source, "naming/event-name-capwords", 1);
}

#[test]
fn test_event_name_capwords_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    event GoodEvent(uint256 x);
}
"#;
    assert_no_diagnostics(source, "naming/event-name-capwords");
}

#[test]
fn test_error_name_capwords_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    error bad_error();
}
"#;
    assert_diagnostic_count(source, "naming/error-name-capwords", 1);
}

#[test]
fn test_error_name_capwords_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    error GoodError();
}
"#;
    assert_no_diagnostics(source, "naming/error-name-capwords");
}

#[test]
fn test_param_name_mixedcase_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function bad(uint256 BadParam) public pure returns (uint256) {
        return BadParam;
    }
}
"#;
    assert_diagnostic_count(source, "naming/param-name-mixedcase", 1);
}

#[test]
fn test_param_name_mixedcase_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function good(uint256 goodParam) public pure returns (uint256) {
        return goodParam;
    }
}
"#;
    assert_no_diagnostics(source, "naming/param-name-mixedcase");
}

#[test]
fn test_named_parameters_mapping_detects_missing_regular_names() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.18;
contract Test {
    mapping(address => uint256) public balances;
}
"#;
    assert_diagnostic_count(source, "naming/named-parameters-mapping", 2);
}

#[test]
fn test_named_parameters_mapping_only_requires_outer_key_for_nested_mappings() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.18;
contract Test {
    mapping(address => mapping(address token => uint256 balance)) public balances;
}
"#;
    assert_diagnostic_count(source, "naming/named-parameters-mapping", 1);
}

#[test]
fn test_named_parameters_mapping_allows_missing_nested_mapping_names() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.18;
contract Test {
    mapping(address owner => mapping(address => uint256 balance)) public balances;
}
"#;
    assert_no_diagnostics(source, "naming/named-parameters-mapping");
}

#[test]
fn test_named_parameters_mapping_detects_mapping_typed_function_parameters() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.18;
library TestLib {
    function configure(mapping(address => uint256) storage balances) internal {
        balances[address(0)] = 1;
    }
}
"#;
    assert_diagnostic_count(source, "naming/named-parameters-mapping", 2);
}

#[test]
fn test_named_parameters_mapping_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.18;
contract Test {
    mapping(address owner => mapping(address token => uint256 balance)) public balances;
}
"#;
    assert_no_diagnostics(source, "naming/named-parameters-mapping");
}

#[test]
fn test_var_name_mixedcase_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function bad() public pure returns (uint256) {
        uint256 BadVar = 42;
        return BadVar;
    }
}
"#;
    assert_diagnostic_count(source, "naming/var-name-mixedcase", 1);
}

#[test]
fn test_var_name_mixedcase_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function good() public pure returns (uint256) {
        uint256 goodVar = 42;
        return goodVar;
    }
}
"#;
    assert_no_diagnostics(source, "naming/var-name-mixedcase");
}

#[test]
fn test_immutable_name_snakecase_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    uint256 immutable badName;
    constructor() { badName = 42; }
}
"#;
    assert_diagnostic_count(source, "naming/immutable-name-snakecase", 1);
}

#[test]
fn test_immutable_name_snakecase_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    uint256 immutable GOOD_NAME;
    constructor() { GOOD_NAME = 42; }
}
"#;
    assert_no_diagnostics(source, "naming/immutable-name-snakecase");
}

#[test]
fn test_private_vars_underscore_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    uint256 private badName;
}
"#;
    assert_diagnostic_count(source, "naming/private-vars-underscore", 1);
}

#[test]
fn test_private_vars_underscore_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    uint256 private _goodName;
}
"#;
    assert_no_diagnostics(source, "naming/private-vars-underscore");
}

#[test]
fn test_modifier_name_mixedcase_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    modifier BadModifier() { _; }
}
"#;
    assert_diagnostic_count(source, "naming/modifier-name-mixedcase", 1);
}

#[test]
fn test_modifier_name_mixedcase_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    modifier goodModifier() { _; }
}
"#;
    assert_no_diagnostics(source, "naming/modifier-name-mixedcase");
}

#[test]
fn test_type_name_capwords_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
type bad_type is uint256;
"#;
    assert_diagnostic_count(source, "naming/type-name-capwords", 1);
}

#[test]
fn test_type_name_capwords_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
type GoodType is uint256;
"#;
    assert_no_diagnostics(source, "naming/type-name-capwords");
}

#[test]
fn test_foundry_test_functions_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function testbadname() public {}
}
"#;
    assert_diagnostic_count(source, "naming/foundry-test-functions", 1);
}

#[test]
fn test_foundry_test_functions_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function testTransfer() public {}
    function testFuzzAmount(uint256 amount) public {}
    function test_something() public {}
}
"#;
    assert_no_diagnostics(source, "naming/foundry-test-functions");
}

// =============================================================================
// Deferred Security rules
// =============================================================================

#[test]
fn test_reentrancy_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    uint256 public balance;
    function withdraw() public {
        (bool success, ) = msg.sender.call{value: balance}("");
        balance = 0;
    }
}
"#;
    assert_diagnostic_count(source, "security/reentrancy", 1);
}

#[test]
fn test_reentrancy_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    uint256 public balance;
    function withdraw() public {
        balance = 0;
        (bool success, ) = msg.sender.call{value: balance}("");
    }
}
"#;
    assert_no_diagnostics(source, "security/reentrancy");
}

#[test]
fn test_uninitialized_storage_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    struct Data { uint256 value; }
    Data[] public items;
    function bad() public {
        Data storage item;
    }
}
"#;
    assert_diagnostic_count(source, "security/uninitialized-storage", 1);
}

#[test]
fn test_uninitialized_storage_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    struct Data { uint256 value; }
    Data[] public items;
    function good() public {
        Data storage item = items[0];
    }
}
"#;
    assert_no_diagnostics(source, "security/uninitialized-storage");
}

// =============================================================================
// Deferred Best Practices rules
// =============================================================================

#[test]
fn test_constructor_syntax_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract MyContract {
    function MyContract() public {}
}
"#;
    assert_diagnostic_count(source, "best-practices/constructor-syntax", 1);
}

#[test]
fn test_constructor_syntax_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract MyContract {
    constructor() {}
}
"#;
    assert_no_diagnostics(source, "best-practices/constructor-syntax");
}

#[test]
fn test_docs_natspec_detected_for_missing_function_tags() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
/// @notice Test contract
contract Test {
    function doSomething(uint256 x) public pure returns (uint256) {
        return x;
    }
}
"#;
    assert_diagnostic_count(source, "docs/natspec", 1);
}

#[test]
fn test_docs_natspec_clean_for_function_tags() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
/// @notice Test contract
contract Test {
    /// @notice Does something useful
    /// @param x The input value
    /// @return result The output value
    function doSomething(uint256 x) public pure returns (uint256 result) {
        return x;
    }
}
"#;
    assert_no_diagnostics(source, "docs/natspec");
}

#[test]
fn test_docs_natspec_fixes_block_comment_to_triple_slash() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    /**
     * @notice Does something useful
     */
    function doSomething() public {}
}
"#;
    let fixed = fix_source(source);
    assert!(fixed.contains("/// @notice Does something useful"));
    assert!(!fixed.contains("/**"));
}

#[test]
fn test_docs_natspec_simple_getter_forbids_return_tag() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
/// @notice Test contract
contract Test {
    /// @notice Reads the value
    /// @return currentValue The stored value
    function value() public view returns (uint256 currentValue) {
        return 1;
    }
}
"#;
    assert_diagnostic_count(source, "docs/natspec", 1);
}

#[test]
fn test_docs_natspec_disabled_by_default_config() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity 0.8.24;
contract Test {
    function doSomething() public pure returns (uint256) {
        return 42;
    }
}
"#;
    let diagnostics = lint_source_with_config(source, &Config::default());
    let natspec_diagnostics: Vec<_> = diagnostics
        .iter()
        .filter(|diag| diag.rule_id == "docs/natspec")
        .collect();
    assert!(natspec_diagnostics.is_empty());
}

#[test]
fn test_docs_natspec_inheritdoc_skips_missing_tags() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
/// @notice Interface docs
interface ITest {
    /// @notice Does foo
    /// @param x Input value
    /// @return result Output value
    function foo(uint256 x) external returns (uint256 result);
}

/// @notice Implementation docs
contract Test is ITest {
    /// @inheritdoc ITest
    function foo(uint256 x) external returns (uint256) {
        return x;
    }
}
"#;
    assert_no_diagnostics(source, "docs/natspec");
}

#[test]
fn test_docs_natspec_respects_visibility_context_filters() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
/// @notice Alias resolver
contract Test {
    /// @dev Apply one round of aliasing.
    /// @param fromName The source DNS-encoded name.
    /// @return matchName The alias that matched.
    /// @return toName The destination DNS-encoded name or empty if no match.
    function _resolveAlias(bytes memory fromName)
        internal
        view
        returns (bytes memory matchName, bytes memory toName)
    {
        return (fromName, fromName);
    }
}
"#;
    let diagnostics =
        lint_source_for_rule_with_config(source, "docs/natspec", &migration_natspec_config());
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn test_docs_natspec_respects_library_function_context_filters() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
/// @dev Shared math helpers.
library TestLib {
    /// @dev Clamp a value to the provided upper bound.
    function clamp(uint256 value, uint256 maxValue) public pure returns (uint256) {
        return value > maxValue ? maxValue : value;
    }
}
"#;
    let diagnostics =
        lint_source_for_rule_with_config(source, "docs/natspec", &migration_natspec_config());
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn test_visibility_modifier_order_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function bad() pure public returns (uint256) {
        return 42;
    }
}
"#;
    assert_diagnostic_count(source, "best-practices/visibility-modifier-order", 1);
}

#[test]
fn test_visibility_modifier_order_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function good() public pure returns (uint256) {
        return 42;
    }
}
"#;
    assert_no_diagnostics(source, "best-practices/visibility-modifier-order");
}

#[test]
fn test_no_unused_imports_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import {Unused} from "some.sol";
contract Test {}
"#;
    assert_diagnostic_count(source, "best-practices/no-unused-imports", 1);
}

#[test]
fn test_no_unused_imports_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import {IERC20} from "some.sol";
contract Test {
    IERC20 public token;
}
"#;
    assert_no_diagnostics(source, "best-practices/no-unused-imports");
}

#[test]
fn test_duplicated_imports_detects_inline_duplicates() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import {Token, Token as Tkn} from "./Token.sol";
contract Test {}
"#;
    assert_diagnostic_count(source, "best-practices/duplicated-imports", 1);
}

#[test]
fn test_duplicated_imports_detects_same_path_duplicates() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import {Token} from "./Token.sol";
import {Token as TokenAlias} from "./Token.sol";
contract Test {}
"#;
    assert_diagnostic_count(source, "best-practices/duplicated-imports", 1);
}

#[test]
fn test_duplicated_imports_detects_cross_path_unaliased_duplicates() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import {SharedLib} from "./LibraryA.sol";
import {SharedLib} from "./LibraryB.sol";
contract Test {}
"#;
    assert_diagnostic_count(source, "best-practices/duplicated-imports", 1);
}

#[test]
fn test_duplicated_imports_allows_cross_path_aliases() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import {SharedLib} from "./LibraryA.sol";
import {SharedLib as SharedLibB} from "./LibraryB.sol";
contract Test {}
"#;
    assert_no_diagnostics(source, "best-practices/duplicated-imports");
}

#[test]
fn test_duplicated_imports_detects_cross_path_alias_duplicates() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import {Foo as SharedLib} from "./LibraryA.sol";
import {Bar as SharedLib} from "./LibraryB.sol";
contract Test {}
"#;
    assert_diagnostic_count(source, "best-practices/duplicated-imports", 1);
}

#[test]
fn test_duplicated_imports_detects_cross_path_plain_import_duplicates() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import "./LibraryA.sol";
import {LibraryA} from "./other/LibraryA.sol";
contract Test {}
"#;
    assert_diagnostic_count(source, "best-practices/duplicated-imports", 1);
}

#[test]
fn test_duplicated_imports_ignores_namespace_imports_from_same_path() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import * as LibraryA from "./LibraryA.sol";
import * as LibraryAAgain from "./LibraryA.sol";
contract Test {}
"#;
    assert_no_diagnostics(source, "best-practices/duplicated-imports");
}

#[test]
fn test_duplicated_imports_ignores_namespace_imports_from_different_paths() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import * as LibraryA from "./LibraryA.sol";
import * as LibraryB from "./LibraryB.sol";
contract Test {}
"#;
    assert_no_diagnostics(source, "best-practices/duplicated-imports");
}

#[test]
fn test_no_unused_state_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    uint256 private unusedVar;
    function good() public pure returns (uint256) {
        return 42;
    }
}
"#;
    assert_diagnostic_count(source, "best-practices/no-unused-state", 1);
}

#[test]
fn test_no_unused_state_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    uint256 private usedVar;
    function good() public view returns (uint256) {
        return usedVar;
    }
}
"#;
    assert_no_diagnostics(source, "best-practices/no-unused-state");
}

#[test]
fn test_no_unused_state_skips_public() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    uint256 public notUsedButPublic;
}
"#;
    assert_no_diagnostics(source, "best-practices/no-unused-state");
}

#[test]
fn test_no_unused_vars_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function bad() public pure returns (uint256) {
        uint256 unused = 42;
        return 0;
    }
}
"#;
    assert_diagnostic_count(source, "best-practices/no-unused-vars", 1);
}

#[test]
fn test_no_unused_vars_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function good() public pure returns (uint256) {
        uint256 used = 42;
        return used;
    }
}
"#;
    assert_no_diagnostics(source, "best-practices/no-unused-vars");
}

#[test]
fn test_no_unused_vars_underscore_prefix() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function good() public pure returns (uint256) {
        uint256 _ignored = 42;
        return 0;
    }
}
"#;
    assert_no_diagnostics(source, "best-practices/no-unused-vars");
}

// =============================================================================
// Gas Optimization rules (Chunk 8)
// =============================================================================

#[test]
fn test_bool_storage_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    bool public paused;
}
"#;
    assert_diagnostic_count(source, "gas/bool-storage", 1);
}

#[test]
fn test_bool_storage_span_covers_bool_keyword() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    bool public paused;
}
"#;
    let diagnostics = lint_source_for_rule(source, "gas/bool-storage");
    assert_eq!(diagnostics.len(), 1);
    let span = &diagnostics[0].span;
    assert_eq!(
        &source[span.clone()],
        "bool",
        "span should cover 'bool' keyword, not whitespace"
    );
}

#[test]
fn test_bool_storage_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    uint256 public paused;
}
"#;
    assert_no_diagnostics(source, "gas/bool-storage");
}

#[test]
fn test_increment_by_one_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function bad() public pure {
        uint256 i = 0;
        i += 1;
    }
}
"#;
    assert_diagnostic_count(source, "gas/increment-by-one", 1);
}

#[test]
fn test_increment_by_one_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function good() public pure {
        uint256 i = 0;
        ++i;
    }
}
"#;
    assert_no_diagnostics(source, "gas/increment-by-one");
}

#[test]
fn test_increment_by_one_not_triggered_by_larger() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function good() public pure {
        uint256 i = 0;
        i += 10;
    }
}
"#;
    assert_no_diagnostics(source, "gas/increment-by-one");
}

#[test]
fn test_fix_increment_by_one() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function bad() public pure {
        uint256 i = 0;
        i += 1;
    }
}
"#;
    let fixed = fix_source(source);
    assert!(
        fixed.contains("++i"),
        "Expected `i += 1` to be replaced with `++i`"
    );
}

#[test]
fn test_cache_array_length_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    uint256[] public arr;
    function bad() public view {
        for (uint256 i = 0; i < arr.length; i++) {
        }
    }
}
"#;
    assert_diagnostic_count(source, "gas/cache-array-length", 1);
}

#[test]
fn test_cache_array_length_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    uint256[] public arr;
    function good() public view {
        uint256 len = arr.length;
        for (uint256 i = 0; i < len; i++) {
        }
    }
}
"#;
    assert_no_diagnostics(source, "gas/cache-array-length");
}

#[test]
fn test_unchecked_increment_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function bad() public pure {
        for (uint256 i = 0; i < 10; i++) {
        }
    }
}
"#;
    assert_diagnostic_count(source, "gas/unchecked-increment", 1);
}

#[test]
fn test_unchecked_increment_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function good() public pure {
        for (uint256 i = 0; i < 10; ) {
            unchecked { ++i; }
        }
    }
}
"#;
    assert_no_diagnostics(source, "gas/unchecked-increment");
}

#[test]
fn test_small_strings_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function bad(uint256 x) public pure {
        require(x > 0, "This is a very long error message that exceeds thirty-two bytes of length easily");
    }
}
"#;
    assert_diagnostic_count(source, "gas/small-strings", 1);
}

#[test]
fn test_small_strings_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function good(uint256 x) public pure {
        require(x > 0, "too low");
    }
}
"#;
    assert_no_diagnostics(source, "gas/small-strings");
}

#[test]
fn test_gas_custom_errors_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function bad(uint256 x) public pure {
        require(x > 0, "too low");
    }
}
"#;
    let mut config = Config::default();
    config.lint.preset = solgrid_config::RulePreset::All;
    config
        .lint
        .rules
        .insert("best-practices/custom-errors".into(), RuleLevel::Off);
    let diagnostics = lint_source_with_config(source, &config);
    assert_eq!(
        diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.rule_id == "gas/custom-errors")
            .count(),
        1
    );
}

#[test]
fn test_gas_custom_errors_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    error TooLow();
    function good(uint256 x) public pure {
        if (x == 0) revert TooLow();
    }
}
"#;
    assert_no_diagnostics(source, "gas/custom-errors");
}

#[test]
fn test_gas_custom_errors_only_when_best_practices_rule_is_disabled() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity 0.8.24;
contract Test {
    function bad(uint256 x) public pure {
        require(x > 0, "too low");
    }
}
"#;
    let mut config = Config::default();
    config
        .lint
        .rules
        .insert("best-practices/custom-errors".into(), RuleLevel::Off);
    config
        .lint
        .rules
        .insert("gas/custom-errors".into(), RuleLevel::Info);

    let diagnostics = lint_source_with_config(source, &config);
    assert_eq!(
        diagnostics
            .iter()
            .filter(|diag| diag.rule_id == "gas/custom-errors")
            .count(),
        1
    );
}

#[test]
fn test_use_bytes32_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    string public name = "MyToken";
}
"#;
    assert_diagnostic_count(source, "gas/use-bytes32", 1);
}

#[test]
fn test_use_bytes32_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    bytes32 public name;
}
"#;
    assert_no_diagnostics(source, "gas/use-bytes32");
}

#[test]
fn test_calldata_parameters_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function bad(string memory name) external pure returns (bytes32) {
        return keccak256(bytes(name));
    }
}
"#;
    assert_diagnostic_count(source, "gas/calldata-parameters", 1);
}

#[test]
fn test_calldata_parameters_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function good(string calldata name) external pure returns (bytes32) {
        return keccak256(bytes(name));
    }
}
"#;
    assert_no_diagnostics(source, "gas/calldata-parameters");
}

#[test]
fn test_calldata_parameters_internal_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function _internal(string memory name) internal pure returns (bytes32) {
        return keccak256(bytes(name));
    }
}
"#;
    assert_no_diagnostics(source, "gas/calldata-parameters");
}

#[test]
fn test_fix_calldata_parameters() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function bad(string memory name) external pure returns (bytes32) {
        return keccak256(bytes(name));
    }
}
"#;
    let fixed = fix_source(source);
    assert!(
        fixed.contains("calldata"),
        "Expected `memory` to be replaced with `calldata`"
    );
}

#[test]
fn test_indexed_events_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    event Transfer(address from, address to, uint256 amount);
}
"#;
    let diagnostics = lint_source_for_rule(source, "gas/indexed-events");
    assert!(
        !diagnostics.is_empty(),
        "Expected at least 1 diagnostic for indexed-events, got 0",
    );
}

#[test]
fn test_indexed_events_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    event Transfer(address indexed from, address indexed to, uint256 indexed amount);
}
"#;
    assert_no_diagnostics(source, "gas/indexed-events");
}

#[test]
fn test_named_return_values_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function bad() public pure returns (uint256) {
        return 42;
    }
}
"#;
    assert_diagnostic_count(source, "gas/named-return-values", 1);
}

#[test]
fn test_named_return_values_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function good() public pure returns (uint256 amount) {
        amount = 42;
    }
}
"#;
    assert_no_diagnostics(source, "gas/named-return-values");
}

#[test]
fn test_named_return_values_no_returns_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function good() public pure {}
}
"#;
    assert_no_diagnostics(source, "gas/named-return-values");
}

#[test]
fn test_use_constant_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    uint256 public MAX_SUPPLY = 1000000;
    function get() public view returns (uint256) {
        return MAX_SUPPLY;
    }
}
"#;
    assert_diagnostic_count(source, "gas/use-constant", 1);
}

#[test]
fn test_use_constant_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    uint256 public constant MAX_SUPPLY = 1000000;
}
"#;
    assert_no_diagnostics(source, "gas/use-constant");
}

#[test]
fn test_use_immutable_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    address public owner;
    constructor() {
        owner = msg.sender;
    }
}
"#;
    assert_diagnostic_count(source, "gas/use-immutable", 1);
}

#[test]
fn test_use_immutable_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    address public immutable owner;
    constructor() {
        owner = msg.sender;
    }
}
"#;
    assert_no_diagnostics(source, "gas/use-immutable");
}

#[test]
fn test_use_immutable_assigned_elsewhere_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    address public owner;
    constructor() {
        owner = msg.sender;
    }
    function transferOwnership(address newOwner) public {
        owner = newOwner;
    }
}
"#;
    assert_no_diagnostics(source, "gas/use-immutable");
}

#[test]
fn test_no_redundant_sload_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    uint256 public totalSupply;
    function bad() public view returns (uint256) {
        uint256 a = totalSupply;
        uint256 b = totalSupply;
        return a + b;
    }
}
"#;
    assert_diagnostic_count(source, "gas/no-redundant-sload", 1);
}

#[test]
fn test_no_redundant_sload_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    uint256 public totalSupply;
    function good() public view returns (uint256) {
        uint256 cached = totalSupply;
        return cached + cached;
    }
}
"#;
    // totalSupply is only read once in the function
    assert_no_diagnostics(source, "gas/no-redundant-sload");
}

#[test]
fn test_struct_packing_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    struct Bad {
        uint256 a;
        uint8 b;
        uint256 c;
        uint8 d;
    }
}
"#;
    assert_diagnostic_count(source, "gas/struct-packing", 1);
}

#[test]
fn test_struct_packing_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    struct Good {
        uint8 b;
        uint8 d;
        uint256 a;
        uint256 c;
    }
}
"#;
    assert_no_diagnostics(source, "gas/struct-packing");
}

#[test]
fn test_tight_variable_packing_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    uint256 a;
    uint8 b;
    uint256 c;
    uint8 d;
}
"#;
    assert_diagnostic_count(source, "gas/tight-variable-packing", 1);
}

#[test]
fn test_tight_variable_packing_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    uint8 b;
    uint8 d;
    uint256 a;
    uint256 c;
}
"#;
    assert_no_diagnostics(source, "gas/tight-variable-packing");
}

// =============================================================================
// Style rules
// =============================================================================

#[test]
fn test_no_trailing_whitespace_detected() {
    let source = "// SPDX-License-Identifier: MIT\npragma solidity ^0.8.0;\ncontract Test {   \n    uint256 x;\n}\n";
    assert_diagnostic_count(source, "style/no-trailing-whitespace", 1);
}

#[test]
fn test_no_trailing_whitespace_clean() {
    let source = "// SPDX-License-Identifier: MIT\npragma solidity ^0.8.0;\ncontract Test {\n    uint256 x;\n}\n";
    assert_no_diagnostics(source, "style/no-trailing-whitespace");
}

#[test]
fn test_no_trailing_whitespace_fix() {
    let source = "// SPDX-License-Identifier: MIT\npragma solidity ^0.8.0;\ncontract Test {   \n    uint256 x;  \n}\n";
    let fixed = fix_source(source);
    assert!(!fixed.contains("Test {   \n"));
    assert!(!fixed.contains("x;  \n"));
    assert!(fixed.contains("Test {\n"));
    assert!(fixed.contains("x;\n"));
}

#[test]
fn test_eol_last_detected() {
    let source = "// SPDX-License-Identifier: MIT\npragma solidity ^0.8.0;\ncontract Test {}";
    assert_diagnostic_count(source, "style/eol-last", 1);
}

#[test]
fn test_eol_last_clean() {
    let source = "// SPDX-License-Identifier: MIT\npragma solidity ^0.8.0;\ncontract Test {}\n";
    assert_no_diagnostics(source, "style/eol-last");
}

#[test]
fn test_eol_last_fix() {
    let source = "// SPDX-License-Identifier: MIT\npragma solidity ^0.8.0;\ncontract Test {}";
    let fixed = fix_source(source);
    assert!(fixed.ends_with('\n'));
}

#[test]
fn test_no_multiple_empty_lines_detected() {
    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;




contract Test {
    uint256 x;
}
"#;
    assert_diagnostic_count(source, "style/no-multiple-empty-lines", 1);
}

#[test]
fn test_no_multiple_empty_lines_clean() {
    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    uint256 x;
}
"#;
    assert_no_diagnostics(source, "style/no-multiple-empty-lines");
}

#[test]
fn test_max_line_length_detected() {
    let source = format!(
        "// SPDX-License-Identifier: MIT\npragma solidity ^0.8.0;\ncontract Test {{\n    // {}\n}}\n",
        "x".repeat(200)
    );
    assert_diagnostic_count(&source, "style/max-line-length", 1);
}

#[test]
fn test_max_line_length_clean() {
    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    uint256 x;
}
"#;
    assert_no_diagnostics(source, "style/max-line-length");
}

#[test]
fn test_ordering_detected_at_file_level() {
    let source = r#"
// SPDX-License-Identifier: MIT
contract Test {}
import "./Foo.sol";
pragma solidity ^0.8.0;
"#;
    let diagnostics = lint_source_for_rule(source, "style/ordering");
    assert_eq!(diagnostics.len(), 1);
    assert!(diagnostics[0].fix.is_none());
}

#[test]
fn test_ordering_detected_in_contract_body() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    event Done();
    uint256 value;
}
"#;
    assert_diagnostic_count(source, "style/ordering", 1);
}

#[test]
fn test_ordering_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import "./Foo.sol";
contract Test {
    uint256 constant VALUE = 1;
    uint256 value;
    event Done();
    constructor() {}
    function run() external {}
    function _helper() internal {}
}
"#;
    assert_no_diagnostics(source, "style/ordering");
}

#[test]
fn test_ordering_detected_constructor_after_function() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function run() external {}
    constructor() {}
}
"#;
    assert_diagnostic_count(source, "style/ordering", 1);
}

#[test]
fn test_ordering_detected_receive_after_fallback() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    fallback() external payable {}
    receive() external payable {}
}
"#;
    assert_diagnostic_count(source, "style/ordering", 1);
}

#[test]
fn test_ordering_detected_visibility_ordering_within_functions() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function _helper() internal {}
    function run() external {}
}
"#;
    assert_diagnostic_count(source, "style/ordering", 1);
}

#[test]
fn test_ordering_detected_mutability_ordering_within_visibility() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function inspect() external view returns (uint256) {
        return 1;
    }

    function mutate() external returns (uint256) {
        return 2;
    }
}
"#;
    assert_diagnostic_count(source, "style/ordering", 1);
}

#[test]
fn test_category_headers_fix_rebuilds_sections() {
    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    event Done();
    uint256 value;
    function run() external {}
}
"#;
    let fixed = fix_source_unsafe(source);
    assert!(fixed.contains("// Storage"));
    assert!(fixed.contains("// Events"));
    assert!(fixed.contains("// Implementation"));
    assert!(fixed.find("// Storage").unwrap() < fixed.find("// Events").unwrap());
}

#[test]
fn test_category_headers_fix_splits_function_sections() {
    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    uint256 value;
    function _helper() internal {}
    function run() external {}
}
"#;
    let fixed = fix_source_unsafe(source);
    assert!(fixed.contains("// Implementation"));
    assert!(fixed.contains("// Internal Functions"));
    assert!(fixed.find("function run").unwrap() < fixed.find("function _helper").unwrap());
}

#[test]
fn test_category_headers_ignores_spacing_only_differences() {
    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    ////////////////////////////////////////////////////////////////////////
    // Storage
    ////////////////////////////////////////////////////////////////////////


    uint256 value;

    ////////////////////////////////////////////////////////////////////////
    // Implementation
    ////////////////////////////////////////////////////////////////////////
    function run() external {}
}
"#;
    assert_no_diagnostics(source, "style/category-headers");
}

#[test]
fn test_category_headers_respects_min_categories_setting() {
    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    uint256 value;
    function run() external {}
}
"#;
    let config = load_test_config(
        r#"
[lint.settings."style/category-headers"]
min_categories = 3
"#,
    );
    let diagnostics = lint_source_for_rule_with_config(source, "style/category-headers", &config);
    assert!(diagnostics.is_empty());
}

#[test]
fn test_category_headers_accepts_custom_labels_and_order() {
    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    ////////////////////////////////////////////////////////////////////////
    // Data Types
    ////////////////////////////////////////////////////////////////////////

    struct Entry {
        uint256 value;
    }

    ////////////////////////////////////////////////////////////////////////
    // State
    ////////////////////////////////////////////////////////////////////////

    uint256 private storedValue;

    ////////////////////////////////////////////////////////////////////////
    // External API
    ////////////////////////////////////////////////////////////////////////

    function run() external {}
}
"#;
    let config = load_test_config(
        r#"
[lint]
preset = "all"

[lint.settings."style/category-headers"]
min_categories = 3
order = ["types", "storage", "implementation"]

[lint.settings."style/category-headers".labels]
types = "Data Types"
storage = "State"
implementation = "External API"
"#,
    );
    let diagnostics = lint_source_for_rule_with_config(source, "style/category-headers", &config);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn test_category_headers_partial_order_preserves_unlisted_categories() {
    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    event Done();
    uint256 value;
    function run() external {}
}
"#;
    let config = load_test_config(
        r#"
[lint]
preset = "all"

[lint.settings."style/category-headers"]
order = ["storage", "implementation"]
"#,
    );
    let fixed = fix_source_unsafe_with_config(source, &config);
    assert!(fixed.contains("event Done();"), "{fixed}");
    assert!(fixed.contains("// Events"), "{fixed}");
    assert!(fixed.contains("// Storage"), "{fixed}");
    assert!(fixed.contains("// Implementation"), "{fixed}");
}

#[test]
fn test_category_headers_merges_constants_and_immutables_when_both_present() {
    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    uint256 constant MAX = 10;
    uint256 immutable ownerId;
    constructor(uint256 initialOwnerId) {
        ownerId = initialOwnerId;
    }
}
"#;
    let fixed = fix_source_unsafe(source);
    assert!(fixed.contains("// Constants & Immutables"));
    assert!(!fixed.contains("// Constants\n"));
    assert!(!fixed.contains("// Immutables\n"));
}

#[test]
fn test_category_headers_respects_separate_constant_and_immutable_order() {
    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    uint256 constant MAX = 10;
    uint256 immutable ownerId;
    constructor(uint256 initialOwnerId) {
        ownerId = initialOwnerId;
    }
}
"#;
    let config = load_test_config(
        r#"
[lint]
preset = "all"

[lint.settings."style/category-headers"]
order = ["constants", "immutables", "initialization"]
"#,
    );
    let fixed = fix_source_unsafe_with_config(source, &config);
    assert!(fixed.contains("// Constants"), "{fixed}");
    assert!(fixed.contains("// Immutables"), "{fixed}");
    assert!(!fixed.contains("// Constants & Immutables"), "{fixed}");
}

#[test]
fn test_category_headers_respects_custom_constant_and_immutable_labels() {
    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    uint256 constant MAX = 10;
    uint256 immutable ownerId;
    constructor(uint256 initialOwnerId) {
        ownerId = initialOwnerId;
    }
}
"#;
    let config = load_test_config(
        r#"
[lint]
preset = "all"

[lint.settings."style/category-headers".labels]
constants = "Compile-Time Values"
immutables = "Constructor State"
"#,
    );
    let fixed = fix_source_unsafe_with_config(source, &config);
    assert!(fixed.contains("// Compile-Time Values"), "{fixed}");
    assert!(fixed.contains("// Constructor State"), "{fixed}");
    assert!(!fixed.contains("// Constants & Immutables"), "{fixed}");
}

#[test]
fn test_category_headers_uses_constants_section_when_only_constants_exist() {
    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    uint256 constant MAX = 10;
    function run() external {}
}
"#;
    let fixed = fix_source_unsafe(source);
    assert!(fixed.contains("// Constants"));
    assert!(!fixed.contains("// Constants & Immutables"));
}

#[test]
fn test_category_headers_uses_configured_initialization_functions() {
    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    uint256 value;
    function bootstrap() public {}
}
"#;
    let config = load_test_config(
        r#"
[lint]
preset = "all"

[lint.settings."style/category-headers"]
initialization_functions = ["bootstrap"]
"#,
    );
    let fixed = fix_source_unsafe_with_config(source, &config);
    assert!(fixed.contains("// Initialization"));
    assert!(!fixed.contains("// Implementation"));
}

#[test]
fn test_category_headers_fix_keeps_initialize_consistent_with_ordering() {
    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    uint256 private _value;
    function initialize() public {}
    function run() external {}
}
"#;
    let fixed = fix_source_unsafe(source);
    assert_no_diagnostics(&fixed, "style/ordering");
}

#[test]
fn test_ordering_uses_configured_initialization_functions() {
    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    uint256 private _value;
    function bootstrap() public {}
    function run() external {}
}
"#;
    let config = load_test_config(
        r#"
[lint]
preset = "all"

[lint.settings."style/category-headers"]
initialization_functions = ["bootstrap"]
"#,
    );
    let diagnostics = lint_source_for_rule_with_config(source, "style/ordering", &config);
    assert!(diagnostics.is_empty());
}

#[test]
fn test_category_headers_empty_contract_body_clean() {
    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {}
"#;
    assert_no_diagnostics(source, "style/category-headers");
}

#[test]
fn test_imports_ordering_detected() {
    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import "./Zebra.sol";
import "./Alpha.sol";
contract Test {}
"#;
    let diagnostics = lint_source_for_rule(source, "style/imports-ordering");
    assert!(!diagnostics.is_empty());
}

#[test]
fn test_imports_ordering_ignores_nonconsecutive_import_blocks() {
    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import "./Zebra.sol";
contract Test {}
import "./Alpha.sol";
"#;
    assert_no_diagnostics(source, "style/imports-ordering");
    let fixed = fix_source(source);
    assert!(fixed.contains("contract Test {}"));
}

#[test]
fn test_imports_ordering_fix_groups_and_normalizes_quotes() {
    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import './Local.sol';
import "forge-std/Test.sol";
contract Test {}
"#;
    let fixed = fix_source(source);
    assert!(fixed.contains("import \"forge-std/Test.sol\";\n\nimport \"./Local.sol\";"));
    assert!(!fixed.contains("import './Local.sol';"));
}

#[test]
fn test_imports_ordering_spacing_only_fix() {
    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import "forge-std/Test.sol";
import "./Local.sol";
contract Test {}
"#;
    let fixed = fix_source(source);
    assert!(fixed.contains("import \"forge-std/Test.sol\";\n\nimport \"./Local.sol\";"));
}

#[test]
fn test_imports_ordering_separates_parent_and_current_dir_groups_by_default() {
    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import "../Parent.sol";
import "./Local.sol";
contract Test {}
"#;
    let fixed = fix_source(source);
    assert!(fixed.contains("import \"../Parent.sol\";\n\nimport \"./Local.sol\";"));
}

#[test]
fn test_imports_ordering_respects_regex_group_config() {
    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import "./Local.sol";
import "forge-std/Test.sol";
import "@openzeppelin/contracts/token/ERC20/ERC20.sol";
contract Test {}
"#;
    let config = load_test_config(
        r#"
[lint]
preset = "all"

[lint.settings."style/imports-ordering"]
import_order = ["^@openzeppelin/", "^forge-std/"]
"#,
    );
    let fixed = fix_source_with_config(source, &config);
    assert!(fixed.contains(
        "import \"@openzeppelin/contracts/token/ERC20/ERC20.sol\";\n\nimport \"forge-std/Test.sol\";\n\nimport \"./Local.sol\";"
    ));
}

#[test]
fn test_fix_visibility_modifier_order() {
    let source = "// SPDX-License-Identifier: MIT\npragma solidity ^0.8.0;\ncontract Test {\n    function bad() pure public returns (uint256) {\n        return 42;\n    }\n}\n";
    let fixed = fix_source(source);
    assert!(
        fixed.contains("public pure"),
        "Expected modifiers reordered to: public pure, got: {}",
        fixed
    );
}

#[test]
fn test_fix_no_unused_imports_single() {
    let source = "// SPDX-License-Identifier: MIT\npragma solidity ^0.8.0;\nimport {Unused} from \"some.sol\";\ncontract Test {}\n";
    let fixed = fix_source(source);
    assert!(
        !fixed.contains("import"),
        "Expected unused import to be removed"
    );
}

#[test]
fn test_fix_no_unused_imports_partial() {
    let source = "// SPDX-License-Identifier: MIT\npragma solidity ^0.8.0;\nimport {Used, Unused} from \"some.sol\";\ncontract Test {\n    Used public x;\n}\n";
    let fixed = fix_source(source);
    assert!(
        fixed.contains("import"),
        "Expected import statement to remain"
    );
    assert!(
        !fixed.contains("Unused"),
        "Expected unused alias to be removed"
    );
    assert!(fixed.contains("Used"), "Expected used alias to remain");
}

#[test]
fn test_no_unused_imports_inheritdoc_reference_is_used() {
    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import {IERC165} from "some.sol";
contract Test {
    /// @inheritdoc IERC165
    function supportsInterface(bytes4 interfaceId) public pure returns (bool) {
        interfaceId;
        return true;
    }
}
"#;
    assert_no_diagnostics(source, "best-practices/no-unused-imports");
}

#[test]
fn test_fix_no_unused_imports_preserves_inheritdoc_reference() {
    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import {IERC165} from "some.sol";
contract Test {
    /// @inheritdoc IERC165
    function supportsInterface(bytes4 interfaceId) public pure returns (bool) {
        interfaceId;
        return true;
    }
}
"#;
    let fixed = fix_source(source);
    assert!(
        fixed.contains(r#"import {IERC165} from "some.sol";"#),
        "Expected import referenced by @inheritdoc to remain"
    );
}

#[test]
fn test_fix_use_constant() {
    let source = "// SPDX-License-Identifier: MIT\npragma solidity ^0.8.0;\ncontract Test {\n    uint256 public MAX_SUPPLY = 1000000;\n    function get() public view returns (uint256) {\n        return MAX_SUPPLY;\n    }\n}\n";
    let fixed = fix_source_unsafe(source);
    assert!(
        fixed.contains("constant"),
        "Expected `constant` keyword to be inserted"
    );
}

#[test]
fn test_fix_use_immutable() {
    let source = "// SPDX-License-Identifier: MIT\npragma solidity ^0.8.0;\ncontract Test {\n    address public owner;\n    constructor() {\n        owner = msg.sender;\n    }\n}\n";
    let fixed = fix_source_unsafe(source);
    assert!(
        fixed.contains("immutable"),
        "Expected `immutable` keyword to be inserted"
    );
}

// =============================================================================
// Edge case tests for autofix bugs
// =============================================================================

#[test]
fn test_fix_no_unused_imports_multiple_unused_aliases() {
    // Bug: when multiple aliases are unused in the same import, each diagnostic
    // generates a fix that replaces the entire {…} range. The fixer should not
    // abort due to overlapping edits — all unused aliases should be removed.
    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import {A, B, C} from "some.sol";
contract Test {
    B public x;
}
"#;
    let expected = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {B} from "some.sol";

contract Test {
    B public x;
}
"#;
    let fixed = fix_source(source);
    assert_eq!(fixed, expected);
}

#[test]
fn test_fix_no_unused_imports_attached_to_every_diagnostic() {
    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import {A, B, C} from "some.sol";
contract Test {
    B public x;
}
"#;
    let diags = lint_source_for_rule(source, "best-practices/no-unused-imports");
    assert_eq!(diags.len(), 2);
    assert!(
        diags.iter().all(|diag| diag.fix.is_some()),
        "Expected every no-unused-imports diagnostic to have a fix"
    );
}

#[test]
fn test_fix_no_unused_imports_aliased() {
    // When a single aliased import is unused, the entire import line should be
    // deleted (same as the non-aliased single-alias case).
    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import {Foo as Bar} from "some.sol";
contract Test {}
"#;
    let expected = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {}
"#;
    let fixed = fix_source(source);
    assert_eq!(fixed, expected);
}

#[test]
fn test_fix_no_unused_imports_removes_attached_comment_block() {
    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
/// @dev Temporary helper import.
import {Foo} from "some.sol";
contract Test {}
"#;
    let expected = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {}
"#;
    let fixed = fix_source(source);
    assert_eq!(fixed, expected);
}

#[test]
fn test_fix_no_unused_imports_partial_aliased() {
    // Bug: build_unused_import_fix filters by first_word (original name "Orig")
    // but unused_name is the alias name ("Unused"). The fix should remove the
    // unused aliased entry and keep the used one.
    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import {Used, Orig as Unused} from "some.sol";
contract Test {
    Used public x;
}
"#;
    let expected = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {Used} from "some.sol";

contract Test {
    Used public x;
}
"#;
    let fixed = fix_source(source);
    assert_eq!(fixed, expected);
}

#[test]
fn test_fix_visibility_modifier_order_preserves_parameterized_modifier() {
    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    modifier onlyOwner(address who) {
        _;
    }

    function bad() pure public onlyOwner(msg.sender) returns (uint256) {
        return 42;
    }
}
"#;
    let expected = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    modifier onlyOwner(address who) {
        _;
    }

    function bad() public pure onlyOwner(msg.sender) returns (uint256) {
        return 42;
    }
}
"#;
    let fixed = fix_source(source);
    assert_eq!(fixed, expected);
}

// =============================================================================
// prefer-remappings rule tests
// =============================================================================

#[test]
fn test_prefer_remappings_no_remappings() {
    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import "../utils/Helper.sol";
contract Test {}
"#;
    assert_no_diagnostics(source, "style/prefer-remappings");
}

#[test]
fn test_prefer_remappings_relative_matches_remapping() {
    use solgrid_linter::testing::lint_source_with_remappings_for_rule;
    use std::path::{Path, PathBuf};

    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import "../utils/Helper.sol";
contract Test {}
"#;
    let remappings = vec![("@src/".to_string(), PathBuf::from("/project/src/"))];
    let diags = lint_source_with_remappings_for_rule(
        source,
        Path::new("/project/src/contracts/Token.sol"),
        &remappings,
        "style/prefer-remappings",
    );
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("@src/utils/Helper.sol"));
}

#[test]
fn test_prefer_remappings_no_match() {
    use solgrid_linter::testing::lint_source_with_remappings_for_rule;
    use std::path::{Path, PathBuf};

    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import "../lib/External.sol";
contract Test {}
"#;
    let remappings = vec![("@src/".to_string(), PathBuf::from("/project/src/"))];
    let diags = lint_source_with_remappings_for_rule(
        source,
        Path::new("/project/test/Token.sol"),
        &remappings,
        "style/prefer-remappings",
    );
    assert_eq!(diags.len(), 0);
}

#[test]
fn test_prefer_remappings_absolute_import_ignored() {
    use solgrid_linter::testing::lint_source_with_remappings_for_rule;
    use std::path::{Path, PathBuf};

    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import "@openzeppelin/contracts/token/ERC20/ERC20.sol";
contract Test {}
"#;
    let remappings = vec![(
        "@openzeppelin/".to_string(),
        PathBuf::from("/project/lib/openzeppelin-contracts/"),
    )];
    let diags = lint_source_with_remappings_for_rule(
        source,
        Path::new("/project/src/Token.sol"),
        &remappings,
        "style/prefer-remappings",
    );
    assert_eq!(diags.len(), 0);
}

#[test]
fn test_prefer_remappings_longest_target_wins() {
    use solgrid_linter::testing::lint_source_with_remappings_for_rule;
    use std::path::{Path, PathBuf};

    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import "../utils/Helper.sol";
contract Test {}
"#;
    let remappings = vec![
        ("@root/".to_string(), PathBuf::from("/project/")),
        ("@src/".to_string(), PathBuf::from("/project/src/")),
    ];
    let diags = lint_source_with_remappings_for_rule(
        source,
        Path::new("/project/src/contracts/Token.sol"),
        &remappings,
        "style/prefer-remappings",
    );
    assert_eq!(diags.len(), 1);
    // Should use @src/ (more specific) not @root/
    assert!(diags[0].message.contains("@src/utils/Helper.sol"));
}

#[test]
fn test_prefer_remappings_fix_replaces_path() {
    use solgrid_linter::testing::lint_source_with_remappings_for_rule;
    use std::path::{Path, PathBuf};

    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import "../utils/Helper.sol";
contract Test {}
"#;
    let remappings = vec![("@src/".to_string(), PathBuf::from("/project/src/"))];
    let diags = lint_source_with_remappings_for_rule(
        source,
        Path::new("/project/src/contracts/Token.sol"),
        &remappings,
        "style/prefer-remappings",
    );
    assert_eq!(diags.len(), 1);
    let fix = diags[0].fix.as_ref().expect("should have a fix");
    assert_eq!(fix.edits.len(), 1);
    assert_eq!(fix.edits[0].replacement, "@src/utils/Helper.sol");
}

#[test]
fn test_prefer_remappings_same_dir_relative() {
    use solgrid_linter::testing::lint_source_with_remappings_for_rule;
    use std::path::{Path, PathBuf};

    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import "./Helper.sol";
contract Test {}
"#;
    let remappings = vec![("@src/".to_string(), PathBuf::from("/project/src/"))];
    let diags = lint_source_with_remappings_for_rule(
        source,
        Path::new("/project/src/contracts/Token.sol"),
        &remappings,
        "style/prefer-remappings",
    );
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("@src/contracts/Helper.sol"));
}

#[test]
fn test_prefer_remappings_named_import() {
    use solgrid_linter::testing::lint_source_with_remappings_for_rule;
    use std::path::{Path, PathBuf};

    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import {IERC20} from "../interfaces/IERC20.sol";
contract Test {}
"#;
    let remappings = vec![("@src/".to_string(), PathBuf::from("/project/src/"))];
    let diags = lint_source_with_remappings_for_rule(
        source,
        Path::new("/project/src/contracts/Token.sol"),
        &remappings,
        "style/prefer-remappings",
    );
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("@src/interfaces/IERC20.sol"));
}

#[test]
fn test_prefer_remappings_multiple_imports_mixed() {
    use solgrid_linter::testing::lint_source_with_remappings_for_rule;
    use std::path::{Path, PathBuf};

    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import "../utils/Helper.sol";
import "../lib/External.sol";
import "@openzeppelin/contracts/token/ERC20/ERC20.sol";
contract Test {}
"#;
    let remappings = vec![("@src/".to_string(), PathBuf::from("/project/src/"))];
    let diags = lint_source_with_remappings_for_rule(
        source,
        Path::new("/project/src/contracts/Token.sol"),
        &remappings,
        "style/prefer-remappings",
    );
    // ../utils/Helper.sol resolves to /project/src/utils/Helper.sol → matches @src/
    // ../lib/External.sol resolves to /project/src/lib/External.sol → matches @src/
    // @openzeppelin/... is not relative → ignored
    assert_eq!(diags.len(), 2);
}

#[test]
fn test_prefer_remappings_chained_parent_dirs() {
    use solgrid_linter::testing::lint_source_with_remappings_for_rule;
    use std::path::{Path, PathBuf};

    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import "../../utils/Helper.sol";
contract Test {}
"#;
    let remappings = vec![("@src/".to_string(), PathBuf::from("/project/src/"))];
    let diags = lint_source_with_remappings_for_rule(
        source,
        Path::new("/project/src/contracts/deep/Token.sol"),
        &remappings,
        "style/prefer-remappings",
    );
    // ../../utils/Helper.sol from /project/src/contracts/deep/ → /project/src/utils/Helper.sol
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("@src/utils/Helper.sol"));
}

#[test]
fn test_prefer_remappings_fix_end_to_end() {
    use solgrid_linter::testing::fix_source_with_remappings;
    use std::path::{Path, PathBuf};

    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import "../utils/Helper.sol";
contract Test {}
"#;
    let expected = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "@src/utils/Helper.sol";

contract Test {}
"#;
    let remappings = vec![("@src/".to_string(), PathBuf::from("/project/src/"))];
    let fixed = fix_source_with_remappings(
        source,
        Path::new("/project/src/contracts/Token.sol"),
        &remappings,
        true, // suggestion fixes require unsafe
    );
    assert_eq!(fixed, expected);
}

#[test]
#[cfg(unix)]
fn test_prefer_remappings_matches_canonical_paths() {
    use solgrid_linter::testing::lint_source_with_remappings_for_rule;
    use std::fs;
    use std::os::unix::fs::symlink;
    use std::time::{SystemTime, UNIX_EPOCH};

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("solgrid-remap-test-{unique}"));
    let real_root = root.join("real");
    fs::create_dir_all(real_root.join("src/contracts")).expect("create contracts");
    fs::create_dir_all(real_root.join("src/utils")).expect("create utils");
    symlink(&real_root, root.join("link")).expect("create symlink");

    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import "../utils/Helper.sol";
contract Test {}
"#;
    let remappings = vec![("@src/".to_string(), root.join("link/src"))];
    let diags = lint_source_with_remappings_for_rule(
        source,
        &real_root.join("src/contracts/Token.sol"),
        &remappings,
        "style/prefer-remappings",
    );

    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("@src/utils/Helper.sol"));

    fs::remove_dir_all(&root).expect("cleanup");
}

#[test]
fn test_prefer_remappings_fix_end_to_end_named_import() {
    use solgrid_linter::testing::fix_source_with_remappings;
    use std::path::{Path, PathBuf};

    // Use IERC20 in the contract body to avoid no-unused-imports removing it
    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import {IERC20} from "../interfaces/IERC20.sol";
contract Test {
    IERC20 public token;
}
"#;
    let expected = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {IERC20} from "@src/interfaces/IERC20.sol";

contract Test {
    IERC20 public token;
}
"#;
    let remappings = vec![("@src/".to_string(), PathBuf::from("/project/src/"))];
    let fixed = fix_source_with_remappings(
        source,
        Path::new("/project/src/contracts/Token.sol"),
        &remappings,
        true,
    );
    assert_eq!(fixed, expected);
}

#[test]
fn test_prefer_remappings_prefix_without_trailing_slash() {
    use solgrid_linter::testing::lint_source_with_remappings_for_rule;
    use std::path::{Path, PathBuf};

    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import "../utils/Helper.sol";
contract Test {}
"#;
    // Prefix intentionally lacks trailing slash
    let remappings = vec![("@src".to_string(), PathBuf::from("/project/src/"))];
    let diags = lint_source_with_remappings_for_rule(
        source,
        Path::new("/project/src/contracts/Token.sol"),
        &remappings,
        "style/prefer-remappings",
    );
    assert_eq!(diags.len(), 1);
    // Should produce "@src/utils/Helper.sol", not "@srcutils/Helper.sol"
    assert!(
        diags[0].message.contains("@src/utils/Helper.sol"),
        "got: {}",
        diags[0].message
    );
}

#[test]
fn test_file_name_format_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract MyContract {}
"#;
    // The test uses "test.sol" as filename, which doesn't match "MyContract"
    let diags = lint_source_for_rule(source, "style/file-name-format");
    assert!(
        !diags.is_empty(),
        "Expected file name format diagnostic for mismatched name"
    );
}

#[test]
fn test_file_name_format_no_contract() {
    // No contract = no diagnostic
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
"#;
    assert_no_diagnostics(source, "style/file-name-format");
}

// =============================================================================
// Documentation rules
// =============================================================================

#[test]
fn test_license_identifier_detected() {
    let source = r#"
pragma solidity ^0.8.0;
contract Test {}
"#;
    assert_diagnostic_count(source, "docs/license-identifier", 1);
}

#[test]
fn test_license_identifier_clean() {
    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {}
"#;
    assert_no_diagnostics(source, "docs/license-identifier");
}

#[test]
fn test_docs_natspec_detected_on_contract() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {}
"#;
    assert_diagnostic_count(source, "docs/natspec", 1);
}

#[test]
fn test_docs_natspec_param_mismatch_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
/// @notice Test contract
contract Test {
    /// @notice Does foo
    /// @param wrongName The value
    function foo(uint256 x) external {}
}
"#;
    assert_diagnostic_count(source, "docs/natspec", 1);
}

#[test]
fn test_docs_natspec_event_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
/// @notice Test contract
contract Test {
    /// @notice Emitted on transfer
    /// @param from Sender
    /// @param to Recipient
    /// @param value Amount
    event Transfer(address indexed from, address indexed to, uint256 value);
}
"#;
    assert_no_diagnostics(source, "docs/natspec");
}

#[test]
fn test_natspec_modifier_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    modifier onlyOwner() {
        _;
    }
}
"#;
    assert_diagnostic_count(source, "docs/natspec-modifier", 1);
}

#[test]
fn test_natspec_modifier_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    /// @notice Restricts access to owner
    modifier onlyOwner() {
        _;
    }
}
"#;
    assert_no_diagnostics(source, "docs/natspec-modifier");
}

#[test]
fn test_selector_tags_error_fix() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    error Unauthorized(address caller);
}
"#;
    let fixed = fix_source(source);
    assert!(fixed.contains("/// @dev Error selector: `0x8e4a23d6`"));
}

#[test]
fn test_selector_tags_interface_fix() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
interface ITest {
    function foo(uint256 x) external returns (bool);
}
"#;
    let fixed = fix_source(source);
    assert!(fixed.contains("/// @dev Interface selector: `0x2fbebd38`"));
}

#[test]
fn test_selector_tags_accepts_exact_canonical_interface_line() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
/// @dev Interface selector: `0xc2985578`
interface ITest {
    function foo() external;
}
"#;
    assert_no_diagnostics(source, "docs/selector-tags");
}

#[test]
fn test_selector_tags_resolves_imported_structs() {
    let dir = tempfile::tempdir().unwrap();
    let shared_path = dir.path().join("Shared.sol");
    let main_path = dir.path().join("Main.sol");

    fs::write(
        &shared_path,
        r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
struct Pair {
    uint256 left;
    bool right;
}
"#,
    )
    .unwrap();

    fs::write(
        &main_path,
        r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import {Pair} from "./Shared.sol";

error InvalidPair(Pair pair);
"#,
    )
    .unwrap();

    let engine = LintEngine::new();
    let mut config = Config::default();
    config.lint.preset = RulePreset::All;
    let result = engine.fix_source(
        &fs::read_to_string(&main_path).unwrap(),
        &main_path,
        &config,
        false,
    );
    let fixed = result.0;
    assert!(fixed.contains("/// @dev Error selector: `0x"));
}

#[test]
fn test_selector_tags_skip_unresolved_custom_types() {
    let dir = tempfile::tempdir().unwrap();
    let main_path = dir.path().join("Main.sol");

    fs::write(
        &main_path,
        r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import {Pair} from "./Missing.sol";

error InvalidPair(Pair pair);
"#,
    )
    .unwrap();

    let source = fs::read_to_string(&main_path).unwrap();
    let engine = LintEngine::new();
    let mut config = Config::default();
    config.lint.preset = RulePreset::All;

    let diagnostics = engine.lint_source(&source, &main_path, &config).diagnostics;
    assert_eq!(
        diagnostics
            .iter()
            .filter(|diag| diag.rule_id == "docs/selector-tags")
            .count(),
        0
    );

    let (fixed, _) = engine.fix_source(&source, &main_path, &config, false);
    assert!(!fixed.contains("Error selector"));
}

#[test]
fn test_selector_tags_rewrites_incorrect_error_tag() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    /// @dev Error selector: `0xdeadbeef`
    error Unauthorized(address caller);
}
"#;
    let fixed = fix_source(source);
    assert!(fixed.contains("/// @dev Error selector: `0x8e4a23d6`"));
    assert!(!fixed.contains("0xdeadbeef"));
}

#[test]
fn test_selector_tags_parameterless_interface_rewrites_stale_tag() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
/// @dev Interface selector: `0xdeadbeef`
interface ITest {
    function foo() external;
}
"#;
    let fixed = fix_source(source);
    assert!(fixed.contains("/// @dev Interface selector: `0xc2985578`"));
    assert!(!fixed.contains("0xdeadbeef"));
}

#[test]
fn test_selector_tags_ignores_events() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    event Done(address caller);
}
"#;
    assert_no_diagnostics(source, "docs/selector-tags");
}

// =============================================================================
// Config setting tests
// =============================================================================

#[test]
fn test_code_complexity_threshold_setting() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity 0.8.24;
contract Test {
    function complex(uint256 a, uint256 b, uint256 c) public pure returns (uint256) {
        if (a > 0) {
            if (b > 0) {
                if (c > 0) {
                    return a + b + c;
                }
            }
        }
        return 0;
    }
}
"#;
    let mut config = Config::default();
    config.lint.settings.insert(
        "best-practices/code-complexity".into(),
        table(&[("threshold", toml::Value::Integer(3))]),
    );
    let diagnostics = lint_source_with_config(source, &config);
    assert_eq!(
        diagnostics
            .iter()
            .filter(|diag| diag.rule_id == "best-practices/code-complexity")
            .count(),
        1
    );
}

#[test]
fn test_function_max_lines_setting() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity 0.8.24;
contract Test {
    function longOne() public pure returns (uint256) {
        uint256 x = 0;
        x += 1;
        x += 2;
        x += 3;
        return x;
    }
}
"#;
    let mut config = Config::default();
    config.lint.settings.insert(
        "best-practices/function-max-lines".into(),
        table(&[("max_lines", toml::Value::Integer(3))]),
    );
    let diagnostics = lint_source_with_config(source, &config);
    assert_eq!(
        diagnostics
            .iter()
            .filter(|diag| diag.rule_id == "best-practices/function-max-lines")
            .count(),
        1
    );
}

#[test]
fn test_max_states_count_setting() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity 0.8.24;
contract Test {
    uint256 a;
    uint256 b;
    uint256 c;
}
"#;
    let mut config = Config::default();
    config.lint.settings.insert(
        "best-practices/max-states-count".into(),
        table(&[("max_count", toml::Value::Integer(2))]),
    );
    let diagnostics = lint_source_with_config(source, &config);
    assert_eq!(
        diagnostics
            .iter()
            .filter(|diag| diag.rule_id == "best-practices/max-states-count")
            .count(),
        1
    );
}

#[test]
fn test_compiler_version_allowed_setting_passes() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity >=0.8.19 <0.9.0;
contract Test {}
"#;
    let mut config = Config::default();
    config.lint.settings.insert(
        "security/compiler-version".into(),
        table(&[(
            "allowed",
            toml::Value::Array(vec![
                toml::Value::String(">=0.8.19".into()),
                toml::Value::String("<0.9.0".into()),
            ]),
        )]),
    );
    let diagnostics = lint_source_with_config(source, &config);
    assert_eq!(
        diagnostics
            .iter()
            .filter(|diag| diag.rule_id == "security/compiler-version")
            .count(),
        0
    );
}

#[test]
fn test_compiler_version_allowed_setting_fails() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity 0.8.18;
contract Test {}
"#;
    let mut config = Config::default();
    config.lint.settings.insert(
        "security/compiler-version".into(),
        table(&[(
            "allowed",
            toml::Value::Array(vec![
                toml::Value::String(">=0.8.19".into()),
                toml::Value::String("<0.9.0".into()),
            ]),
        )]),
    );
    let diagnostics = lint_source_with_config(source, &config);
    assert_eq!(
        diagnostics
            .iter()
            .filter(|diag| diag.rule_id == "security/compiler-version")
            .count(),
        1
    );
}

#[test]
fn test_compiler_version_allowed_setting_rejects_wide_pragma_range() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity >=0.8.0 <0.8.19;
contract Test {}
"#;
    let mut config = Config::default();
    config.lint.settings.insert(
        "security/compiler-version".into(),
        table(&[(
            "allowed",
            toml::Value::Array(vec![
                toml::Value::String(">=0.8.19".into()),
                toml::Value::String("<0.9.0".into()),
            ]),
        )]),
    );
    let diagnostics = lint_source_with_config(source, &config);
    assert_eq!(
        diagnostics
            .iter()
            .filter(|diag| diag.rule_id == "security/compiler-version")
            .count(),
        1
    );
}

#[test]
fn test_compiler_version_allowed_setting_exact_match() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity 0.8.24;
contract Test {}
"#;
    let mut config = Config::default();
    config.lint.settings.insert(
        "security/compiler-version".into(),
        table(&[(
            "allowed",
            toml::Value::Array(vec![toml::Value::String("=0.8.24".into())]),
        )]),
    );
    let diagnostics = lint_source_with_config(source, &config);
    assert_eq!(
        diagnostics
            .iter()
            .filter(|diag| diag.rule_id == "security/compiler-version")
            .count(),
        0
    );
}

#[test]
fn test_compiler_version_allowed_setting_exact_mismatch() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity 0.8.23;
contract Test {}
"#;
    let mut config = Config::default();
    config.lint.settings.insert(
        "security/compiler-version".into(),
        table(&[(
            "allowed",
            toml::Value::Array(vec![toml::Value::String("=0.8.24".into())]),
        )]),
    );
    let diagnostics = lint_source_with_config(source, &config);
    assert_eq!(
        diagnostics
            .iter()
            .filter(|diag| diag.rule_id == "security/compiler-version")
            .count(),
        1
    );
}

#[test]
fn test_compiler_version_allowed_setting_supports_caret_pragma() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.19;
contract Test {}
"#;
    let mut config = Config::default();
    config.lint.settings.insert(
        "security/compiler-version".into(),
        table(&[(
            "allowed",
            toml::Value::Array(vec![
                toml::Value::String(">=0.8.19".into()),
                toml::Value::String("<0.9.0".into()),
            ]),
        )]),
    );
    let diagnostics = lint_source_with_config(source, &config);
    assert_eq!(
        diagnostics
            .iter()
            .filter(|diag| diag.rule_id == "security/compiler-version")
            .count(),
        0
    );
}

#[test]
fn test_compiler_version_allowed_setting_supports_tilde_pragma() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ~0.8.19;
contract Test {}
"#;
    let mut config = Config::default();
    config.lint.settings.insert(
        "security/compiler-version".into(),
        table(&[(
            "allowed",
            toml::Value::Array(vec![
                toml::Value::String(">=0.8.19".into()),
                toml::Value::String("<0.9.0".into()),
            ]),
        )]),
    );
    let diagnostics = lint_source_with_config(source, &config);
    assert_eq!(
        diagnostics
            .iter()
            .filter(|diag| diag.rule_id == "security/compiler-version")
            .count(),
        0
    );
}

#[test]
fn test_compiler_version_allowed_setting_flags_unsupported_disjunction() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.19 || ^0.8.24;
contract Test {}
"#;
    let mut config = Config::default();
    config.lint.settings.insert(
        "security/compiler-version".into(),
        table(&[(
            "allowed",
            toml::Value::Array(vec![
                toml::Value::String(">=0.8.19".into()),
                toml::Value::String("<0.9.0".into()),
            ]),
        )]),
    );
    let diagnostics = lint_source_with_config(source, &config);
    let compiler_diagnostics: Vec<_> = diagnostics
        .iter()
        .filter(|diag| diag.rule_id == "security/compiler-version")
        .collect();
    assert_eq!(compiler_diagnostics.len(), 1);
    assert!(compiler_diagnostics[0]
        .message
        .contains("could not be verified against the configured allowed range"));
}

#[test]
fn test_foundry_test_function_pattern_setting() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity 0.8.24;
contract Test {
    function test_custom() public {}
}
"#;
    let mut config = Config::default();
    config.lint.settings.insert(
        "naming/foundry-test-functions".into(),
        table(&[("pattern", toml::Value::String("^test_[a-z]+$".into()))]),
    );
    let diagnostics = lint_source_with_config(source, &config);
    assert_eq!(
        diagnostics
            .iter()
            .filter(|diag| diag.rule_id == "naming/foundry-test-functions")
            .count(),
        0
    );
}

#[test]
fn test_max_line_length_setting() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity 0.8.24;
contract Test { string constant NAME = "123456789012345678901234567890"; }
"#;
    let mut config = Config::default();
    config.lint.settings.insert(
        "style/max-line-length".into(),
        table(&[("limit", toml::Value::Integer(40))]),
    );
    config
        .lint
        .rules
        .insert("style/max-line-length".into(), RuleLevel::Info);
    let diagnostics = lint_source_with_config(source, &config);
    assert_eq!(
        diagnostics
            .iter()
            .filter(|diag| diag.rule_id == "style/max-line-length")
            .count(),
        1
    );
}

// =============================================================================
// Registry tests
// =============================================================================

#[test]
fn test_registry_has_all_rules() {
    let engine = solgrid_linter::LintEngine::new();
    let registry = engine.registry();
    // The NatSpec and ordering cleanup removes several overlapping rules while
    // adding consolidated replacements.
    assert!(
        registry.len() >= 82,
        "Expected at least 82 rules, got {}",
        registry.len()
    );
}

#[test]
fn test_registry_lookup() {
    let engine = solgrid_linter::LintEngine::new();
    let registry = engine.registry();
    assert!(registry.get("security/tx-origin").is_some());
    assert!(registry.get("security/avoid-selfdestruct").is_some());
    assert!(registry.get("security/reentrancy").is_some());
    assert!(registry.get("security/uninitialized-storage").is_some());
    assert!(registry.get("naming/contract-name-capwords").is_some());
    assert!(registry.get("naming/interface-starts-with-i").is_some());
    assert!(registry.get("naming/named-parameters-mapping").is_some());
    assert!(registry.get("best-practices/no-floating-pragma").is_some());
    assert!(registry.get("best-practices/constructor-syntax").is_some());
    assert!(registry.get("best-practices/duplicated-imports").is_some());
    assert!(registry
        .get("best-practices/visibility-modifier-order")
        .is_some());
    assert!(registry.get("best-practices/no-unused-imports").is_some());
    assert!(registry.get("best-practices/no-unused-state").is_some());
    assert!(registry.get("best-practices/no-unused-vars").is_some());
    // Gas rules
    assert!(registry.get("gas/bool-storage").is_some());
    assert!(registry.get("gas/increment-by-one").is_some());
    assert!(registry.get("gas/cache-array-length").is_some());
    assert!(registry.get("gas/unchecked-increment").is_some());
    assert!(registry.get("gas/small-strings").is_some());
    assert!(registry.get("gas/custom-errors").is_some());
    assert!(registry.get("gas/use-bytes32").is_some());
    assert!(registry.get("docs/natspec").is_some());
    assert!(registry.get("docs/selector-tags").is_some());
    assert!(registry.get("style/category-headers").is_some());
    assert!(registry.get("style/imports-ordering").is_some());
    assert!(registry.get("style/ordering").is_some());
    assert!(registry.get("gas/calldata-parameters").is_some());
    assert!(registry.get("gas/indexed-events").is_some());
    assert!(registry.get("gas/named-return-values").is_some());
    assert!(registry.get("gas/use-constant").is_some());
    assert!(registry.get("gas/use-immutable").is_some());
    assert!(registry.get("gas/no-redundant-sload").is_some());
    assert!(registry.get("gas/struct-packing").is_some());
    assert!(registry.get("gas/tight-variable-packing").is_some());
    // Style rules
    assert!(registry.get("style/ordering").is_some());
    assert!(registry.get("style/category-headers").is_some());
    assert!(registry.get("style/imports-ordering").is_some());
    assert!(registry.get("style/max-line-length").is_some());
    assert!(registry.get("style/no-trailing-whitespace").is_some());
    assert!(registry.get("style/eol-last").is_some());
    assert!(registry.get("style/no-multiple-empty-lines").is_some());
    assert!(registry.get("style/prefer-remappings").is_some());
    assert!(registry.get("style/file-name-format").is_some());
    // Docs rules
    assert!(registry.get("docs/natspec").is_some());
    assert!(registry.get("docs/selector-tags").is_some());
    assert!(registry.get("docs/natspec-modifier").is_some());
    assert!(registry.get("docs/license-identifier").is_some());
    assert!(registry.get("nonexistent/rule").is_none());
}

// =============================================================================
// Additional security rules
// =============================================================================

#[test]
fn test_state_visibility_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    uint256 x;
}
"#;
    assert_diagnostic_count(source, "security/state-visibility", 1);
}

#[test]
fn test_state_visibility_span_excludes_initializer() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    uint256 constant X = 1;
}
"#;
    let diagnostics = lint_source_for_rule(source, "security/state-visibility");
    assert_eq!(diagnostics.len(), 1);
    let span = &diagnostics[0].span;
    assert_eq!(
        &source[span.clone()],
        "uint256 constant X",
        "span should cover declaration up to the name, not the initializer"
    );
}

#[test]
fn test_state_visibility_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    uint256 public x;
}
"#;
    assert_no_diagnostics(source, "security/state-visibility");
}

#[test]
fn test_unchecked_transfer_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
interface IERC20 {
    function transfer(address to, uint256 amount) external returns (bool);
}
contract Test {
    function bad(IERC20 token, address to, uint256 amount) public {
        token.transfer(to, amount);
    }
}
"#;
    assert_diagnostic_count(source, "security/unchecked-transfer", 1);
}

#[test]
fn test_unchecked_transfer_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
interface IERC20 {
    function transfer(address to, uint256 amount) external returns (bool);
}
contract Test {
    function good(IERC20 token, address to, uint256 amount) public {
        require(token.transfer(to, amount));
    }
}
"#;
    assert_no_diagnostics(source, "security/unchecked-transfer");
}

// =============================================================================
// Additional best practices rules
// =============================================================================

#[test]
fn test_no_empty_blocks_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function foo() public {}
}
"#;
    assert_diagnostic_count(source, "best-practices/no-empty-blocks", 1);
}

#[test]
fn test_no_empty_blocks_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    receive() external payable {}
}
"#;
    assert_no_diagnostics(source, "best-practices/no-empty-blocks");
}

#[test]
fn test_no_empty_blocks_reports_virtual_functions_by_default() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function hook() internal virtual {}
}
"#;
    assert_diagnostic_count(source, "best-practices/no-empty-blocks", 1);
}

#[test]
fn test_no_empty_blocks_ignores_commented_empty_functions_by_default() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function hook() internal {
        /* empty */
    }
}
"#;
    assert_no_diagnostics(source, "best-practices/no-empty-blocks");
}

#[test]
fn test_no_empty_blocks_can_report_commented_empty_functions() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function hook() internal {
        /* empty */
    }
}
"#;
    let mut config = Config::default();
    config.lint.preset = RulePreset::All;
    config.lint.settings.insert(
        "best-practices/no-empty-blocks".into(),
        table(&[("allow_comments", toml::Value::Boolean(false))]),
    );

    let diagnostics =
        lint_source_for_rule_with_config(source, "best-practices/no-empty-blocks", &config);
    assert_eq!(diagnostics.len(), 1, "{diagnostics:#?}");
}

// =============================================================================
// Additional gas rules
// =============================================================================

#[test]
fn test_use_bytes32_constant_string_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    string constant NAME = "hello";
}
"#;
    assert_diagnostic_count(source, "gas/use-bytes32", 1);
}

#[test]
fn test_use_bytes32_non_constant_string_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    string public name;
}
"#;
    assert_no_diagnostics(source, "gas/use-bytes32");
}

// =============================================================================
// Code-review finding tests
// =============================================================================

#[test]
fn test_compiler_version_allowed_range_subset_check_regression() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity >=0.8.0 <0.8.19;
contract Test {}
"#;
    let mut config = Config::default();
    config.lint.settings.insert(
        "security/compiler-version".into(),
        table(&[(
            "allowed",
            toml::Value::Array(vec![
                toml::Value::String(">=0.8.19".into()),
                toml::Value::String("<0.9.0".into()),
            ]),
        )]),
    );
    let diagnostics = lint_source_with_config(source, &config);
    let compiler_diags: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule_id == "security/compiler-version")
        .collect();
    assert_eq!(
        compiler_diags.len(),
        1,
        "wide pragma ranges should fail when any permitted compiler version falls outside the configured allowed range"
    );
}

#[test]
fn test_gas_custom_errors_suppressed_when_best_practices_enabled_via_preset() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity 0.8.24;
contract Test {
    function bad(uint256 x) public pure {
        require(x > 0, "x must be positive");
    }
}
"#;
    let mut config = Config::default();
    config
        .lint
        .rules
        .insert("gas/custom-errors".into(), RuleLevel::Warn);

    let diagnostics = lint_source_with_config(source, &config);
    let bp_count = diagnostics
        .iter()
        .filter(|d| d.rule_id == "best-practices/custom-errors")
        .count();
    let gas_count = diagnostics
        .iter()
        .filter(|d| d.rule_id == "gas/custom-errors")
        .count();

    assert_eq!(bp_count, 1, "best-practices/custom-errors should fire");
    assert_eq!(
        gas_count, 0,
        "gas/custom-errors should be suppressed by registry metadata when best-practices/custom-errors is enabled"
    );
}
