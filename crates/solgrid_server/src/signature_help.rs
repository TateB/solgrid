//! Signature help support for Solidity call expressions.

use crate::resolve::ImportResolver;
use crate::semantic;
use std::path::Path;
use tower_lsp_server::ls_types;

pub fn signature_help_at_position(
    source: &str,
    position: &ls_types::Position,
    get_source: &dyn Fn(&Path) -> Option<String>,
    resolver: &ImportResolver,
    current_file: Option<&Path>,
) -> Option<ls_types::SignatureHelp> {
    let offset = crate::convert::position_to_offset(source, *position);
    let help =
        semantic::signature_help_at_offset(source, offset, current_file, get_source, resolver)?;

    Some(ls_types::SignatureHelp {
        signatures: help
            .signatures
            .into_iter()
            .map(|signature| ls_types::SignatureInformation {
                label: signature.label,
                documentation: None,
                parameters: Some(
                    signature
                        .parameter_ranges
                        .into_iter()
                        .map(|(start, end)| ls_types::ParameterInformation {
                            label: ls_types::ParameterLabel::LabelOffsets([start, end]),
                            documentation: None,
                        })
                        .collect(),
                ),
                active_parameter: Some(help.active_parameter),
            })
            .collect(),
        active_signature: Some(0),
        active_parameter: Some(help.active_parameter),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noop_source(_path: &Path) -> Option<String> {
        None
    }

    fn noop_resolver() -> ImportResolver {
        ImportResolver::new(None)
    }

    #[test]
    fn test_signature_help_for_builtin_call() {
        let source = "contract T { function f() public { require(true, ); } }";
        let offset = source.find("require(true, ").unwrap() + "require(true, ".len();
        let position = crate::convert::offset_to_position(source, offset);
        let help =
            signature_help_at_position(source, &position, &noop_source, &noop_resolver(), None)
                .unwrap();

        assert!(!help.signatures.is_empty());
        assert!(help.signatures[0].label.contains("require("));
        assert_eq!(help.active_parameter, Some(1));
    }

    #[test]
    fn test_signature_help_for_user_defined_function() {
        let source = r#"contract T {
    function transfer(address recipient, uint256 amount) public {}
    function callTransfer(address recipient) public {
        transfer(recipient, );
    }
}"#;
        let offset = source.find("transfer(recipient, ").unwrap() + "transfer(recipient, ".len();
        let position = crate::convert::offset_to_position(source, offset);
        let help =
            signature_help_at_position(source, &position, &noop_source, &noop_resolver(), None)
                .unwrap();

        assert!(help.signatures[0]
            .label
            .contains("function transfer(address recipient, uint256 amount) public"));
        assert_eq!(help.active_parameter, Some(1));
    }

    #[test]
    fn test_signature_help_for_constructor_call() {
        let source = r#"contract SomethingA {
    constructor(uint256 count, address owner) {}
}

contract SomethingB {
    function build(address owner) public {
        new SomethingA(1, );
    }
}"#;
        let offset = source.find("new SomethingA(1, ").unwrap() + "new SomethingA(1, ".len();
        let position = crate::convert::offset_to_position(source, offset);
        let help =
            signature_help_at_position(source, &position, &noop_source, &noop_resolver(), None)
                .unwrap();

        assert!(help.signatures[0]
            .label
            .contains("constructor(uint256 count, address owner)"));
        assert_eq!(help.active_parameter, Some(1));
    }
}
