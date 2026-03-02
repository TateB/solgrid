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
// Registry tests
// =============================================================================

#[test]
fn test_registry_has_all_rules() {
    let engine = solgrid_linter::LintEngine::new();
    let registry = engine.registry();
    // We registered 12 rules total
    assert!(
        registry.len() >= 12,
        "Expected at least 12 rules, got {}",
        registry.len()
    );
}

#[test]
fn test_registry_lookup() {
    let engine = solgrid_linter::LintEngine::new();
    let registry = engine.registry();
    assert!(registry.get("security/tx-origin").is_some());
    assert!(registry.get("naming/contract-name-capwords").is_some());
    assert!(registry.get("nonexistent/rule").is_none());
}
