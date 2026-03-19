//! Integration tests for lint rules.

use solgrid_linter::testing::{
    assert_diagnostic_count, assert_no_diagnostics, fix_source, fix_source_unsafe,
    lint_source_for_rule,
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
fn test_use_natspec_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function doSomething() public pure returns (uint256) {
        return 42;
    }
}
"#;
    assert_diagnostic_count(source, "best-practices/use-natspec", 1);
}

#[test]
fn test_use_natspec_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    /// @notice Does something useful
    function doSomething() public pure returns (uint256) {
        return 42;
    }
}
"#;
    assert_no_diagnostics(source, "best-practices/use-natspec");
}

#[test]
fn test_use_natspec_block_comment_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    /**
     * @notice Does something useful
     */
    function doSomething() public pure returns (uint256) {
        return 42;
    }
}
"#;
    assert_no_diagnostics(source, "best-practices/use-natspec");
}

#[test]
fn test_use_natspec_skips_internal() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function _internal() internal pure returns (uint256) {
        return 42;
    }
}
"#;
    assert_no_diagnostics(source, "best-practices/use-natspec");
}

#[test]
fn test_natspec_params_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    /// @notice Transfers tokens
    function transfer(address to, uint256 amount) public {
    }
}
"#;
    let diagnostics = lint_source_for_rule(source, "best-practices/natspec-params");
    assert!(
        diagnostics.len() >= 2,
        "Expected at least 2 diagnostics for missing @param, got {}",
        diagnostics.len()
    );
}

#[test]
fn test_natspec_params_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    /// @notice Transfers tokens
    /// @param to Recipient address
    /// @param amount Number of tokens
    function transfer(address to, uint256 amount) public {
    }
}
"#;
    assert_no_diagnostics(source, "best-practices/natspec-params");
}

#[test]
fn test_natspec_returns_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    /// @notice Gets the balance
    function getBalance() public pure returns (uint256) {
        return 42;
    }
}
"#;
    assert_diagnostic_count(source, "best-practices/natspec-returns", 1);
}

#[test]
fn test_natspec_returns_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    /// @notice Gets the balance
    /// @return The current balance
    function getBalance() public pure returns (uint256) {
        return 42;
    }
}
"#;
    assert_no_diagnostics(source, "best-practices/natspec-returns");
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
    assert_diagnostic_count(source, "gas/custom-errors", 1);
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
fn test_func_order_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function foo() private {}
    function bar() external {}
}
"#;
    assert_diagnostic_count(source, "style/func-order", 1);
}

#[test]
fn test_func_order_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    constructor() {}
    function bar() external {}
    function baz() public {}
    function foo() private {}
}
"#;
    assert_no_diagnostics(source, "style/func-order");
}

#[test]
fn test_ordering_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
contract Test {}
import "./Foo.sol";
pragma solidity ^0.8.0;
"#;
    // import after contract, pragma after contract
    let diags = lint_source_for_rule(source, "style/ordering");
    assert!(!diags.is_empty(), "Expected at least 1 ordering diagnostic");
}

#[test]
fn test_ordering_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import "./Foo.sol";
contract Test {}
"#;
    assert_no_diagnostics(source, "style/ordering");
}

#[test]
fn test_contract_layout_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function foo() external {}
    uint256 x;
}
"#;
    assert_diagnostic_count(source, "style/contract-layout", 1);
}

#[test]
fn test_contract_layout_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    uint256 x;
    event Transfer(address indexed from, address indexed to, uint256 value);
    function foo() external {}
}
"#;
    assert_no_diagnostics(source, "style/contract-layout");
}

#[test]
fn test_imports_ordering_detected() {
    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import "./Zebra.sol";
import "./Alpha.sol";
contract Test {}
"#;
    assert_diagnostic_count(source, "style/imports-ordering", 1);
}

#[test]
fn test_imports_ordering_clean() {
    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import "./Alpha.sol";
import "./Zebra.sol";
contract Test {}
"#;
    assert_no_diagnostics(source, "style/imports-ordering");
}

#[test]
fn test_imports_ordering_fix() {
    let source = "// SPDX-License-Identifier: MIT\npragma solidity ^0.8.0;\nimport \"./Zebra.sol\";\nimport \"./Alpha.sol\";\ncontract Test {}\n";
    let fixed = fix_source(source);
    assert!(fixed.contains("import \"./Alpha.sol\";\nimport \"./Zebra.sol\";"));
}

#[test]
fn test_imports_ordering_fix_multiple() {
    let source = "// SPDX-License-Identifier: MIT\npragma solidity ^0.8.0;\nimport \"./Charlie.sol\";\nimport \"./Alpha.sol\";\nimport \"./Bravo.sol\";\ncontract Test {}\n";
    let fixed = fix_source(source);
    assert!(fixed
        .contains("import \"./Alpha.sol\";\nimport \"./Bravo.sol\";\nimport \"./Charlie.sol\";"));
}

#[test]
fn test_contract_layout_fix() {
    let source = "\n// SPDX-License-Identifier: MIT\npragma solidity ^0.8.0;\ncontract Test {\n    function foo() external {}\n    uint256 x;\n}\n";
    let diags = lint_source_for_rule(source, "style/contract-layout");
    assert!(!diags.is_empty());
    assert!(
        diags[0].fix.is_some(),
        "Expected contract-layout diagnostic to have a fix"
    );
    let fixed = fix_source_unsafe(source);
    let x_pos = fixed.find("uint256 x").unwrap();
    let foo_pos = fixed.find("function foo").unwrap();
    assert!(
        x_pos < foo_pos,
        "Expected state variable before function after fix"
    );
}

#[test]
fn test_contract_layout_fix_attached_to_every_diagnostic() {
    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function foo() external {}
    error Oops();
    uint256 x;
}
"#;
    let diags = lint_source_for_rule(source, "style/contract-layout");
    assert_eq!(diags.len(), 2);
    assert!(
        diags.iter().all(|diag| diag.fix.is_some()),
        "Expected every contract-layout diagnostic to have a fix"
    );

    let expected = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    uint256 x;

    error Oops();

    function foo() external {}
}
"#;
    let fixed = fix_source_unsafe(source);
    assert_eq!(fixed, expected);
}

#[test]
fn test_contract_layout_fix_normalizes_member_spacing() {
    let source = r#"abstract contract CCIPBatcher is CCIPReader {
    /// @notice The batch gateway supplied an incorrect number of responses.
    /// @dev Error selector: `0x4a5c31ea`
    error InvalidBatchGatewayResponse();

    uint256 constant FLAG_OFFCHAIN = 1 << 0; // the lookup reverted `OffchainLookup`
    uint256 constant FLAG_CALL_ERROR = 1 << 1; // the initial call or callback reverted
    uint256 constant FLAG_BATCH_ERROR = 1 << 2; // `OffchainLookup` failed on the batch gateway
    uint256 constant FLAG_EMPTY_RESPONSE = 1 << 3; // the initial call or callback returned `0x`
    uint256 constant FLAG_EIP140_BEFORE = 1 << 4; // does not have revert op code
    uint256 constant FLAG_EIP140_AFTER = 1 << 5; // has revert op code
    uint256 constant FLAG_DONE = 1 << 6; // the lookup has finished processing (private)

    uint256 constant FLAGS_ANY_ERROR =
        FLAG_CALL_ERROR | FLAG_BATCH_ERROR | FLAG_EMPTY_RESPONSE;
    uint256 constant FLAGS_ANY_EIP140 = FLAG_EIP140_BEFORE | FLAG_EIP140_AFTER;

    /// @dev An independent `OffchainLookup` session.
    struct Lookup {
        address target; // contract to call
        bytes call; // initial calldata
        bytes data; // response or error
        uint256 flags; // see: FLAG_*
    }

    /// @dev A batch gateway session.
    struct Batch {
        Lookup[] lookups;
        string[] gateways;
    }

    function createBatch(
        bytes memory data
    ) internal pure returns (Batch memory batch) {}
}
"#;
    let expected = r#"abstract contract CCIPBatcher is CCIPReader {
    /// @dev An independent `OffchainLookup` session.
    struct Lookup {
        address target; // contract to call
        bytes call; // initial calldata
        bytes data; // response or error
        uint256 flags; // see: FLAG_*
    }

    /// @dev A batch gateway session.
    struct Batch {
        Lookup[] lookups;
        string[] gateways;
    }

    uint256 constant FLAG_OFFCHAIN = 1 << 0; // the lookup reverted `OffchainLookup`
    uint256 constant FLAG_CALL_ERROR = 1 << 1; // the initial call or callback reverted
    uint256 constant FLAG_BATCH_ERROR = 1 << 2; // `OffchainLookup` failed on the batch gateway
    uint256 constant FLAG_EMPTY_RESPONSE = 1 << 3; // the initial call or callback returned `0x`
    uint256 constant FLAG_EIP140_BEFORE = 1 << 4; // does not have revert op code
    uint256 constant FLAG_EIP140_AFTER = 1 << 5; // has revert op code
    uint256 constant FLAG_DONE = 1 << 6; // the lookup has finished processing (private)

    uint256 constant FLAGS_ANY_ERROR =
        FLAG_CALL_ERROR | FLAG_BATCH_ERROR | FLAG_EMPTY_RESPONSE;
    uint256 constant FLAGS_ANY_EIP140 = FLAG_EIP140_BEFORE | FLAG_EIP140_AFTER;

    /// @notice The batch gateway supplied an incorrect number of responses.
    /// @dev Error selector: `0x4a5c31ea`
    error InvalidBatchGatewayResponse();

    function createBatch(
        bytes memory data
    ) internal pure returns (Batch memory batch) {}
}
"#;

    let fixed = fix_source_unsafe(source);
    assert_eq!(fixed, expected);
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

#[test]
fn test_fix_func_order() {
    let source = "\n// SPDX-License-Identifier: MIT\npragma solidity ^0.8.0;\ncontract Test {\n    function foo() private {}\n    function bar() external {}\n}\n";
    let diags = lint_source_for_rule(source, "style/func-order");
    assert!(!diags.is_empty());
    assert!(
        diags[0].fix.is_some(),
        "Expected func-order diagnostic to have a fix"
    );
    let fixed = fix_source_unsafe(source);
    let bar_pos = fixed.find("function bar").unwrap();
    let foo_pos = fixed.find("function foo").unwrap();
    assert!(
        bar_pos < foo_pos,
        "Expected external before private after fix"
    );
}

#[test]
fn test_fix_func_order_attached_to_every_diagnostic() {
    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function a() private {}
    function b() public {}
    function c() external {}
}
"#;
    let diags = lint_source_for_rule(source, "style/func-order");
    assert_eq!(diags.len(), 2);
    assert!(
        diags.iter().all(|diag| diag.fix.is_some()),
        "Expected every func-order diagnostic to have a fix"
    );

    let expected = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function c() external {}

    function b() public {}

    function a() private {}
}
"#;
    let fixed = fix_source_unsafe(source);
    assert_eq!(fixed, expected);
}

#[test]
fn test_fix_ordering() {
    let source = "\n// SPDX-License-Identifier: MIT\ncontract Test {}\nimport \"./Foo.sol\";\npragma solidity ^0.8.0;\n";
    let diags = lint_source_for_rule(source, "style/ordering");
    assert!(!diags.is_empty());
    assert!(
        diags[0].fix.is_some(),
        "Expected ordering diagnostic to have a fix"
    );
}

#[test]
fn test_fix_ordering_attached_to_every_diagnostic() {
    let source = r#"// SPDX-License-Identifier: MIT
contract Test {}
library Math {}
import "./Foo.sol";
pragma solidity ^0.8.0;
"#;
    let diags = lint_source_for_rule(source, "style/ordering");
    assert_eq!(diags.len(), 3);
    assert!(
        diags.iter().all(|diag| diag.fix.is_some()),
        "Expected every ordering diagnostic to have a fix"
    );

    let expected = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import "./Foo.sol";
library Math {}
contract Test {}
"#;
    let fixed = fix_source_unsafe(source);
    assert_eq!(fixed, expected);
}

#[test]
fn test_fix_import_path_format() {
    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import "./External.sol";
import "Local.sol";
import "Other.sol";
contract Test {}
"#;
    let expected = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import "External.sol";
import "Local.sol";
import "Other.sol";
contract Test {}
"#;
    let fixed = fix_source_unsafe(source);
    assert_eq!(fixed, expected);
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

#[test]
fn test_fix_func_order_preserves_non_function_members() {
    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    uint256 value;

    function foo() private {}

    function bar() external {}
}
"#;
    let expected = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    uint256 value;

    function bar() external {}

    function foo() private {}
}
"#;
    let fixed = fix_source_unsafe(source);
    assert_eq!(fixed, expected);
}

#[test]
fn test_fix_import_path_format_does_not_rewrite_package_imports() {
    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import "./Local.sol";
import "./Other.sol";
import "@openzeppelin/contracts/token/ERC20/ERC20.sol";
contract Test {}
"#;
    let expected = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import "./Local.sol";
import "./Other.sol";
import "@openzeppelin/contracts/token/ERC20/ERC20.sol";
contract Test {}
"#;
    let fixed = fix_source_unsafe(source);
    assert_eq!(fixed, expected);
}

#[test]
fn test_fix_import_path_format_does_not_rewrite_parent_relative_imports() {
    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import "../utils/Helper.sol";
import "A.sol";
import "B.sol";
contract Test {}
"#;
    let expected = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import "../utils/Helper.sol";
import "A.sol";
import "B.sol";
contract Test {}
"#;
    let fixed = fix_source_unsafe(source);
    assert_eq!(fixed, expected);
}

#[test]
fn test_import_path_format_detected() {
    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import "./Local.sol";
import "./Other.sol";
import "lib/External.sol";
contract Test {}
"#;
    // Mix of relative and absolute - minority (absolute) should be flagged
    assert_diagnostic_count(source, "style/import-path-format", 1);
}

#[test]
fn test_import_path_format_clean() {
    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import "./Local.sol";
import "./Other.sol";
contract Test {}
"#;
    assert_no_diagnostics(source, "style/import-path-format");
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
fn test_natspec_contract_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {}
"#;
    assert_diagnostic_count(source, "docs/natspec-contract", 1);
}

#[test]
fn test_natspec_contract_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
/// @title Test contract
/// @author Test author
contract Test {}
"#;
    assert_no_diagnostics(source, "docs/natspec-contract");
}

#[test]
fn test_natspec_contract_missing_author() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
/// @title Test contract
contract Test {}
"#;
    assert_diagnostic_count(source, "docs/natspec-contract", 1);
}

#[test]
fn test_natspec_interface_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
interface ITest {
    function foo() external;
}
"#;
    assert_diagnostic_count(source, "docs/natspec-interface", 1);
}

#[test]
fn test_natspec_interface_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
interface ITest {
    /// @notice Does foo
    function foo() external;
}
"#;
    assert_no_diagnostics(source, "docs/natspec-interface");
}

#[test]
fn test_natspec_function_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function foo() external {}
}
"#;
    assert_diagnostic_count(source, "docs/natspec-function", 1);
}

#[test]
fn test_natspec_function_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    /// @notice Does foo
    function foo() external {}
}
"#;
    assert_no_diagnostics(source, "docs/natspec-function");
}

#[test]
fn test_natspec_function_missing_notice() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    /// @param x The value
    function foo(uint256 x) external {}
}
"#;
    assert_diagnostic_count(source, "docs/natspec-function", 1);
}

#[test]
fn test_natspec_event_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    event Transfer(address indexed from, address indexed to, uint256 value);
}
"#;
    assert_diagnostic_count(source, "docs/natspec-event", 1);
}

#[test]
fn test_natspec_event_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    /// @notice Emitted on transfer
    event Transfer(address indexed from, address indexed to, uint256 value);
}
"#;
    assert_no_diagnostics(source, "docs/natspec-event");
}

#[test]
fn test_natspec_error_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    error Unauthorized();
}
"#;
    assert_diagnostic_count(source, "docs/natspec-error", 1);
}

#[test]
fn test_natspec_error_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    /// @notice Thrown when unauthorized
    error Unauthorized();
}
"#;
    assert_no_diagnostics(source, "docs/natspec-error");
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
fn test_natspec_param_mismatch_detected() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    /// @notice Does foo
    /// @param wrongName The value
    function foo(uint256 x) external {}
}
"#;
    assert_diagnostic_count(source, "docs/natspec-param-mismatch", 1);
}

#[test]
fn test_natspec_param_mismatch_clean() {
    let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    /// @notice Does foo
    /// @param x The value
    function foo(uint256 x) external {}
}
"#;
    assert_no_diagnostics(source, "docs/natspec-param-mismatch");
}

// =============================================================================
// Registry tests
// =============================================================================

#[test]
fn test_registry_has_all_rules() {
    let engine = solgrid_linter::LintEngine::new();
    let registry = engine.registry();
    // 19 security + 22 best-practices + 16 naming + 15 gas + 10 style + 8 docs = 90 rules
    assert!(
        registry.len() >= 90,
        "Expected at least 90 rules, got {}",
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
    assert!(registry.get("best-practices/no-floating-pragma").is_some());
    assert!(registry.get("best-practices/constructor-syntax").is_some());
    assert!(registry.get("best-practices/use-natspec").is_some());
    assert!(registry.get("best-practices/natspec-params").is_some());
    assert!(registry.get("best-practices/natspec-returns").is_some());
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
    assert!(registry.get("gas/calldata-parameters").is_some());
    assert!(registry.get("gas/indexed-events").is_some());
    assert!(registry.get("gas/named-return-values").is_some());
    assert!(registry.get("gas/use-constant").is_some());
    assert!(registry.get("gas/use-immutable").is_some());
    assert!(registry.get("gas/no-redundant-sload").is_some());
    assert!(registry.get("gas/struct-packing").is_some());
    assert!(registry.get("gas/tight-variable-packing").is_some());
    // Style rules
    assert!(registry.get("style/func-order").is_some());
    assert!(registry.get("style/ordering").is_some());
    assert!(registry.get("style/imports-ordering").is_some());
    assert!(registry.get("style/max-line-length").is_some());
    assert!(registry.get("style/no-trailing-whitespace").is_some());
    assert!(registry.get("style/eol-last").is_some());
    assert!(registry.get("style/no-multiple-empty-lines").is_some());
    assert!(registry.get("style/contract-layout").is_some());
    assert!(registry.get("style/import-path-format").is_some());
    assert!(registry.get("style/file-name-format").is_some());
    // Docs rules
    assert!(registry.get("docs/natspec-contract").is_some());
    assert!(registry.get("docs/natspec-interface").is_some());
    assert!(registry.get("docs/natspec-function").is_some());
    assert!(registry.get("docs/natspec-event").is_some());
    assert!(registry.get("docs/natspec-error").is_some());
    assert!(registry.get("docs/natspec-modifier").is_some());
    assert!(registry.get("docs/natspec-param-mismatch").is_some());
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
