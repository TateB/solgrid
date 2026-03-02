//! Integration tests for the solgrid formatter.

use solgrid_config::FormatConfig;
use solgrid_formatter::{check_formatted, format_source, format_source_verified};

fn default_config() -> FormatConfig {
    FormatConfig::default()
}

// --- Pragma formatting ---

#[test]
fn test_format_pragma() {
    let source = "pragma solidity ^0.8.0;\n";
    let formatted = format_source(source, &default_config()).unwrap();
    assert!(formatted.contains("pragma solidity ^0.8.0;"));
}

// --- Import formatting ---

#[test]
fn test_format_plain_import() {
    let source = "import \"./Foo.sol\";\n";
    let formatted = format_source(source, &default_config()).unwrap();
    assert!(formatted.contains("import \"./Foo.sol\";"));
}

#[test]
fn test_format_named_import() {
    let source = "import {Foo, Bar} from \"./Foo.sol\";\n";
    let formatted = format_source(source, &default_config()).unwrap();
    assert!(formatted.contains("Foo"));
    assert!(formatted.contains("Bar"));
    assert!(formatted.contains("from"));
}

#[test]
fn test_format_named_import_with_bracket_spacing() {
    let source = "import {Foo} from \"./Foo.sol\";\n";
    let config = FormatConfig {
        bracket_spacing: true,
        ..default_config()
    };
    let formatted = format_source(source, &config).unwrap();
    assert!(formatted.contains("{ Foo }"));
}

// --- Contract formatting ---

#[test]
fn test_format_empty_contract() {
    let source = "contract Foo {}\n";
    let formatted = format_source(source, &default_config()).unwrap();
    assert!(formatted.contains("contract Foo {"));
}

#[test]
fn test_format_contract_with_inheritance() {
    let source = "contract Foo is Bar, Baz {}\n";
    let formatted = format_source(source, &default_config()).unwrap();
    assert!(formatted.contains("is Bar, Baz"));
}

#[test]
fn test_format_interface() {
    let source = "interface IFoo {\n    function bar() external;\n}\n";
    let formatted = format_source(source, &default_config()).unwrap();
    assert!(formatted.contains("interface IFoo"));
    assert!(formatted.contains("function bar()"));
}

#[test]
fn test_format_library() {
    let source = "library MathLib {\n    function add(uint256 a, uint256 b) internal pure returns (uint256) {\n        return a + b;\n    }\n}\n";
    let formatted = format_source(source, &default_config()).unwrap();
    assert!(formatted.contains("library MathLib"));
}

// --- Function formatting ---

#[test]
fn test_format_simple_function() {
    let source = "contract T {\n    function foo() public pure returns (uint256) {\n        return 1;\n    }\n}\n";
    let formatted = format_source(source, &default_config()).unwrap();
    assert!(formatted.contains("function foo()"));
    assert!(formatted.contains("public"));
    assert!(formatted.contains("pure"));
    assert!(formatted.contains("returns"));
    assert!(formatted.contains("return 1;"));
}

#[test]
fn test_format_constructor() {
    let source = "contract T {\n    constructor(uint256 x) {\n        value = x;\n    }\n}\n";
    let formatted = format_source(source, &default_config()).unwrap();
    assert!(formatted.contains("constructor("));
}

#[test]
fn test_format_fallback() {
    let source = "contract T {\n    fallback() external payable {}\n}\n";
    let formatted = format_source(source, &default_config()).unwrap();
    assert!(formatted.contains("fallback()"));
}

// --- Variable declarations ---

#[test]
fn test_format_state_variable() {
    let source = "contract T {\n    uint256 public x;\n}\n";
    let formatted = format_source(source, &default_config()).unwrap();
    assert!(formatted.contains("uint256 public x;"));
}

#[test]
fn test_format_constant() {
    let source = "contract T {\n    uint256 constant MAX = 100;\n}\n";
    let formatted = format_source(source, &default_config()).unwrap();
    assert!(formatted.contains("constant"));
    assert!(formatted.contains("MAX"));
}

// --- Uint type config ---

#[test]
fn test_uint_type_long() {
    let source = "contract T {\n    uint x;\n}\n";
    let config = FormatConfig {
        uint_type: solgrid_config::UintType::Long,
        ..default_config()
    };
    let formatted = format_source(source, &config).unwrap();
    assert!(formatted.contains("uint256"));
}

#[test]
fn test_uint_type_short() {
    let source = "contract T {\n    uint256 x;\n}\n";
    let config = FormatConfig {
        uint_type: solgrid_config::UintType::Short,
        ..default_config()
    };
    let formatted = format_source(source, &config).unwrap();
    assert!(formatted.contains("uint ") || formatted.contains("uint\n"));
}

// --- Struct / Enum / Event / Error ---

#[test]
fn test_format_struct() {
    let source =
        "contract T {\n    struct Point {\n        uint256 x;\n        uint256 y;\n    }\n}\n";
    let formatted = format_source(source, &default_config()).unwrap();
    assert!(formatted.contains("struct Point"));
    assert!(formatted.contains("uint256 x;"));
}

#[test]
fn test_format_enum() {
    let source = "contract T {\n    enum Status {\n        Active,\n        Inactive\n    }\n}\n";
    let formatted = format_source(source, &default_config()).unwrap();
    assert!(formatted.contains("enum Status"));
    assert!(formatted.contains("Active"));
}

#[test]
fn test_format_event() {
    let source = "contract T {\n    event Transfer(address indexed from, address indexed to, uint256 value);\n}\n";
    let formatted = format_source(source, &default_config()).unwrap();
    assert!(formatted.contains("event Transfer("));
    assert!(formatted.contains("indexed"));
}

#[test]
fn test_format_error() {
    let source =
        "contract T {\n    error InsufficientBalance(uint256 available, uint256 required);\n}\n";
    let formatted = format_source(source, &default_config()).unwrap();
    assert!(formatted.contains("error InsufficientBalance("));
}

// --- Expressions ---

#[test]
fn test_format_binary_expr() {
    let source = "contract T {\n    function f() public pure returns (uint256) {\n        return 1 + 2;\n    }\n}\n";
    let formatted = format_source(source, &default_config()).unwrap();
    assert!(formatted.contains("1 + 2"));
}

#[test]
fn test_format_function_call() {
    let source = "contract T {\n    function f() public {\n        foo(1, 2);\n    }\n}\n";
    let formatted = format_source(source, &default_config()).unwrap();
    assert!(formatted.contains("foo("));
}

// --- Statements ---

#[test]
fn test_format_if_else() {
    let source = "contract T {\n    function f(uint256 x) public pure returns (uint256) {\n        if (x > 0) {\n            return x;\n        } else {\n            return 0;\n        }\n    }\n}\n";
    let formatted = format_source(source, &default_config()).unwrap();
    assert!(formatted.contains("if ("));
    assert!(formatted.contains("else"));
}

#[test]
fn test_format_for_loop() {
    let source = "contract T {\n    function f() public pure {\n        for (uint256 i = 0; i < 10; i++) {\n            continue;\n        }\n    }\n}\n";
    let formatted = format_source(source, &default_config()).unwrap();
    assert!(formatted.contains("for ("));
}

#[test]
fn test_format_while_loop() {
    let source = "contract T {\n    function f(uint256 x) public pure {\n        while (x > 0) {\n            x--;\n        }\n    }\n}\n";
    let formatted = format_source(source, &default_config()).unwrap();
    assert!(formatted.contains("while ("));
}

#[test]
fn test_format_emit() {
    let source = "contract T {\n    event Foo(uint256 x);\n    function f() public {\n        emit Foo(1);\n    }\n}\n";
    let formatted = format_source(source, &default_config()).unwrap();
    assert!(formatted.contains("emit Foo("));
}

// --- Single quote config ---

#[test]
fn test_single_quote_config() {
    let source = "contract T {\n    string public name = \"hello\";\n}\n";
    let config = FormatConfig {
        single_quote: true,
        ..default_config()
    };
    let formatted = format_source(source, &config).unwrap();
    assert!(formatted.contains("'hello'"));
}

// --- Number underscore config ---

#[test]
fn test_number_underscore_thousands() {
    let source = "contract T {\n    uint256 constant X = 1000000;\n}\n";
    let config = FormatConfig {
        number_underscore: solgrid_config::NumberUnderscore::Thousands,
        ..default_config()
    };
    let formatted = format_source(source, &config).unwrap();
    assert!(formatted.contains("1_000_000"));
}

#[test]
fn test_number_underscore_remove() {
    let source = "contract T {\n    uint256 constant X = 1_000_000;\n}\n";
    let config = FormatConfig {
        number_underscore: solgrid_config::NumberUnderscore::Remove,
        ..default_config()
    };
    let formatted = format_source(source, &config).unwrap();
    assert!(formatted.contains("1000000"));
}

// --- Comment preservation ---

#[test]
fn test_preserve_line_comment() {
    let source = "// SPDX-License-Identifier: MIT\npragma solidity ^0.8.0;\n";
    let formatted = format_source(source, &default_config()).unwrap();
    assert!(formatted.contains("// SPDX-License-Identifier: MIT"));
}

#[test]
fn test_preserve_block_comment() {
    let source = "/* Multi-line comment */\npragma solidity ^0.8.0;\n";
    let formatted = format_source(source, &default_config()).unwrap();
    assert!(formatted.contains("/* Multi-line comment */"));
}

// --- Formatter directives ---

#[test]
fn test_fmt_off_on() {
    let source = "// solgrid-fmt: off\ncontract   T  {  }\n// solgrid-fmt: on\n";
    let formatted = format_source(source, &default_config()).unwrap();
    // The "contract   T  {  }" should be preserved verbatim
    assert!(formatted.contains("contract   T  {  }"));
}

// --- Idempotency ---

#[test]
fn test_idempotency_simple() {
    let source = "pragma solidity ^0.8.0;\n\ncontract Foo {\n    uint256 public x;\n}\n";
    let result = format_source_verified(source, &default_config());
    assert!(
        result.is_ok(),
        "Formatter should be idempotent: {}",
        result.unwrap_err()
    );
}

#[test]
fn test_idempotency_complex() {
    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {IERC20} from "./IERC20.sol";

contract Token is IERC20 {
    mapping(address => uint256) private _balances;

    event Transfer(address indexed from, address indexed to, uint256 value);
    error InsufficientBalance(uint256 available, uint256 required);

    function transfer(address to, uint256 amount) external returns (bool) {
        if (_balances[msg.sender] < amount) {
            revert InsufficientBalance(_balances[msg.sender], amount);
        }
        _balances[msg.sender] -= amount;
        _balances[to] += amount;
        emit Transfer(msg.sender, to, amount);
        return true;
    }
}
"#;
    let result = format_source_verified(source, &default_config());
    assert!(
        result.is_ok(),
        "Formatter should be idempotent: {}",
        result.unwrap_err()
    );
}

// --- Round-trip (format then re-parse) ---

#[test]
fn test_round_trip_parses() {
    let source = r#"pragma solidity ^0.8.0;

contract Test {
    uint256 public value;

    constructor(uint256 _value) {
        value = _value;
    }

    function getValue() public view returns (uint256) {
        return value;
    }
}
"#;
    let formatted = format_source(source, &default_config()).unwrap();
    // The formatted output should also parse successfully
    let result = format_source(&formatted, &default_config());
    assert!(result.is_ok(), "Formatted code should be valid Solidity");
}

// --- Check formatted ---

#[test]
fn test_check_formatted() {
    let source = "pragma solidity ^0.8.0;\n";
    let formatted = format_source(source, &default_config()).unwrap();
    let is_formatted = check_formatted(&formatted, &default_config()).unwrap();
    assert!(
        is_formatted,
        "Re-formatted output should be considered formatted"
    );
}

// --- Sort imports config ---

#[test]
fn test_sort_imports() {
    let source = r#"import "./C.sol";
import "./A.sol";
import "./B.sol";
"#;
    let config = FormatConfig {
        sort_imports: true,
        ..default_config()
    };
    let formatted = format_source(source, &config).unwrap();
    let a_pos = formatted.find("A.sol").unwrap();
    let b_pos = formatted.find("B.sol").unwrap();
    let c_pos = formatted.find("C.sol").unwrap();
    assert!(a_pos < b_pos, "A.sol should come before B.sol");
    assert!(b_pos < c_pos, "B.sol should come before C.sol");
}

// --- UDVT formatting ---

#[test]
fn test_format_udvt() {
    let source = "type Price is uint256;\n";
    let formatted = format_source(source, &default_config()).unwrap();
    assert!(formatted.contains("type Price is uint256;"));
}

// --- Mapping formatting ---

#[test]
fn test_format_mapping() {
    let source = "contract T {\n    mapping(address => uint256) public balances;\n}\n";
    let formatted = format_source(source, &default_config()).unwrap();
    assert!(formatted.contains("mapping(address => uint256)"));
}

// --- Syntax error handling ---

#[test]
fn test_syntax_error() {
    let source = "contract { }"; // Missing name
    let result = format_source(source, &default_config());
    assert!(result.is_err());
}
