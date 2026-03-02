//! Thin wrapper around the Solar Solidity parser.
//!
//! Provides a simplified API for parsing Solidity source into an AST,
//! insulating the rest of solgrid from Solar API changes.

pub use solar_ast;
pub use solar_data_structures;
pub use solar_interface;
pub use solar_parse;

use solar_interface::{ColorChoice, Session};

/// Errors that can occur during parsing.
#[derive(Debug)]
pub enum ParseError {
    /// The parser encountered errors in the source.
    SyntaxErrors(String),
    /// An internal error occurred.
    Internal(String),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::SyntaxErrors(msg) => write!(f, "{msg}"),
            ParseError::Internal(msg) => write!(f, "internal parser error: {msg}"),
        }
    }
}

impl std::error::Error for ParseError {}

/// Parse a Solidity source string and invoke a callback with the AST.
///
/// The Solar parser requires a `Session` context for memory management.
/// This function creates a session, parses the source, and invokes the
/// callback with the parsed AST items.
///
/// Returns `Ok(T)` with the callback's return value, or `Err` if parsing fails.
pub fn with_parsed_ast<T, F>(source: &str, filename: &str, callback: F) -> Result<T, ParseError>
where
    T: Send,
    F: FnOnce(&solar_ast::SourceUnit<'_>) -> T + Send,
{
    let sess = Session::builder()
        .with_buffer_emitter(ColorChoice::Never)
        .build();
    sess.enter(|| -> Result<T, ParseError> {
        let arena = solar_ast::Arena::new();
        let filename_obj = solar_interface::source_map::FileName::Custom(filename.to_string());
        let parser =
            solar_parse::Parser::from_source_code(&sess, &arena, filename_obj, source.to_string());
        match parser {
            Ok(mut parser) => match parser.parse_file() {
                Ok(source_unit) => Ok(callback(&source_unit)),
                Err(e) => {
                    e.emit();
                    Err(ParseError::SyntaxErrors(format!(
                        "syntax error(s) in {filename}"
                    )))
                }
            },
            Err(_) => Err(ParseError::Internal(format!(
                "failed to create parser for {filename}"
            ))),
        }
    })
}

/// Parse a Solidity source string and invoke a callback with the AST.
///
/// Same as `with_parsed_ast` but uses `enter_sequential` to avoid thread pool
/// overhead. Good for single-file operations and testing.
pub fn with_parsed_ast_sequential<T, F>(
    source: &str,
    filename: &str,
    callback: F,
) -> Result<T, ParseError>
where
    F: FnOnce(&solar_ast::SourceUnit<'_>) -> T,
{
    let sess = Session::builder()
        .with_buffer_emitter(ColorChoice::Never)
        .build();
    sess.enter_sequential(|| -> Result<T, ParseError> {
        let arena = solar_ast::Arena::new();
        let filename_obj = solar_interface::source_map::FileName::Custom(filename.to_string());
        let parser =
            solar_parse::Parser::from_source_code(&sess, &arena, filename_obj, source.to_string());
        match parser {
            Ok(mut parser) => match parser.parse_file() {
                Ok(source_unit) => Ok(callback(&source_unit)),
                Err(e) => {
                    e.emit();
                    Err(ParseError::SyntaxErrors(format!(
                        "syntax error(s) in {filename}"
                    )))
                }
            },
            Err(_) => Err(ParseError::Internal(format!(
                "failed to create parser for {filename}"
            ))),
        }
    })
}

/// Check if a Solidity source string is syntactically valid.
pub fn check_syntax(source: &str, filename: &str) -> Result<(), ParseError> {
    with_parsed_ast(source, filename, |_| ())
}
