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
fn test_preserve_assembly_comment_without_duplication() {
    let source = r#"contract T {
    function f() public pure returns (uint256 result) {
        assembly {
            // load result
            result := 1
        }
    }
}
"#;
    let expected = r#"contract T {
    function f() public pure returns (uint256 result) {
        assembly {
            // load result
            result := 1
        }
    }
}
"#;

    let formatted = format_source(source, &default_config()).unwrap();
    assert_eq!(formatted, expected);
    assert_eq!(formatted.matches("// load result").count(), 1);

    let reformatted = format_source(&formatted, &default_config()).unwrap();
    assert_eq!(reformatted, expected);
}

#[test]
fn test_keep_wrapped_constant_initializer_indented_after_equals() {
    let source = r#"abstract contract CCIPBatcher is CCIPReader {
    uint256 constant FLAGS_ANY_ERROR =
        FLAG_CALL_ERROR | FLAG_BATCH_ERROR | FLAG_EMPTY_RESPONSE;
}
"#;
    let expected = r#"abstract contract CCIPBatcher is CCIPReader {
    uint256 constant FLAGS_ANY_ERROR =
        FLAG_CALL_ERROR | FLAG_BATCH_ERROR | FLAG_EMPTY_RESPONSE;
}
"#;

    let formatted = format_source(source, &default_config()).unwrap();
    assert_eq!(formatted, expected);
}

#[test]
fn test_preserve_struct_field_trailing_comments() {
    let source = r#"contract T {
    /// @dev An independent `OffchainLookup` session.
    struct Lookup {
        address target; // contract to call
        bytes call; // initial calldata
        bytes data; // response or error
        uint256 flags; // see: FLAG_*
    }
}
"#;
    let expected = r#"contract T {
    /// @dev An independent `OffchainLookup` session.
    struct Lookup {
        address target; // contract to call
        bytes call; // initial calldata
        bytes data; // response or error
        uint256 flags; // see: FLAG_*
    }
}
"#;

    let formatted = format_source(source, &default_config()).unwrap();
    assert_eq!(formatted, expected);
}

#[test]
fn test_keep_ternary_continuation_indented() {
    let source = r#"contract T {
    function f(Lookup memory lu) internal view returns (uint256) {
        uint256 flags = detectEIP140(lu.target)
            ? FLAG_EIP140_AFTER
            : FLAG_EIP140_BEFORE;
    }
}
"#;
    let expected = r#"contract T {
    function f(Lookup memory lu) internal view returns (uint256) {
        uint256 flags = detectEIP140(lu.target)
            ? FLAG_EIP140_AFTER
            : FLAG_EIP140_BEFORE;
    }
}
"#;

    let formatted = format_source(source, &default_config()).unwrap();
    assert_eq!(formatted, expected);
}

#[test]
fn test_preserve_comments_inside_empty_if_block() {
    let source = r#"contract T {
    function f(bool unsafe, bytes memory v, bool ok, Lookup memory lu) internal {
        if (unsafe && v.length == 0) {
            // unsafe contracts appear the same for throw and unimplemented fallback
            // decision: interpret like an unimplemented function selector response
        } else if (!ok) {
            lu.flags |= FLAG_CALL_ERROR;
        }
    }
}
"#;
    let expected = r#"contract T {
    function f(bool unsafe, bytes memory v, bool ok, Lookup memory lu) internal {
        if (unsafe && v.length == 0) {
            // unsafe contracts appear the same for throw and unimplemented fallback
            // decision: interpret like an unimplemented function selector response
        } else if (!ok) {
            lu.flags |= FLAG_CALL_ERROR;
        }
    }
}
"#;

    let formatted = format_source(source, &default_config()).unwrap();
    assert_eq!(formatted, expected);
}

#[test]
fn test_preserve_single_blank_line_within_function_body() {
    let source = r#"contract T {
    function f() internal pure {
        uint256 x = 1;

        uint256 y = 2;
    }
}
"#;
    let expected = r#"contract T {
    function f() internal pure {
        uint256 x = 1;

        uint256 y = 2;
    }
}
"#;

    let formatted = format_source(source, &default_config()).unwrap();
    assert_eq!(formatted, expected);
}

#[test]
fn test_preserve_blank_line_below_top_level_comment() {
    let source = r#"// Something

contract Thing {}
"#;
    let expected = r#"// Something

contract Thing {}
"#;

    let formatted = format_source(source, &default_config()).unwrap();
    assert_eq!(formatted, expected);
}

#[test]
fn test_preserve_blank_line_below_contract_comment_block() {
    let source = r#"contract Thing {
    /* ERC721 internal functions */

    /// @dev Approve `to` to operate on `tokenId`
    /// Emits an {Approval} event.
    function _approve(address to, uint256 tokenId) internal virtual {}
}
"#;
    let expected = r#"contract Thing {
    /* ERC721 internal functions */

    /// @dev Approve `to` to operate on `tokenId`
    /// Emits an {Approval} event.
    function _approve(address to, uint256 tokenId) internal virtual {}
}
"#;

    let formatted = format_source(source, &default_config()).unwrap();
    assert_eq!(formatted, expected);
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
fn test_preserve_regular_line_comment_text() {
    let source = "//check this stays exact\npragma solidity ^0.8.0;\n";
    let formatted = format_source(source, &default_config()).unwrap();
    assert!(formatted.contains("//check this stays exact"));
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

#[test]
fn test_sort_imports_respects_import_groups() {
    let source = r#"import "./Local.sol";
import "@openzeppelin/contracts/access/Ownable.sol";
import "forge-std/Test.sol";
import "../Parent.sol";
"#;
    let config = FormatConfig {
        sort_imports: true,
        import_order: vec![
            "^forge-std/".into(),
            "^@?\\w".into(),
            "^\\.\\./".into(),
            "^\\./".into(),
        ],
        ..default_config()
    };
    let formatted = format_source(source, &config).unwrap();
    assert!(formatted.contains(
        "import \"forge-std/Test.sol\";\n\nimport \"@openzeppelin/contracts/access/Ownable.sol\";\n\nimport \"../Parent.sol\";\n\nimport \"./Local.sol\";"
    ));
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

#[test]
fn test_insert_blank_lines_between_import_groups() {
    let source = r#"import "@openzeppelin/contracts/access/Ownable.sol";
import "@openzeppelin/contracts/utils/introspection/ERC165.sol";
import "../utils/LibString.sol";
import "./StandaloneReverseRegistrar.sol";
"#;
    let formatted = format_source(source, &default_config()).unwrap();
    assert!(
        formatted.contains(
            "ERC165.sol\";\n\nimport \"../utils/LibString.sol\";\n\nimport \"./StandaloneReverseRegistrar.sol\";"
        ),
        "expected canonical blank lines between import groups, got:\n{formatted}"
    );
}

#[test]
fn test_one_blank_line_between_import_and_contract_with_doc_comment() {
    let source = r#"import "./Foo.sol";

/// A doc comment.
contract A {}
"#;
    let formatted = format_source(source, &default_config()).unwrap();
    assert!(
        formatted.contains("\"./Foo.sol\";\n\n/// A doc comment."),
        "expected 1 blank line between import and doc comment, got:\n{formatted}"
    );
    assert!(
        !formatted.contains("\"./Foo.sol\";\n\n\n/// A doc comment."),
        "should NOT have 2 blank lines between import and doc comment, got:\n{formatted}"
    );
}

#[test]
fn test_one_blank_line_between_import_and_interface_with_comment() {
    let source = r#"import "./Foo.sol";
/// Interface doc.
interface IFoo {}
"#;
    let formatted = format_source(source, &default_config()).unwrap();
    assert!(
        formatted.contains("\"./Foo.sol\";\n\n/// Interface doc."),
        "expected 1 blank line between import and commented interface, got:\n{formatted}"
    );
    assert!(
        !formatted.contains("\"./Foo.sol\";\n\n\n/// Interface doc."),
        "should NOT have 2 blank lines between import and commented interface, got:\n{formatted}"
    );
}

#[test]
fn test_one_blank_line_between_multiple_imports_and_contract_with_comment() {
    let source = r#"import "./Foo.sol";
import "./Bar.sol";
/// Contract doc.
contract A {}
"#;
    let formatted = format_source(source, &default_config()).unwrap();
    assert!(
        formatted.contains("\"./Bar.sol\";\n\n/// Contract doc."),
        "expected 1 blank line between last import and doc comment, got:\n{formatted}"
    );
    assert!(
        !formatted.contains("\"./Bar.sol\";\n\n\n/// Contract doc."),
        "should NOT have 2 blank lines between last import and doc comment, got:\n{formatted}"
    );
}

#[test]
fn test_two_blank_lines_between_contracts_with_doc_comment() {
    let source = "contract A {}\n/// Doc for B.\ncontract B {}\n";
    let formatted = format_source(source, &default_config()).unwrap();
    assert!(
        formatted.contains("}\n\n\n/// Doc for B."),
        "expected 2 blank lines between contract and next doc comment, got:\n{formatted}"
    );
    assert!(
        !formatted.contains("}\n\n\n\n/// Doc for B."),
        "should NOT have 3 blank lines between contract and next doc comment, got:\n{formatted}"
    );
}

#[test]
fn test_one_blank_line_between_top_level_constants() {
    let source = r#"uint16 constant A = 1;
/// B doc.
uint16 constant B = 2;
"#;
    let formatted = format_source(source, &default_config()).unwrap();
    assert_eq!(
        formatted,
        r#"uint16 constant A = 1;

/// B doc.
uint16 constant B = 2;
"#
    );
}

// ============================================================================
// Solidity Style Guide — Yes/No Example Tests
// https://docs.soliditylang.org/en/latest/style-guide.html
// ============================================================================

// --- 1. Blank Lines: Two blank lines between top-level declarations ---

const STYLE_GUIDE_01_BLANK_LINES_YES: &str = r#"// SPDX-License-Identifier: GPL-3.0
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

#[test]
fn test_style_guide_blank_lines_between_contracts_yes() {
    let formatted = format_source(STYLE_GUIDE_01_BLANK_LINES_YES, &default_config()).unwrap();
    assert_eq!(formatted, STYLE_GUIDE_01_BLANK_LINES_YES);
}

#[test]
fn test_style_guide_blank_lines_between_contracts_no() {
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
    assert_eq!(formatted, STYLE_GUIDE_01_BLANK_LINES_YES);
}

// --- 2. Blank Lines: Functions within contract ---

const STYLE_GUIDE_02_FUNCTION_SPACING_YES: &str = r#"contract B {
    function spam() public pure returns (uint256) {
        return 1;
    }

    function ham() public pure returns (uint256) {
        return 2;
    }
}
"#;

#[test]
fn test_style_guide_function_spacing_yes() {
    let config = FormatConfig {
        contract_body_spacing: solgrid_config::ContractBodySpacing::Single,
        ..default_config()
    };
    let formatted = format_source(STYLE_GUIDE_02_FUNCTION_SPACING_YES, &config).unwrap();
    assert_eq!(formatted, STYLE_GUIDE_02_FUNCTION_SPACING_YES);
}

#[test]
fn test_style_guide_function_spacing_no() {
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
    assert_eq!(formatted, STYLE_GUIDE_02_FUNCTION_SPACING_YES);
}

// --- 3. Maximum Line Length: Function Calls ---

const STYLE_GUIDE_03_LONG_FUNCTION_CALL_YES: &str = r#"contract T {
    function f() public {
        thisFunctionCallIsReallyLong(
            longArgument1,
            longArgument2,
            longArgument3
        );
    }
}
"#;

#[test]
fn test_style_guide_long_function_call_yes() {
    let config = FormatConfig {
        line_length: 50,
        ..default_config()
    };
    let formatted = format_source(STYLE_GUIDE_03_LONG_FUNCTION_CALL_YES, &config).unwrap();
    assert_eq!(formatted, STYLE_GUIDE_03_LONG_FUNCTION_CALL_YES);
}

#[test]
fn test_style_guide_long_function_call_no_aligned_to_paren() {
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
    assert_eq!(formatted, STYLE_GUIDE_03_LONG_FUNCTION_CALL_YES);
}

// --- 4. Maximum Line Length: Assignment Statements ---

const STYLE_GUIDE_04_ASSIGNMENT_WRAPPING_YES: &str = r#"contract T {
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

#[test]
fn test_style_guide_assignment_wrapping_yes() {
    let config = FormatConfig {
        line_length: 60,
        ..default_config()
    };
    let formatted = format_source(STYLE_GUIDE_04_ASSIGNMENT_WRAPPING_YES, &config).unwrap();
    assert_eq!(formatted, STYLE_GUIDE_04_ASSIGNMENT_WRAPPING_YES);
}

// --- 5. Maximum Line Length: Event Definitions ---

const STYLE_GUIDE_05_LONG_EVENT_YES: &str = r#"contract T {
    event LongAndLotsOfArgs(
        address sender,
        address recipient,
        uint256 publicKey,
        uint256 amount,
        bytes32[] options
    );
}
"#;

#[test]
fn test_style_guide_long_event_definition_yes() {
    let config = FormatConfig {
        line_length: 50,
        ..default_config()
    };
    let formatted = format_source(STYLE_GUIDE_05_LONG_EVENT_YES, &config).unwrap();
    assert_eq!(formatted, STYLE_GUIDE_05_LONG_EVENT_YES);
}

#[test]
fn test_style_guide_long_event_definition_no() {
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
    assert_eq!(formatted, STYLE_GUIDE_05_LONG_EVENT_YES);
}

// --- 6. Imports: Placement ---

const STYLE_GUIDE_06_IMPORTS_YES: &str = r#"// SPDX-License-Identifier: GPL-3.0
pragma solidity >=0.4.0 <0.9.0;

import "./Owned.sol";

contract A {}


contract B {}
"#;

#[test]
fn test_style_guide_imports_before_contracts_yes() {
    let formatted = format_source(STYLE_GUIDE_06_IMPORTS_YES, &default_config()).unwrap();
    assert_eq!(formatted, STYLE_GUIDE_06_IMPORTS_YES);
}

// --- 7. Whitespace in Expressions ---

const STYLE_GUIDE_07_NO_WHITESPACE_YES: &str = r#"contract T {
    uint256[] public ham;
    function f() public {
        ham[1];
    }
}
"#;

#[test]
fn test_style_guide_no_whitespace_in_parens_yes() {
    let formatted = format_source(STYLE_GUIDE_07_NO_WHITESPACE_YES, &default_config()).unwrap();
    assert_eq!(formatted, STYLE_GUIDE_07_NO_WHITESPACE_YES);
}

#[test]
fn test_style_guide_no_whitespace_in_parens_no() {
    let source = r#"contract T {
    uint256[] public ham;
    function f() public {
        ham[ 1 ];
    }
}
"#;
    let formatted = format_source(source, &default_config()).unwrap();
    assert_eq!(formatted, STYLE_GUIDE_07_NO_WHITESPACE_YES);
}

// --- 8. No Space Before Commas/Semicolons ---

const STYLE_GUIDE_08_NO_SPACE_COMMA_YES: &str = r#"contract T {
    function spam(uint256 i, uint256 j) public pure {}
}
"#;

#[test]
fn test_style_guide_no_space_before_comma() {
    let formatted = format_source(STYLE_GUIDE_08_NO_SPACE_COMMA_YES, &default_config()).unwrap();
    assert_eq!(formatted, STYLE_GUIDE_08_NO_SPACE_COMMA_YES);
}

// --- 9. No Extra Alignment ---

const STYLE_GUIDE_09_NO_ALIGNMENT_YES: &str = r#"contract T {
    function f() public {
        uint256 x = 1;
        uint256 y = 2;
        uint256 longVariable = 3;
    }
}
"#;

#[test]
fn test_style_guide_no_alignment_yes() {
    let formatted = format_source(STYLE_GUIDE_09_NO_ALIGNMENT_YES, &default_config()).unwrap();
    assert_eq!(formatted, STYLE_GUIDE_09_NO_ALIGNMENT_YES);
}

#[test]
fn test_style_guide_no_alignment_no() {
    let source = r#"contract T {
    function f() public {
        uint256 x            = 1;
        uint256 y            = 2;
        uint256 longVariable = 3;
    }
}
"#;
    let formatted = format_source(source, &default_config()).unwrap();
    assert_eq!(formatted, STYLE_GUIDE_09_NO_ALIGNMENT_YES);
}

// --- 10. Receive/Fallback: No Space Before Parens ---

const STYLE_GUIDE_10_RECEIVE_FALLBACK_YES: &str = r#"contract T {
    receive() external payable {}

    fallback() external {}
}
"#;

#[test]
fn test_style_guide_receive_fallback_no_space_yes() {
    let config = FormatConfig {
        contract_body_spacing: solgrid_config::ContractBodySpacing::Single,
        ..default_config()
    };
    let formatted = format_source(STYLE_GUIDE_10_RECEIVE_FALLBACK_YES, &config).unwrap();
    assert_eq!(formatted, STYLE_GUIDE_10_RECEIVE_FALLBACK_YES);
}

#[test]
fn test_style_guide_receive_fallback_no_space_no() {
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
    assert_eq!(formatted, STYLE_GUIDE_10_RECEIVE_FALLBACK_YES);
}

// --- 11. Control Structures: Brace Placement ---

const STYLE_GUIDE_11_BRACE_SAME_LINE_YES: &str = r#"// SPDX-License-Identifier: GPL-3.0
pragma solidity >=0.4.0 <0.9.0;

contract Coin {
    struct Bank {
        address owner;
        uint256 balance;
    }
}
"#;

#[test]
fn test_style_guide_brace_same_line_yes() {
    let formatted = format_source(STYLE_GUIDE_11_BRACE_SAME_LINE_YES, &default_config()).unwrap();
    assert_eq!(formatted, STYLE_GUIDE_11_BRACE_SAME_LINE_YES);
}

#[test]
fn test_style_guide_brace_same_line_no() {
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
    assert_eq!(formatted, STYLE_GUIDE_11_BRACE_SAME_LINE_YES);
}

// --- 12. If/While/For: Keyword Spacing ---

const STYLE_GUIDE_12_CONTROL_KEYWORD_YES: &str = r#"contract T {
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

#[test]
fn test_style_guide_control_keyword_spacing_yes() {
    let formatted = format_source(STYLE_GUIDE_12_CONTROL_KEYWORD_YES, &default_config()).unwrap();
    assert_eq!(formatted, STYLE_GUIDE_12_CONTROL_KEYWORD_YES);
}

// --- 13. If/Else: Else Placement ---

const STYLE_GUIDE_13_ELSE_SAME_LINE_YES: &str = r#"contract T {
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

#[test]
fn test_style_guide_else_same_line_yes() {
    let formatted = format_source(STYLE_GUIDE_13_ELSE_SAME_LINE_YES, &default_config()).unwrap();
    assert_eq!(formatted, STYLE_GUIDE_13_ELSE_SAME_LINE_YES);
}

#[test]
fn test_style_guide_else_same_line_no() {
    let source = r#"contract T {
    function f(uint256 x) public pure returns (uint256) {
        if (x < 3) {
            x += 1;
        }
        else if (x > 7) {
            x -= 1;
        }
        else {
            x = 5;
        }
        return x;
    }
}
"#;
    let formatted = format_source(source, &default_config()).unwrap();
    assert_eq!(formatted, STYLE_GUIDE_13_ELSE_SAME_LINE_YES);
}

// --- 14. Function Declaration: Brace Placement ---

const STYLE_GUIDE_14_FUNC_BRACE_YES: &str = r#"contract T {
    function increment(uint256 x) public pure returns (uint256) {
        return x + 1;
    }
}
"#;

#[test]
fn test_style_guide_function_brace_same_line_yes() {
    let formatted = format_source(STYLE_GUIDE_14_FUNC_BRACE_YES, &default_config()).unwrap();
    assert_eq!(formatted, STYLE_GUIDE_14_FUNC_BRACE_YES);
}

#[test]
fn test_style_guide_function_brace_same_line_no() {
    let source = r#"contract T {
    function increment(uint256 x) public pure returns (uint256)
    {
        return x + 1;
    }
}
"#;
    let formatted = format_source(source, &default_config()).unwrap();
    assert_eq!(formatted, STYLE_GUIDE_14_FUNC_BRACE_YES);
}

// --- 15. Function Declaration: Modifier Order ---

const STYLE_GUIDE_15_MODIFIER_ORDER_YES: &str = r#"contract T {
    mapping(address => uint256) balanceOf;
    function balance(uint256 from) public view override returns (uint256) {
        return balanceOf[from];
    }
}
"#;

const STYLE_GUIDE_15_MODIFIER_ORDER_CUSTOM_YES: &str = r#"contract T {
    function increment(uint256 x) public pure onlyOwner returns (uint256) {
        return x + 1;
    }
}
"#;

#[test]
fn test_style_guide_modifier_order_yes() {
    let formatted = format_source(STYLE_GUIDE_15_MODIFIER_ORDER_YES, &default_config()).unwrap();
    assert_eq!(formatted, STYLE_GUIDE_15_MODIFIER_ORDER_YES);
}

#[test]
fn test_style_guide_modifier_order_no() {
    let source = r#"contract T {
    mapping(address => uint256) balanceOf;
    function balance(uint256 from) public override view returns (uint256) {
        return balanceOf[from];
    }
}
"#;
    let formatted = format_source(source, &default_config()).unwrap();
    assert_eq!(formatted, STYLE_GUIDE_15_MODIFIER_ORDER_YES);
}

#[test]
fn test_style_guide_modifier_order_custom_no() {
    let source = r#"contract T {
    function increment(uint256 x) onlyOwner public pure returns (uint256) {
        return x + 1;
    }
}
"#;
    let formatted = format_source(source, &default_config()).unwrap();
    assert_eq!(formatted, STYLE_GUIDE_15_MODIFIER_ORDER_CUSTOM_YES);
}

// --- 16. Long Function Declarations ---

const STYLE_GUIDE_16_LONG_FUNC_DECL_YES: &str = r#"contract T {
    function thisFunctionHasLotsOfArguments(
        address a,
        address b,
        address c,
        address d,
        address e,
        address f
    )
        public
    {
        a;
    }
}
"#;

#[test]
fn test_style_guide_long_function_declaration_yes() {
    let config = FormatConfig {
        line_length: 50,
        ..default_config()
    };
    let formatted = format_source(STYLE_GUIDE_16_LONG_FUNC_DECL_YES, &config).unwrap();
    assert_eq!(formatted, STYLE_GUIDE_16_LONG_FUNC_DECL_YES);
}

#[test]
fn test_style_guide_long_function_declaration_no() {
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
    assert_eq!(formatted, STYLE_GUIDE_16_LONG_FUNC_DECL_YES);
}

// --- 17. Modifiers on Separate Lines ---

const STYLE_GUIDE_17_MODIFIERS_LINES_YES: &str = r#"contract T {
    function thisFunctionNameIsReallyLong(address x, address y, address z)
        public
        pure
        returns (address)
    {
        return x;
    }
}
"#;

#[test]
fn test_style_guide_modifiers_separate_lines_yes() {
    let config = FormatConfig {
        line_length: 80,
        ..default_config()
    };
    let formatted = format_source(STYLE_GUIDE_17_MODIFIERS_LINES_YES, &config).unwrap();
    assert_eq!(formatted, STYLE_GUIDE_17_MODIFIERS_LINES_YES);
}

// --- 18. Multiline Return Statements ---

const STYLE_GUIDE_18_MULTILINE_RETURN_YES: &str = r#"contract T {
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

#[test]
fn test_style_guide_multiline_return_yes() {
    let config = FormatConfig {
        line_length: 50,
        ..default_config()
    };
    let formatted = format_source(STYLE_GUIDE_18_MULTILINE_RETURN_YES, &config).unwrap();
    assert_eq!(formatted, STYLE_GUIDE_18_MULTILINE_RETURN_YES);
}

// --- 19. Constructor with Base Arguments ---

const STYLE_GUIDE_19_CONSTRUCTOR_INHERIT_YES: &str = r#"contract B {
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

#[test]
fn test_style_guide_constructor_inheritance_yes() {
    let config = FormatConfig {
        line_length: 100,
        ..default_config()
    };
    let formatted = format_source(STYLE_GUIDE_19_CONSTRUCTOR_INHERIT_YES, &config).unwrap();
    assert_eq!(formatted, STYLE_GUIDE_19_CONSTRUCTOR_INHERIT_YES);
}

// --- 20. Mappings: No Space ---

const STYLE_GUIDE_20_MAPPING_YES: &str = r#"contract T {
    mapping(uint256 => uint256) map;
    mapping(address => bool) registeredAddresses;
    mapping(uint256 => mapping(bool => uint256[])) public data;
    mapping(uint256 => mapping(uint256 => uint256)) data2;
}
"#;

#[test]
fn test_style_guide_mapping_no_space_yes() {
    let formatted = format_source(STYLE_GUIDE_20_MAPPING_YES, &default_config()).unwrap();
    assert_eq!(formatted, STYLE_GUIDE_20_MAPPING_YES);
}

#[test]
fn test_style_guide_mapping_no_space_no() {
    let source = r#"contract T {
    mapping (uint256 => uint256) map;
    mapping( address => bool ) registeredAddresses;
    mapping (uint256 => mapping (bool => uint256[])) public data;
    mapping (uint256 => mapping (uint256 => uint256)) data2;
}
"#;
    let formatted = format_source(source, &default_config()).unwrap();
    assert_eq!(formatted, STYLE_GUIDE_20_MAPPING_YES);
}

// --- 21. Variable Declarations: Array Type ---

const STYLE_GUIDE_21_ARRAY_TYPE_YES: &str = r#"contract T {
    uint256[] x;
}
"#;

#[test]
fn test_style_guide_array_type_no_space_yes() {
    let formatted = format_source(STYLE_GUIDE_21_ARRAY_TYPE_YES, &default_config()).unwrap();
    assert_eq!(formatted, STYLE_GUIDE_21_ARRAY_TYPE_YES);
}

#[test]
fn test_style_guide_array_type_no_space_no() {
    let source = r#"contract T {
    uint256 [] x;
}
"#;
    let formatted = format_source(source, &default_config()).unwrap();
    assert_eq!(formatted, STYLE_GUIDE_21_ARRAY_TYPE_YES);
}

// --- 22. Strings: Double Quotes ---

const STYLE_GUIDE_22_DOUBLE_QUOTES_YES: &str = r#"contract T {
    string public str = "foo";
}
"#;

#[test]
fn test_style_guide_double_quotes_yes() {
    let formatted = format_source(STYLE_GUIDE_22_DOUBLE_QUOTES_YES, &default_config()).unwrap();
    assert_eq!(formatted, STYLE_GUIDE_22_DOUBLE_QUOTES_YES);
}

#[test]
fn test_style_guide_double_quotes_no() {
    let source = r#"contract T {
    string public str = 'foo';
}
"#;
    let formatted = format_source(source, &default_config()).unwrap();
    assert_eq!(formatted, STYLE_GUIDE_22_DOUBLE_QUOTES_YES);
}

// --- 23. Operators: Spacing ---

const STYLE_GUIDE_23_OPERATOR_SPACING_YES: &str = r#"contract T {
    function f() public pure {
        uint256 x = 3;
        x = 100 / 10;
        x += 3 + 4;
    }
}
"#;

#[test]
fn test_style_guide_operator_spacing_yes() {
    let formatted = format_source(STYLE_GUIDE_23_OPERATOR_SPACING_YES, &default_config()).unwrap();
    assert_eq!(formatted, STYLE_GUIDE_23_OPERATOR_SPACING_YES);
}

#[test]
fn test_style_guide_operator_spacing_no() {
    let source = r#"contract T {
    function f() public pure {
        uint256 x=3;
        x = 100/10;
        x += 3+4;
    }
}
"#;
    let formatted = format_source(source, &default_config()).unwrap();
    assert_eq!(formatted, STYLE_GUIDE_23_OPERATOR_SPACING_YES);
}

// --- Idempotency: Emit statement (standalone, no style guide rule) ---

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

#[test]
fn test_preserve_bare_catch_without_parentheses() {
    let source = r#"contract T {
    function f(address to) public {
        try Foo(to).bar() returns (uint256 value) {
            consume(value);
        } catch Error(string memory reason) {
            revert(reason);
        } catch {
            revert("failed");
        }
    }
}
"#;
    let expected = r#"contract T {
    function f(address to) public {
        try Foo(to).bar() returns (uint256 value) {
            consume(value);
        } catch Error(string memory reason) {
            revert(reason);
        } catch {
            revert("failed");
        }
    }
}
"#;

    let formatted = format_source(source, &default_config()).unwrap();
    assert_eq!(formatted, expected);

    let reformatted = format_source(&formatted, &default_config()).unwrap();
    assert_eq!(reformatted, expected);
}

#[test]
fn test_preserve_subdenomination_without_duplication() {
    let source = r#"contract T {
    uint64 private constant GRACE_PERIOD = 90 days;
}
"#;
    let formatted = format_source(source, &default_config()).unwrap();
    assert_eq!(formatted, source);
}

#[test]
fn test_wrap_long_constant_initializer_after_equals() {
    let source = r#"contract T {
    bytes32 private constant ETH_NODE = 0x93cdeb708b7545dc668eb9280176169d1c33cfd8ed6f04690a0bcc88a93fc4ae;
}
"#;
    let expected = r#"contract T {
    bytes32 private constant ETH_NODE =
        0x93cdeb708b7545dc668eb9280176169d1c33cfd8ed6f04690a0bcc88a93fc4ae;
}
"#;
    let config = FormatConfig {
        line_length: 80,
        ..default_config()
    };
    let formatted = format_source(source, &config).unwrap();
    assert_eq!(formatted, expected);
}

#[test]
fn test_wrap_long_tuple_assignment_after_equals() {
    let source = r#"contract T {
    function f(bytes calldata data) public {
        (string memory label, address owner, uint16 ownerControlledFuses, address resolver) = abi.decode(data, (string, address, uint16, address));
    }
}
"#;
    let expected = r#"contract T {
    function f(bytes calldata data) public {
        (
            string memory label,
            address owner,
            uint16 ownerControlledFuses,
            address resolver
        ) =
            abi.decode(data, (string, address, uint16, address));
    }
}
"#;
    let config = FormatConfig {
        line_length: 80,
        ..default_config()
    };
    let formatted = format_source(source, &config).unwrap();
    assert_eq!(formatted, expected);
}

#[test]
fn test_preserve_comments_inside_multiline_condition() {
    let source = r#"contract T {
    function f(bytes32 parentNode, bytes32 subnode) public view {
        if (
            expired &&
                // protects a name that has been unwrapped
                (subnodeOwner == address(0)
                    || ens.owner(subnode) == address(0))
        ) {
            revert();
        }
    }
}
"#;
    let formatted = format_source(source, &default_config()).unwrap();
    assert!(formatted.contains("// protects a name that has been unwrapped"));
    assert!(
        formatted.contains("if (")
            && formatted.contains("(subnodeOwner == address(0)")
            && !formatted.contains(") {\n            // protects a name"),
        "comment should remain inside the condition, got:\n{formatted}"
    );
}

#[test]
fn test_wrap_modifier_arguments_over_multiple_lines() {
    let source = r#"contract T {
    function setRecord(bytes32 node, address owner, address resolver, uint64 ttl)
        public
        operationAllowed(node, CANNOT_TRANSFER | CANNOT_SET_RESOLVER | CANNOT_SET_TTL)
    {}
}
"#;
    let expected = r#"contract T {
    function setRecord(bytes32 node, address owner, address resolver, uint64 ttl)
        public
        operationAllowed(
            node,
            CANNOT_TRANSFER | CANNOT_SET_RESOLVER | CANNOT_SET_TTL
        )
    {}
}
"#;
    let config = FormatConfig {
        line_length: 80,
        ..default_config()
    };
    let formatted = format_source(source, &config).unwrap();
    assert_eq!(formatted, expected);
}

#[test]
fn test_format_logical_or_chain_without_staircase_indent() {
    let source = r#"contract T {
    function supportsInterface(bytes4 interfaceId) public view returns (bool) {
        return interfaceId == type(INameWrapper).interfaceId ||
            interfaceId == type(IERC721Receiver).interfaceId ||
                super.supportsInterface(interfaceId);
    }
}
"#;
    let expected = r#"contract T {
    function supportsInterface(bytes4 interfaceId) public view returns (bool) {
        return
            interfaceId == type(INameWrapper).interfaceId
                || interfaceId == type(IERC721Receiver).interfaceId
                || super.supportsInterface(interfaceId);
    }
}
"#;
    let formatted = format_source(source, &default_config()).unwrap();
    assert_eq!(formatted, expected);
}

#[test]
fn test_format_if_condition_breaks_after_open_paren() {
    let source = r#"contract T {
    function f(address registrant) public view {
        if (registrant != msg.sender &&
            !registrar.isApprovedForAll(registrant, msg.sender)) {
            revert();
        }
    }
}
"#;
    let expected = r#"contract T {
    function f(address registrant) public view {
        if (
            registrant != msg.sender
                && !registrar.isApprovedForAll(registrant, msg.sender)
        ) {
            revert();
        }
    }
}
"#;
    let config = FormatConfig {
        line_length: 80,
        ..default_config()
    };
    let formatted = format_source(source, &config).unwrap();
    assert_eq!(formatted, expected);
}

#[test]
fn test_format_parenthesized_logical_chain_like_forge() {
    let source = r#"contract T {
    function f(address owner, address addr, bytes32 node, uint32 fuses, uint64 expiry) public view returns (bool) {
        return (owner == addr ||
            isApprovedForAll(owner, addr) ||
                getApproved(uint256(node)) == addr) &&
            !_isETH2LDInGracePeriod(fuses, expiry);
    }
}
"#;
    let expected = r#"contract T {
    function f(
        address owner,
        address addr,
        bytes32 node,
        uint32 fuses,
        uint64 expiry
    )
        public
        view
        returns (bool)
    {
        return
            (owner == addr
                    || isApprovedForAll(owner, addr)
                    || getApproved(uint256(node)) == addr)
                && !_isETH2LDInGracePeriod(fuses, expiry);
    }
}
"#;
    let config = FormatConfig {
        line_length: 80,
        ..default_config()
    };
    let formatted = format_source(source, &config).unwrap();
    assert_eq!(formatted, expected);
}

#[test]
fn test_preserve_ignored_parameter_comments_in_signature() {
    let source = r#"contract T {
    function onERC721Received(
        address /*operator*/,
        address /*from*/,
        uint256 tokenId,
        bytes calldata data
    ) external returns (bytes4) {
        return this.onERC721Received.selector;
    }
}
"#;
    let expected = r#"contract T {
    function onERC721Received(
        address /*operator*/,
        address /*from*/,
        uint256 tokenId,
        bytes calldata data
    )
        external
        returns (bytes4)
    {
        return this.onERC721Received.selector;
    }
}
"#;

    let formatted = format_source(source, &default_config()).unwrap();
    assert_eq!(formatted, expected);
}

#[test]
fn test_preserve_argument_local_comment() {
    let source = r#"contract T {
    function f(Metadata memory md) public {
        ETH_REGISTRY.register(
            md.label,
            md.owner,
            md.subregistry,
            md.resolver,
            REGISTRATION_ROLE_BITMAP,
            0 // use reserved expiry
        );
    }
}
"#;
    let expected = r#"contract T {
    function f(Metadata memory md) public {
        ETH_REGISTRY.register(
            md.label,
            md.owner,
            md.subregistry,
            md.resolver,
            REGISTRATION_ROLE_BITMAP,
            0 // use reserved expiry
        );
    }
}
"#;

    let formatted = format_source(source, &default_config()).unwrap();
    assert_eq!(formatted, expected);
}

#[test]
fn test_statement_comment_after_call_is_not_attached_to_argument() {
    let source = r#"contract T {
    function f(uint256 a, uint256 b) public {
        foo(a, b); // note
    }
}
"#;
    let expected = r#"contract T {
    function f(uint256 a, uint256 b) public {
        foo(a, b); // note
    }
}
"#;

    let formatted = format_source(source, &default_config()).unwrap();
    assert_eq!(formatted, expected);
}

#[test]
fn test_preserve_call_argument_line_comment_after_comma() {
    let source = r#"contract T {
    function f(uint256 a, uint256 b) public {
        foo(
            a, // first
            b
        );
    }
}
"#;
    let expected = r#"contract T {
    function f(uint256 a, uint256 b) public {
        foo(
            a, // first
            b
        );
    }
}
"#;

    let formatted = format_source(source, &default_config()).unwrap();
    assert_eq!(formatted, expected);
}

#[test]
fn test_preserve_named_argument_line_comment_after_comma() {
    let source = r#"contract T {
    function f() public {
        foo({
            a: 1, // first
            b: 2
        });
    }
}
"#;
    let expected = r#"contract T {
    function f() public {
        foo({
            a: 1, // first
            b: 2
        });
    }
}
"#;

    let formatted = format_source(source, &default_config()).unwrap();
    assert_eq!(formatted, expected);
}

#[test]
fn test_preserve_last_call_argument_line_comment() {
    let source = r#"contract T {
    function f(uint256 a) public {
        foo(
            a // note
        );
    }
}
"#;
    let expected = r#"contract T {
    function f(uint256 a) public {
        foo(
            a // note
        );
    }
}
"#;

    let formatted = format_source(source, &default_config()).unwrap();
    assert_eq!(formatted, expected);
}

#[test]
fn test_preserve_last_named_argument_line_comment() {
    let source = r#"contract T {
    function f() public {
        foo({
            a: 1 // note
        });
    }
}
"#;
    let expected = r#"contract T {
    function f() public {
        foo({
            a: 1 // note
        });
    }
}
"#;

    let formatted = format_source(source, &default_config()).unwrap();
    assert_eq!(formatted, expected);
}

#[test]
fn test_preserve_parameter_line_comment_after_comma() {
    let source = r#"contract T {
    function f(
        uint256 a, // first
        uint256 b
    ) public pure {}
}
"#;
    let expected = r#"contract T {
    function f(
        uint256 a, // first
        uint256 b
    )
        public
        pure
    {}
}
"#;

    let formatted = format_source(source, &default_config()).unwrap();
    assert_eq!(formatted, expected);
}

#[test]
fn test_preserve_comment_after_argument_line_comment_on_separate_line() {
    let source = r#"contract T {
    function f(uint256 a, uint256 b) public {
        foo(
            a, // first
            /* second */
            b
        );
    }
}
"#;
    let expected = r#"contract T {
    function f(uint256 a, uint256 b) public {
        foo(
            a, // first
            /* second */
            b
        );
    }
}
"#;

    let formatted = format_source(source, &default_config()).unwrap();
    assert_eq!(formatted, expected);
}

#[test]
fn test_preserve_comment_after_parameter_line_comment_on_separate_line() {
    let source = r#"contract T {
    function f(
        uint256 a, // first
        /* second */
        uint256 b
    ) public pure {}
}
"#;
    let expected = r#"contract T {
    function f(
        uint256 a, // first
        /* second */
        uint256 b
    )
        public
        pure
    {}
}
"#;

    let formatted = format_source(source, &default_config()).unwrap();
    assert_eq!(formatted, expected);
}

#[test]
fn test_break_long_return_after_keyword() {
    let source = r#"contract T {
    function f(
        mapping(address => uint256) storage roles,
        uint256 resource,
        address account,
        uint256 roleBitmap
    ) internal view returns (bool) {
        return (roles[resource][account] | roles[0][account]) & roleBitmap == roleBitmap;
    }
}
"#;
    let expected = r#"contract T {
    function f(
        mapping(address => uint256) storage roles,
        uint256 resource,
        address account,
        uint256 roleBitmap
    )
        internal
        view
        returns (bool)
    {
        return
            (roles[resource][account] | roles[0][account]) & roleBitmap
            == roleBitmap;
    }
}
"#;
    let config = FormatConfig {
        line_length: 80,
        ..default_config()
    };

    let formatted = format_source(source, &config).unwrap();
    assert_eq!(formatted, expected);
}

#[test]
fn test_format_bitwise_or_chain_without_staircase_indent() {
    let source = r#"contract T {
    uint256 constant REGISTRATION_ROLE_BITMAP = 0 |
        RegistryRolesLib.ROLE_SET_SUBREGISTRY |
        RegistryRolesLib.ROLE_SET_SUBREGISTRY_ADMIN |
        RegistryRolesLib.ROLE_SET_RESOLVER |
        RegistryRolesLib.ROLE_SET_RESOLVER_ADMIN |
        RegistryRolesLib.ROLE_CAN_TRANSFER_ADMIN;
}
"#;
    let expected = r#"contract T {
    uint256 constant REGISTRATION_ROLE_BITMAP =
        0
        | RegistryRolesLib.ROLE_SET_SUBREGISTRY
        | RegistryRolesLib.ROLE_SET_SUBREGISTRY_ADMIN
        | RegistryRolesLib.ROLE_SET_RESOLVER
        | RegistryRolesLib.ROLE_SET_RESOLVER_ADMIN
        | RegistryRolesLib.ROLE_CAN_TRANSFER_ADMIN;
}
"#;

    let formatted = format_source(source, &default_config()).unwrap();
    assert_eq!(formatted, expected);
}

#[test]
fn test_multiline_enum_keeps_one_member_per_line() {
    let source = r#"contract T {
    enum State {
        START,
        IGNORED_KEY,
        IGNORED_KEY_ARG,
        VALUE,
        QUOTED_VALUE,
        UNQUOTED_VALUE,
        IGNORED_VALUE,
        IGNORED_QUOTED_VALUE,
        IGNORED_UNQUOTED_VALUE
    }
}
"#;
    let expected = r#"contract T {
    enum State {
        START,
        IGNORED_KEY,
        IGNORED_KEY_ARG,
        VALUE,
        QUOTED_VALUE,
        UNQUOTED_VALUE,
        IGNORED_VALUE,
        IGNORED_QUOTED_VALUE,
        IGNORED_UNQUOTED_VALUE
    }
}
"#;

    let formatted = format_source(source, &default_config()).unwrap();
    assert_eq!(formatted, expected);
}

#[test]
fn test_catch_with_typed_parameter_keeps_space() {
    let source = r#"contract T {
    function f(address to) public {
        try Foo(to).bar() returns (uint256 value) {
            consume(value);
        } catch (bytes memory reason) {
            consume(reason.length);
        }
    }
}
"#;

    let formatted = format_source(source, &default_config()).unwrap();
    assert!(formatted.contains("catch (bytes memory reason)"));
}

#[test]
fn test_format_long_for_header_by_clause() {
    let source = r#"contract T {
    function f(bytes memory data) public {
        for (RRUtils.RRIterator memory iter = RRUtils.iterateRRs(data, 0); !RRUtils.done(iter); RRUtils.next(iter)) {
            consume(iter);
        }
    }
}
"#;
    let expected = r#"contract T {
    function f(bytes memory data) public {
        for (
            RRUtils.RRIterator memory iter = RRUtils.iterateRRs(data, 0);
            !RRUtils.done(iter);
            RRUtils.next(iter)
        ) {
            consume(iter);
        }
    }
}
"#;
    let config = FormatConfig {
        line_length: 80,
        ..default_config()
    };
    let formatted = format_source(source, &config).unwrap();
    assert_eq!(formatted, expected);
}

#[test]
fn test_format_infinite_loop_uses_canonical_spacing() {
    let source = r#"contract T {
    function f() public {
        for (; ; ) {
            break;
        }
    }
}
"#;
    let expected = r#"contract T {
    function f() public {
        for (;;) {
            break;
        }
    }
}
"#;
    let formatted = format_source(source, &default_config()).unwrap();
    assert_eq!(formatted, expected);
}

#[test]
fn test_format_single_statement_if_body_over_multiple_lines() {
    let source = r#"contract T {
    function f(uint256 x) public pure {
        if (x == 0) revert();
    }
}
"#;
    let expected = r#"contract T {
    function f(uint256 x) public pure {
        if (x == 0)
            revert();
    }
}
"#;
    let formatted = format_source(source, &default_config()).unwrap();
    assert_eq!(formatted, expected);
}
