//! Integration tests for lint rules.

use solgrid_testing::{
    assert_diagnostic_count, assert_no_diagnostics, fix_source, lint_source_for_rule,
};

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
    assert!(diagnostics.len() >= 1, "Expected at least 1 diagnostic for arbitrary-send-eth");
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
                                    c--;
                                }
                            }
                        }
                    }
                }
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
// Registry tests
// =============================================================================

#[test]
fn test_registry_has_all_rules() {
    let engine = solgrid_linter::LintEngine::new();
    let registry = engine.registry();
    // 17 security + 14 best-practices + 16 naming = 47 rules
    assert!(
        registry.len() >= 47,
        "Expected at least 47 rules, got {}",
        registry.len()
    );
}

#[test]
fn test_registry_lookup() {
    let engine = solgrid_linter::LintEngine::new();
    let registry = engine.registry();
    assert!(registry.get("security/tx-origin").is_some());
    assert!(registry.get("security/avoid-selfdestruct").is_some());
    assert!(registry.get("naming/contract-name-capwords").is_some());
    assert!(registry.get("naming/interface-starts-with-i").is_some());
    assert!(registry.get("best-practices/no-floating-pragma").is_some());
    assert!(registry.get("nonexistent/rule").is_none());
}
