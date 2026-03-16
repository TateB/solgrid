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
    assert!(
        formatted.contains("import {Foo, Bar} from \"./Foo.sol\";")
            || formatted.contains("import { Foo, Bar } from \"./Foo.sol\";")
    );
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
    assert!(
        formatted.contains("function foo()"),
        "should contain function signature"
    );
    assert!(formatted.contains("public"), "should contain visibility");
    assert!(formatted.contains("pure"), "should contain mutability");
    assert!(
        formatted.contains("returns (uint256)"),
        "should contain return type"
    );
    assert!(
        formatted.contains("return 1;"),
        "should contain function body"
    );
    // Formatting should be idempotent
    let reformatted = format_source(&formatted, &default_config()).unwrap();
    assert_eq!(formatted, reformatted, "formatting should be idempotent");
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
    // Should have converted uint256 to uint
    assert!(
        formatted.contains("uint x;") || formatted.contains("uint  x;"),
        "expected 'uint x;' but got: {formatted}"
    );
    assert!(
        !formatted.contains("uint256"),
        "uint256 should have been shortened to uint"
    );
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

// --- Contract body spacing ---

#[test]
fn test_preserve_blank_lines_default() {
    let source = r#"contract T {
    uint256 public x;

    uint256 public y;
}
"#;
    let formatted = format_source(source, &default_config()).unwrap();
    // The blank line between x and y should be preserved (indent whitespace on blank line)
    let lines: Vec<&str> = formatted.lines().collect();
    let x_line = lines
        .iter()
        .position(|l| l.contains("uint256 public x;"))
        .unwrap();
    let y_line = lines
        .iter()
        .position(|l| l.contains("uint256 public y;"))
        .unwrap();
    assert!(
        y_line - x_line >= 2,
        "there should be a blank line between x and y, got:\n{formatted}"
    );
}

#[test]
fn test_preserve_no_blank_line() {
    let source = r#"contract T {
    uint256 public x;
    uint256 public y;
}
"#;
    let formatted = format_source(source, &default_config()).unwrap();
    // No blank line in source = no blank line in output
    assert!(
        formatted.contains("uint256 public x;\n    uint256 public y;"),
        "no blank line should be added, got:\n{formatted}"
    );
}

#[test]
fn test_preserve_comment_not_a_gap() {
    let source = r#"contract T {
    uint256 public x;
    // comment about y
    uint256 public y;
}
"#;
    let formatted = format_source(source, &default_config()).unwrap();
    // Comment between items without blank line should not create a gap
    assert!(
        !formatted.contains("x;\n\n"),
        "comment should not create a gap, got:\n{formatted}"
    );
    assert!(
        formatted.contains("// comment about y"),
        "comment should be preserved"
    );
}

#[test]
fn test_preserve_blank_line_with_comment() {
    let source = r#"contract T {
    uint256 public x;

    // comment about y
    uint256 public y;
}
"#;
    let formatted = format_source(source, &default_config()).unwrap();
    // Blank line before comment should be preserved
    let lines: Vec<&str> = formatted.lines().collect();
    let x_line = lines
        .iter()
        .position(|l| l.contains("uint256 public x;"))
        .unwrap();
    let comment_line = lines
        .iter()
        .position(|l| l.contains("// comment about y"))
        .unwrap();
    assert!(
        comment_line - x_line >= 2,
        "blank line should be preserved before comment, got:\n{formatted}"
    );
}

#[test]
fn test_single_spacing_mode() {
    let source = r#"contract T {
    uint256 public x;
    uint256 public y;
    uint256 public z;
}
"#;
    let config = FormatConfig {
        contract_body_spacing: solgrid_config::ContractBodySpacing::Single,
        ..default_config()
    };
    let formatted = format_source(source, &config).unwrap();
    // Should add blank lines between all items
    let lines: Vec<&str> = formatted.lines().collect();
    let x_line = lines
        .iter()
        .position(|l| l.contains("uint256 public x;"))
        .unwrap();
    let y_line = lines
        .iter()
        .position(|l| l.contains("uint256 public y;"))
        .unwrap();
    assert!(
        y_line - x_line >= 2,
        "single mode should add blank line, got:\n{formatted}"
    );
}

#[test]
fn test_compact_spacing_mode() {
    let source = r#"contract T {
    uint256 public x;

    uint256 public y;

    function foo() public pure returns (uint256) {
        return 1;
    }

    function bar() public pure returns (uint256) {
        return 2;
    }
}
"#;
    let config = FormatConfig {
        contract_body_spacing: solgrid_config::ContractBodySpacing::Compact,
        ..default_config()
    };
    let formatted = format_source(source, &config).unwrap();
    let lines: Vec<&str> = formatted.lines().collect();
    // Compact: no blank line between single-line items
    let x_line = lines
        .iter()
        .position(|l| l.contains("uint256 public x;"))
        .unwrap();
    let y_line = lines
        .iter()
        .position(|l| l.contains("uint256 public y;"))
        .unwrap();
    assert_eq!(
        y_line - x_line,
        1,
        "compact mode should remove blank lines between single-line items, got:\n{formatted}"
    );
    // But blank line around multiline items (functions with bodies)
    let foo_line = lines
        .iter()
        .position(|l| l.contains("function foo()"))
        .unwrap();
    assert!(
        foo_line - y_line >= 2,
        "compact mode should keep blank lines around multiline items, got:\n{formatted}"
    );
}

// --- Inheritance brace placement ---

#[test]
fn test_inheritance_brace_new_line_default() {
    // Force a long inheritance list that must wrap
    let source = "contract OwnedResolver is Ownable, ABIResolver, AddrResolver, ContentHashResolver, DNSResolver, InterfaceResolver, NameResolver, PubkeyResolver, TextResolver, ExtendedResolver {}\n";
    let formatted = format_source(source, &default_config()).unwrap();
    // When inheritance wraps, { should be on its own line
    assert!(
        formatted.contains("\n{"),
        "opening brace should be on new line when inheritance wraps, got:\n{formatted}"
    );
}

#[test]
fn test_inheritance_brace_same_line_when_fits() {
    let source = "contract Foo is Bar, Baz {}\n";
    let formatted = format_source(source, &default_config()).unwrap();
    // When inheritance fits on one line, { stays on same line
    assert!(
        formatted.contains("is Bar, Baz {}"),
        "opening brace should stay on same line when inheritance fits, got:\n{formatted}"
    );
}

#[test]
fn test_inheritance_brace_same_line_config() {
    let source = "contract OwnedResolver is Ownable, ABIResolver, AddrResolver, ContentHashResolver, DNSResolver, InterfaceResolver, NameResolver, PubkeyResolver, TextResolver, ExtendedResolver {}\n";
    let config = FormatConfig {
        inheritance_brace_new_line: false,
        ..default_config()
    };
    let formatted = format_source(source, &config).unwrap();
    // With config off, { should be on same line as last base
    assert!(
        !formatted.contains("\n{"),
        "opening brace should NOT be on new line with config off, got:\n{formatted}"
    );
}

#[test]
fn test_inheritance_brace_idempotent() {
    let source = "contract OwnedResolver is Ownable, ABIResolver, AddrResolver, ContentHashResolver, DNSResolver, InterfaceResolver, NameResolver, PubkeyResolver, TextResolver, ExtendedResolver {}\n";
    let formatted = format_source(source, &default_config()).unwrap();
    let reformatted = format_source(&formatted, &default_config()).unwrap();
    assert_eq!(
        formatted, reformatted,
        "inheritance brace formatting should be idempotent"
    );
}

#[test]
fn test_single_spacing_with_comments() {
    let source = r#"contract T {
    uint256 public x;
    // comment about y
    uint256 public y;
}
"#;
    let config = FormatConfig {
        contract_body_spacing: solgrid_config::ContractBodySpacing::Single,
        ..default_config()
    };
    let formatted = format_source(source, &config).unwrap();
    // Single mode should still add blank line even with comments between items
    let lines: Vec<&str> = formatted.lines().collect();
    let x_line = lines
        .iter()
        .position(|l| l.contains("uint256 public x;"))
        .unwrap();
    let comment_line = lines
        .iter()
        .position(|l| l.contains("// comment about y"))
        .unwrap();
    assert!(
        comment_line - x_line >= 2,
        "single mode should add blank line even with comments, got:\n{formatted}"
    );
}

// --- Top-level blank line spacing ---

#[test]
fn test_blank_line_between_pragma_and_import() {
    let source = "pragma solidity ^0.8.0;\nimport \"./Foo.sol\";\n";
    let formatted = format_source(source, &default_config()).unwrap();
    assert!(
        formatted.contains("^0.8.0;\n\nimport"),
        "expected 1 blank line between pragma and import, got:\n{formatted}"
    );
    assert!(
        !formatted.contains("^0.8.0;\n\n\nimport"),
        "should NOT have 2 blank lines between pragma and import, got:\n{formatted}"
    );
}

#[test]
fn test_blank_line_between_import_and_contract() {
    let source = "import \"./Foo.sol\";\ncontract A {}\n";
    let formatted = format_source(source, &default_config()).unwrap();
    assert!(
        formatted.contains("\"./Foo.sol\";\n\ncontract A"),
        "expected 1 blank line between import and contract, got:\n{formatted}"
    );
    assert!(
        !formatted.contains("\"./Foo.sol\";\n\n\ncontract A"),
        "should NOT have 2 blank lines between import and contract, got:\n{formatted}"
    );
}

#[test]
fn test_two_blank_lines_between_contracts() {
    let source = "contract A {}\ncontract B {}\n";
    let formatted = format_source(source, &default_config()).unwrap();
    assert!(
        formatted.contains("}\n\n\ncontract B"),
        "expected 2 blank lines between contracts, got:\n{formatted}"
    );
    assert!(
        !formatted.contains("}\n\n\n\ncontract B"),
        "should NOT have 3 blank lines between contracts, got:\n{formatted}"
    );
}

#[test]
fn test_no_blank_line_between_imports() {
    let source = "import \"./Foo.sol\";\nimport \"./Bar.sol\";\n";
    let formatted = format_source(source, &default_config()).unwrap();
    assert!(
        formatted.contains("\"./Foo.sol\";\nimport"),
        "expected no blank line between imports, got:\n{formatted}"
    );
}

// ============================================================================
// Solidity Style Guide — Yes/No Example Tests
// https://docs.soliditylang.org/en/latest/style-guide.html
// ============================================================================

// --- 1. Blank Lines: Two blank lines between top-level declarations ---

#[test]
fn test_style_guide_blank_lines_between_contracts_yes() {
    // Style guide "Yes" example: two blank lines between contracts
    let source = r#"// SPDX-License-Identifier: GPL-3.0
pragma solidity >=0.4.0 <0.9.0;

contract A {
    uint256 x;
}


contract B {
    uint256 y;
}


contract C {
    uint256 z;
}
"#;
    let formatted = format_source(source, &default_config()).unwrap();
    // Verify two blank lines (3 newlines) between contracts
    assert!(
        formatted.contains("}\n\n\ncontract B"),
        "expected 2 blank lines between A and B, got:\n{formatted}"
    );
    assert!(
        formatted.contains("}\n\n\ncontract C"),
        "expected 2 blank lines between B and C, got:\n{formatted}"
    );
}

#[test]
fn test_style_guide_blank_lines_between_contracts_no() {
    // Style guide "No" example: only 0-1 blank lines between contracts
    let source = r#"// SPDX-License-Identifier: GPL-3.0
pragma solidity >=0.4.0 <0.9.0;

contract A {
    uint256 x;
}
contract B {
    uint256 y;
}

contract C {
    uint256 z;
}
"#;
    let formatted = format_source(source, &default_config()).unwrap();
    // Formatter should insert two blank lines between each contract
    assert!(
        formatted.contains("}\n\n\ncontract B"),
        "formatter should add 2 blank lines between A and B, got:\n{formatted}"
    );
    assert!(
        formatted.contains("}\n\n\ncontract C"),
        "formatter should add 2 blank lines between B and C, got:\n{formatted}"
    );
}

// --- 2. Blank Lines: Functions within contract ---

#[test]
fn test_style_guide_function_spacing_yes() {
    // Style guide "Yes": single blank line between functions with bodies
    let source = r#"contract B {
    function spam() public pure returns (uint256) {
        return 1;
    }

    function ham() public pure returns (uint256) {
        return 2;
    }
}
"#;
    let config = FormatConfig {
        contract_body_spacing: solgrid_config::ContractBodySpacing::Preserve,
        ..default_config()
    };
    let formatted = format_source(source, &config).unwrap();
    // Blank line between functions should be preserved
    assert!(
        formatted.contains("}\n\n    function ham"),
        "expected blank line between spam and ham, got:\n{formatted}"
    );
}

#[test]
fn test_style_guide_function_spacing_no() {
    // Style guide "No": missing blank lines between functions with bodies
    let source = r#"contract B {
    function spam() public pure returns (uint256) {
        return 1;
    }
    function ham() public pure returns (uint256) {
        return 2;
    }
}
"#;
    let config = FormatConfig {
        contract_body_spacing: solgrid_config::ContractBodySpacing::Single,
        ..default_config()
    };
    let formatted = format_source(source, &config).unwrap();
    // Single mode should add blank line between functions
    assert!(
        formatted.contains("}\n\n    function ham"),
        "single mode should add blank line between functions, got:\n{formatted}"
    );
}

// --- 3. Maximum Line Length: Function Calls ---

#[test]
fn test_style_guide_long_function_call_yes() {
    // Style guide "Yes": each argument on its own line, indented
    let source = r#"contract T {
    function f() public {
        thisFunctionCallIsReallyLong(
            longArgument1,
            longArgument2,
            longArgument3
        );
    }
}
"#;
    let config = FormatConfig {
        line_length: 50,
        ..default_config()
    };
    let formatted = format_source(source, &config).unwrap();
    // Each argument should be on its own line
    assert!(
        formatted.contains("longArgument1,"),
        "should contain longArgument1, got:\n{formatted}"
    );
    assert!(
        formatted.contains("longArgument2,"),
        "should contain longArgument2, got:\n{formatted}"
    );
    assert!(
        formatted.contains("longArgument3"),
        "should contain longArgument3, got:\n{formatted}"
    );
}

#[test]
fn test_style_guide_long_function_call_no_aligned_to_paren() {
    // Style guide "No": arguments aligned to opening paren
    let source = r#"contract T {
    function f() public {
        thisFunctionCallIsReallyLong(longArgument1,
                                      longArgument2,
                                      longArgument3
        );
    }
}
"#;
    let config = FormatConfig {
        line_length: 50,
        ..default_config()
    };
    let formatted = format_source(source, &config).unwrap();
    // Formatter should NOT align to opening paren — should use standard indent
    assert!(
        !formatted.contains("                                      longArgument2"),
        "should not align to opening paren, got:\n{formatted}"
    );
}

// --- 4. Maximum Line Length: Assignment Statements ---

#[test]
fn test_style_guide_assignment_wrapping_yes() {
    // Style guide "Yes": function call in assignment wraps args properly
    let source = r#"contract T {
    mapping(uint256 => mapping(uint256 => mapping(bool => uint256[]))) public data;
    function f() public {
        data[being][set][toSomeValue] = someFunction(
            argument1,
            argument2,
            argument3,
            argument4
        );
    }
}
"#;
    let config = FormatConfig {
        line_length: 60,
        ..default_config()
    };
    let formatted = format_source(source, &config).unwrap();
    assert!(
        formatted.contains("someFunction("),
        "should contain someFunction call, got:\n{formatted}"
    );
}

// --- 5. Maximum Line Length: Event Definitions ---

#[test]
fn test_style_guide_long_event_definition_yes() {
    // Style guide "Yes": event params each on own line
    let source = r#"contract T {
    event LongAndLotsOfArgs(
        address sender,
        address recipient,
        uint256 publicKey,
        uint256 amount,
        bytes32[] options
    );
}
"#;
    let config = FormatConfig {
        line_length: 50,
        ..default_config()
    };
    let formatted = format_source(source, &config).unwrap();
    assert!(
        formatted.contains("event LongAndLotsOfArgs("),
        "should contain event declaration, got:\n{formatted}"
    );
    // When wrapping, each param should be on its own line
    assert!(
        formatted.contains("address sender"),
        "should contain sender param, got:\n{formatted}"
    );
    assert!(
        formatted.contains("uint256 amount"),
        "should contain amount param, got:\n{formatted}"
    );
}

#[test]
fn test_style_guide_long_event_definition_no() {
    // Style guide "No": event params aligned to opening paren
    let source = r#"contract T {
    event LongAndLotsOfArgs(address sender,
                            address recipient,
                            uint256 publicKey,
                            uint256 amount,
                            bytes32[] options);
}
"#;
    let config = FormatConfig {
        line_length: 50,
        ..default_config()
    };
    let formatted = format_source(source, &config).unwrap();
    // Formatter should NOT use paren-alignment — should use standard indent
    assert!(
        !formatted.contains("                            address recipient"),
        "should not align params to opening paren, got:\n{formatted}"
    );
}

// --- 6. Imports: Placement ---

#[test]
fn test_style_guide_imports_before_contracts_yes() {
    // Style guide "Yes": imports come before contract declarations
    let source = r#"// SPDX-License-Identifier: GPL-3.0
pragma solidity >=0.4.0 <0.9.0;

import "./Owned.sol";

contract A {
}


contract B {
}
"#;
    let formatted = format_source(source, &default_config()).unwrap();
    let import_pos = formatted.find("import").unwrap();
    let contract_pos = formatted.find("contract").unwrap();
    assert!(
        import_pos < contract_pos,
        "import should come before contract"
    );
}

// --- 7. Whitespace in Expressions ---

#[test]
fn test_style_guide_no_whitespace_in_parens_yes() {
    // Style guide "Yes": no space inside parentheses, brackets, or braces
    let source = r#"contract T {
    uint256[] public ham;
    function f() public {
        ham[1];
    }
}
"#;
    let formatted = format_source(source, &default_config()).unwrap();
    assert!(
        formatted.contains("ham[1]"),
        "should have no spaces inside brackets, got:\n{formatted}"
    );
}

#[test]
fn test_style_guide_no_whitespace_in_parens_no() {
    // Style guide "No": extra spaces inside delimiters
    // The parser handles this transparently — formatter always outputs clean
    let source = r#"contract T {
    uint256[] public ham;
    function f() public {
        ham[ 1 ];
    }
}
"#;
    let formatted = format_source(source, &default_config()).unwrap();
    // Formatter should remove extra spaces inside brackets
    assert!(
        formatted.contains("ham[1]"),
        "should remove spaces inside brackets, got:\n{formatted}"
    );
}

// --- 8. No Space Before Commas/Semicolons ---

#[test]
fn test_style_guide_no_space_before_comma() {
    // Style guide "Yes": no space before comma
    let source = r#"contract T {
    function spam(uint256 i, uint256 j) public pure {}
}
"#;
    let formatted = format_source(source, &default_config()).unwrap();
    assert!(
        !formatted.contains(" ,"),
        "should not have space before comma, got:\n{formatted}"
    );
}

// --- 9. No Extra Alignment ---

#[test]
fn test_style_guide_no_alignment_yes() {
    // Style guide "Yes": single space around assignment
    let source = r#"contract T {
    function f() public {
        uint256 x = 1;
        uint256 y = 2;
        uint256 longVariable = 3;
    }
}
"#;
    let formatted = format_source(source, &default_config()).unwrap();
    assert!(
        formatted.contains("x = 1;"),
        "should have single space around =, got:\n{formatted}"
    );
    assert!(
        formatted.contains("y = 2;"),
        "should have single space around =, got:\n{formatted}"
    );
    assert!(
        formatted.contains("longVariable = 3;"),
        "should have single space around =, got:\n{formatted}"
    );
}

#[test]
fn test_style_guide_no_alignment_no() {
    // Style guide "No": multiple spaces for visual alignment
    let source = r#"contract T {
    function f() public {
        uint256 x            = 1;
        uint256 y            = 2;
        uint256 longVariable = 3;
    }
}
"#;
    let formatted = format_source(source, &default_config()).unwrap();
    // Formatter should remove alignment spaces
    assert!(
        formatted.contains("x = 1;"),
        "should remove alignment spaces, got:\n{formatted}"
    );
    assert!(
        !formatted.contains("x            = 1"),
        "should not have alignment spaces, got:\n{formatted}"
    );
}

// --- 10. Receive/Fallback: No Space Before Parens ---

#[test]
fn test_style_guide_receive_fallback_no_space_yes() {
    // Style guide "Yes": no space between function name and parens
    let source = r#"contract T {
    receive() external payable {}

    fallback() external {}
}
"#;
    let config = FormatConfig {
        contract_body_spacing: solgrid_config::ContractBodySpacing::Single,
        ..default_config()
    };
    let formatted = format_source(source, &config).unwrap();
    assert!(
        formatted.contains("receive()"),
        "should have no space before parens in receive, got:\n{formatted}"
    );
    assert!(
        formatted.contains("fallback()"),
        "should have no space before parens in fallback, got:\n{formatted}"
    );
}

#[test]
fn test_style_guide_receive_fallback_no_space_no() {
    // Style guide "No": space between function name and parens
    let source = r#"contract T {
    receive () external payable {}

    fallback () external {}
}
"#;
    let config = FormatConfig {
        contract_body_spacing: solgrid_config::ContractBodySpacing::Single,
        ..default_config()
    };
    let formatted = format_source(source, &config).unwrap();
    // Formatter should remove space between name and parens
    assert!(
        formatted.contains("receive()"),
        "should remove space before parens in receive, got:\n{formatted}"
    );
    assert!(
        formatted.contains("fallback()"),
        "should remove space before parens in fallback, got:\n{formatted}"
    );
}

// --- 11. Control Structures: Brace Placement ---

#[test]
fn test_style_guide_brace_same_line_yes() {
    // Style guide "Yes": opening brace on same line
    let source = r#"// SPDX-License-Identifier: GPL-3.0
pragma solidity >=0.4.0 <0.9.0;

contract Coin {
    struct Bank {
        address owner;
        uint256 balance;
    }
}
"#;
    let formatted = format_source(source, &default_config()).unwrap();
    assert!(
        formatted.contains("contract Coin {"),
        "contract brace should be on same line, got:\n{formatted}"
    );
    assert!(
        formatted.contains("struct Bank {"),
        "struct brace should be on same line, got:\n{formatted}"
    );
}

#[test]
fn test_style_guide_brace_same_line_no() {
    // Style guide "No": opening brace on next line
    let source = r#"// SPDX-License-Identifier: GPL-3.0
pragma solidity >=0.4.0 <0.9.0;

contract Coin
{
    struct Bank {
        address owner;
        uint256 balance;
    }
}
"#;
    let formatted = format_source(source, &default_config()).unwrap();
    // Formatter should put brace on same line
    assert!(
        formatted.contains("contract Coin {"),
        "formatter should put contract brace on same line, got:\n{formatted}"
    );
}

// --- 12. If/While/For: Keyword Spacing ---

#[test]
fn test_style_guide_control_keyword_spacing_yes() {
    // Style guide "Yes": space between keyword and paren
    let source = r#"contract T {
    function f(uint256 x) public pure {
        if (x > 0) {
            x = 1;
        }
        for (uint256 i = 0; i < 10; i++) {
            x = i;
        }
        while (x > 0) {
            x--;
        }
    }
}
"#;
    let formatted = format_source(source, &default_config()).unwrap();
    assert!(
        formatted.contains("if ("),
        "should have space after if, got:\n{formatted}"
    );
    assert!(
        formatted.contains("for ("),
        "should have space after for, got:\n{formatted}"
    );
    assert!(
        formatted.contains("while ("),
        "should have space after while, got:\n{formatted}"
    );
}

// --- 13. If/Else: Else Placement ---

#[test]
fn test_style_guide_else_same_line_yes() {
    // Style guide "Yes": else on same line as closing brace
    let source = r#"contract T {
    function f(uint256 x) public pure returns (uint256) {
        if (x < 3) {
            x += 1;
        } else if (x > 7) {
            x -= 1;
        } else {
            x = 5;
        }
        return x;
    }
}
"#;
    let formatted = format_source(source, &default_config()).unwrap();
    assert!(
        formatted.contains("} else if"),
        "else if should be on same line as closing brace, got:\n{formatted}"
    );
    assert!(
        formatted.contains("} else {"),
        "else should be on same line as closing brace, got:\n{formatted}"
    );
}

#[test]
fn test_style_guide_else_same_line_no() {
    // Style guide "No": else on new line after closing brace
    let source = r#"contract T {
    function f(uint256 x) public pure returns (uint256) {
        if (x < 3) {
            x += 1;
        }
        else {
            x -= 1;
        }
        return x;
    }
}
"#;
    let formatted = format_source(source, &default_config()).unwrap();
    // Formatter should put else on same line as closing brace
    assert!(
        formatted.contains("} else {"),
        "formatter should put else on same line, got:\n{formatted}"
    );
    assert!(
        !formatted.contains("}\n        else"),
        "should not have else on new line, got:\n{formatted}"
    );
}

// --- 14. Function Declaration: Brace Placement ---

#[test]
fn test_style_guide_function_brace_same_line_yes() {
    // Style guide "Yes": opening brace on same line as declaration
    let source = r#"contract T {
    function increment(uint256 x) public pure returns (uint256) {
        return x + 1;
    }
}
"#;
    let formatted = format_source(source, &default_config()).unwrap();
    // Brace should be on same line
    assert!(
        formatted.contains(") {"),
        "opening brace should be on same line, got:\n{formatted}"
    );
    // Closing brace should be at proper indentation
    assert!(
        formatted.contains("    }"),
        "closing brace should be indented, got:\n{formatted}"
    );
}

#[test]
fn test_style_guide_function_brace_same_line_no() {
    // Style guide "No": opening brace on next line
    let source = r#"contract T {
    function increment(uint256 x) public pure returns (uint256)
    {
        return x + 1;
    }
}
"#;
    let formatted = format_source(source, &default_config()).unwrap();
    // Formatter should put brace on same line
    assert!(
        formatted.contains(") {"),
        "formatter should put brace on same line, got:\n{formatted}"
    );
}

// --- 15. Function Declaration: Modifier Order ---

#[test]
fn test_style_guide_modifier_order_yes() {
    // Style guide "Yes": visibility → mutability → virtual → override → custom → returns
    let source = r#"contract T {
    mapping(address => uint256) balanceOf;
    function balance(uint256 from) public view override returns (uint256) {
        return balanceOf[from];
    }
}
"#;
    let formatted = format_source(source, &default_config()).unwrap();
    let public_pos = formatted.find("public").unwrap();
    let view_pos = formatted.find("view").unwrap();
    let override_pos = formatted.find("override").unwrap();
    let returns_pos = formatted.find("returns").unwrap();
    assert!(public_pos < view_pos, "public should come before view");
    assert!(view_pos < override_pos, "view should come before override");
    assert!(
        override_pos < returns_pos,
        "override should come before returns"
    );
}

#[test]
fn test_style_guide_modifier_order_no() {
    // Style guide "No": override before view
    let source = r#"contract T {
    mapping(address => uint256) balanceOf;
    function balance(uint256 from) public override view returns (uint256) {
        return balanceOf[from];
    }
}
"#;
    let formatted = format_source(source, &default_config()).unwrap();
    // Formatter should reorder: public view override returns
    let public_pos = formatted.find("public").unwrap();
    let view_pos = formatted.find("view").unwrap();
    let override_pos = formatted.find("override").unwrap();
    assert!(
        public_pos < view_pos && view_pos < override_pos,
        "formatter should reorder to: public view override, got:\n{formatted}"
    );
}

#[test]
fn test_style_guide_modifier_order_custom_no() {
    // Style guide "No": custom modifier before visibility
    let source = r#"contract T {
    function increment(uint256 x) onlyOwner public pure returns (uint256) {
        return x + 1;
    }
}
"#;
    let formatted = format_source(source, &default_config()).unwrap();
    // Formatter should reorder: public pure onlyOwner returns
    let public_pos = formatted.find("public").unwrap();
    let only_owner_pos = formatted.find("onlyOwner").unwrap();
    assert!(
        public_pos < only_owner_pos,
        "public should come before onlyOwner, got:\n{formatted}"
    );
}

// --- 16. Long Function Declarations ---

#[test]
fn test_style_guide_long_function_declaration_yes() {
    // Style guide "Yes": each parameter on own line, closing paren on own line
    let source = r#"contract T {
    function thisFunctionHasLotsOfArguments(
        address a,
        address b,
        address c,
        address d,
        address e,
        address f
    ) public {
        a;
    }
}
"#;
    let config = FormatConfig {
        line_length: 50,
        ..default_config()
    };
    let formatted = format_source(source, &config).unwrap();
    // Each address param should appear, properly wrapped
    assert!(
        formatted.contains("address a,"),
        "should contain address a, got:\n{formatted}"
    );
    assert!(
        formatted.contains("address f"),
        "should contain address f, got:\n{formatted}"
    );
}

#[test]
fn test_style_guide_long_function_declaration_no() {
    // Style guide "No": params on same line, cramped
    let source = r#"contract T {
    function thisFunctionHasLotsOfArguments(address a, address b, address c,
        address d, address e, address f) public {
        a;
    }
}
"#;
    let config = FormatConfig {
        line_length: 50,
        ..default_config()
    };
    let formatted = format_source(source, &config).unwrap();
    // Formatter should wrap each param — should not have multiple params on one line
    // (at line_length 50, `address a, address b, address c,` can't fit)
    assert!(
        !formatted.contains("address a, address b, address c,"),
        "should not cram params on one line, got:\n{formatted}"
    );
}

// --- 17. Modifiers on Separate Lines ---

#[test]
fn test_style_guide_modifiers_separate_lines_yes() {
    // Style guide "Yes": each modifier on its own indented line
    let source = r#"contract T {
    function thisFunctionNameIsReallyLong(address x, address y, address z)
        public
        pure
        returns (address)
    {
        return x;
    }
}
"#;
    let config = FormatConfig {
        line_length: 70,
        ..default_config()
    };
    let formatted = format_source(source, &config).unwrap();
    // Function should format with attributes, possibly wrapped
    assert!(
        formatted.contains("public"),
        "should contain public, got:\n{formatted}"
    );
    assert!(
        formatted.contains("pure"),
        "should contain pure, got:\n{formatted}"
    );
    assert!(
        formatted.contains("returns (address)"),
        "should contain returns, got:\n{formatted}"
    );
}

// --- 18. Multiline Return Statements ---

#[test]
fn test_style_guide_multiline_return_yes() {
    // Style guide "Yes": return params each on own line (function signature)
    let source = r#"contract T {
    function thisFunctionNameIsReallyLong(
        address a,
        address b,
        address c
    )
        public
        pure
        returns (
            address someAddressName,
            uint256 LongArgument,
            uint256 Argument
        )
    {
        return a;
    }
}
"#;
    let config = FormatConfig {
        line_length: 50,
        ..default_config()
    };
    let formatted = format_source(source, &config).unwrap();
    assert!(
        formatted.contains("returns (") || formatted.contains("returns("),
        "should contain returns clause, got:\n{formatted}"
    );
}

// --- 19. Constructor with Base Arguments ---

#[test]
fn test_style_guide_constructor_inheritance_yes() {
    // Style guide "Yes": base constructors on separate indented lines
    let source = r#"contract B {
    constructor(uint256 p) {}
}


contract C {
    constructor(uint256 p, uint256 q) {}
}


contract D {
    constructor(uint256 p) {}
}


contract A is B, C, D {
    uint256 x;

    constructor(uint256 param1, uint256 param2, uint256 param3, uint256 param4, uint256 param5)
        B(param1)
        C(param2, param3)
        D(param4)
    {
        x = param5;
    }
}
"#;
    let config = FormatConfig {
        line_length: 80,
        ..default_config()
    };
    let formatted = format_source(source, &config).unwrap();
    assert!(
        formatted.contains("B(param1)"),
        "should contain B(param1) base call, got:\n{formatted}"
    );
    assert!(
        formatted.contains("C(param2, param3)"),
        "should contain C(param2, param3) base call, got:\n{formatted}"
    );
    assert!(
        formatted.contains("D(param4)"),
        "should contain D(param4) base call, got:\n{formatted}"
    );
}

// --- 20. Mappings: No Space ---

#[test]
fn test_style_guide_mapping_no_space_yes() {
    // Style guide "Yes": no space between mapping and paren
    let source = r#"contract T {
    mapping(uint256 => uint256) map;
    mapping(address => bool) registeredAddresses;
    mapping(uint256 => mapping(bool => uint256[])) public data;
    mapping(uint256 => mapping(uint256 => uint256)) data2;
}
"#;
    let formatted = format_source(source, &default_config()).unwrap();
    assert!(
        formatted.contains("mapping(uint256 => uint256)"),
        "should have no space after mapping keyword, got:\n{formatted}"
    );
    assert!(
        formatted.contains("mapping(address => bool)"),
        "should have no space after mapping keyword, got:\n{formatted}"
    );
    // Nested mappings
    assert!(
        formatted.contains("mapping(uint256 => mapping("),
        "nested mapping should have no space, got:\n{formatted}"
    );
}

#[test]
fn test_style_guide_mapping_no_space_no() {
    // Style guide "No": space after mapping keyword
    let source = r#"contract T {
    mapping (uint256 => uint256) map;
    mapping( address => bool ) registeredAddresses;
    mapping (uint256 => mapping (bool => uint256[])) public data;
}
"#;
    let formatted = format_source(source, &default_config()).unwrap();
    // Formatter should remove extra spaces
    assert!(
        formatted.contains("mapping(uint256 => uint256)"),
        "should remove space after mapping keyword, got:\n{formatted}"
    );
    assert!(
        formatted.contains("mapping(address => bool)"),
        "should remove spaces inside mapping parens, got:\n{formatted}"
    );
}

// --- 21. Variable Declarations: Array Type ---

#[test]
fn test_style_guide_array_type_no_space_yes() {
    // Style guide "Yes": no space between type and brackets
    let source = r#"contract T {
    uint256[] x;
}
"#;
    let formatted = format_source(source, &default_config()).unwrap();
    assert!(
        formatted.contains("uint256[]"),
        "should have no space between type and brackets, got:\n{formatted}"
    );
}

#[test]
fn test_style_guide_array_type_no_space_no() {
    // Style guide "No": space between type and brackets
    let source = r#"contract T {
    uint256 [] x;
}
"#;
    let formatted = format_source(source, &default_config()).unwrap();
    // Formatter should remove space between type and brackets
    assert!(
        formatted.contains("uint256[]"),
        "should remove space between type and brackets, got:\n{formatted}"
    );
}

// --- 22. Strings: Double Quotes ---

#[test]
fn test_style_guide_double_quotes_yes() {
    // Style guide "Yes": double quotes for strings
    let source = r#"contract T {
    string public str = "foo";
}
"#;
    let formatted = format_source(source, &default_config()).unwrap();
    assert!(
        formatted.contains("\"foo\""),
        "should use double quotes, got:\n{formatted}"
    );
}

#[test]
fn test_style_guide_double_quotes_no() {
    // Style guide "No": single quotes for strings
    let source = r#"contract T {
    string public str = 'bar';
}
"#;
    let formatted = format_source(source, &default_config()).unwrap();
    // Default config uses double quotes, so single quotes should be converted
    assert!(
        formatted.contains("\"bar\""),
        "should convert single quotes to double quotes, got:\n{formatted}"
    );
    assert!(
        !formatted.contains("'bar'"),
        "should not contain single quotes, got:\n{formatted}"
    );
}

// --- 23. Operators: Spacing ---

#[test]
fn test_style_guide_operator_spacing_yes() {
    // Style guide "Yes": space around operators
    let source = r#"contract T {
    function f() public pure {
        uint256 x = 3;
        x = 100 / 10;
        x += 3 + 4;
    }
}
"#;
    let formatted = format_source(source, &default_config()).unwrap();
    assert!(
        formatted.contains("x = 3;"),
        "should have space around =, got:\n{formatted}"
    );
    assert!(
        formatted.contains("100 / 10"),
        "should have space around /, got:\n{formatted}"
    );
    assert!(
        formatted.contains("3 + 4"),
        "should have space around +, got:\n{formatted}"
    );
}

#[test]
fn test_style_guide_operator_spacing_no() {
    // Style guide "No": no space around operators
    let source = r#"contract T {
    function f() public pure {
        uint256 x=3;
        x = 100/10;
        x += 3+4;
    }
}
"#;
    let formatted = format_source(source, &default_config()).unwrap();
    // Formatter should add spaces around operators
    assert!(
        formatted.contains("x = 3;"),
        "should add space around =, got:\n{formatted}"
    );
    assert!(
        formatted.contains("100 / 10"),
        "should add space around /, got:\n{formatted}"
    );
    assert!(
        formatted.contains("3 + 4"),
        "should add space around +, got:\n{formatted}"
    );
}

// --- Idempotency: All style guide "Yes" examples ---

#[test]
fn test_style_guide_idempotency_contract_spacing() {
    let source = r#"// SPDX-License-Identifier: GPL-3.0
pragma solidity >=0.4.0 <0.9.0;

contract A {}


contract B {}


contract C {}
"#;
    let result = format_source_verified(source, &default_config());
    assert!(
        result.is_ok(),
        "style guide contract spacing should be idempotent: {}",
        result.unwrap_err()
    );
}

#[test]
fn test_style_guide_idempotency_if_else() {
    let source = r#"contract T {
    function f(uint256 x) public pure returns (uint256) {
        if (x < 3) {
            x += 1;
        } else if (x > 7) {
            x -= 1;
        } else {
            x = 5;
        }
        return x;
    }
}
"#;
    let result = format_source_verified(source, &default_config());
    assert!(
        result.is_ok(),
        "style guide if/else should be idempotent: {}",
        result.unwrap_err()
    );
}

#[test]
fn test_style_guide_idempotency_function_declaration() {
    let source = r#"contract T {
    function increment(uint256 x) public pure returns (uint256) {
        return x + 1;
    }
}
"#;
    let result = format_source_verified(source, &default_config());
    assert!(
        result.is_ok(),
        "style guide function decl should be idempotent: {}",
        result.unwrap_err()
    );
}

#[test]
fn test_style_guide_idempotency_mapping() {
    let source = r#"contract T {
    mapping(uint256 => uint256) map;
    mapping(address => bool) registeredAddresses;
    mapping(uint256 => mapping(bool => uint256[])) public data;
}
"#;
    let result = format_source_verified(source, &default_config());
    assert!(
        result.is_ok(),
        "style guide mapping should be idempotent: {}",
        result.unwrap_err()
    );
}

#[test]
fn test_style_guide_idempotency_operator_spacing() {
    let source = r#"contract T {
    function f() public pure {
        uint256 x = 3;
        x = 100 / 10;
        x += 3 + 4;
    }
}
"#;
    let result = format_source_verified(source, &default_config());
    assert!(
        result.is_ok(),
        "style guide operator spacing should be idempotent: {}",
        result.unwrap_err()
    );
}

#[test]
fn test_style_guide_idempotency_struct_brace() {
    let source = r#"// SPDX-License-Identifier: GPL-3.0
pragma solidity >=0.4.0 <0.9.0;

contract Coin {
    struct Bank {
        address owner;
        uint256 balance;
    }
}
"#;
    let result = format_source_verified(source, &default_config());
    assert!(
        result.is_ok(),
        "style guide struct brace should be idempotent: {}",
        result.unwrap_err()
    );
}

#[test]
fn test_style_guide_idempotency_receive_fallback() {
    let source = r#"contract T {
    receive() external payable {}

    fallback() external {}
}
"#;
    let config = FormatConfig {
        contract_body_spacing: solgrid_config::ContractBodySpacing::Preserve,
        ..default_config()
    };
    let result = format_source_verified(source, &config);
    assert!(
        result.is_ok(),
        "style guide receive/fallback should be idempotent: {}",
        result.unwrap_err()
    );
}

#[test]
fn test_style_guide_idempotency_emit_statement() {
    let source = r#"contract T {
    event LongAndLotsOfArgs(
        address sender,
        address recipient,
        uint256 publicKey,
        uint256 amount,
        bytes32[] options
    );

    function f(address sender, address recipient, uint256 publicKey, uint256 amount, bytes32[] memory options)
        public
    {
        emit LongAndLotsOfArgs(sender, recipient, publicKey, amount, options);
    }
}
"#;
    let result = format_source_verified(source, &default_config());
    assert!(
        result.is_ok(),
        "style guide emit statement should be idempotent: {}",
        result.unwrap_err()
    );
}
