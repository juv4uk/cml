//! Contract 3.0 Parse named-kind surface for the CML reader.

use cml::parser::{self, ParseError};

#[test]
fn unexpected_eof_uses_parse_prefix() {
    let err = parser::parse("(").unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.starts_with("Parse:"),
        "expected Parse: prefix, got {msg:?}"
    );
    assert!(matches!(err, ParseError::UnexpectedEOF { .. }));
}

#[test]
fn unexpected_token_uses_parse_prefix() {
    let err = parser::parse(")").unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.starts_with("Parse:"),
        "expected Parse: prefix, got {msg:?}"
    );
    assert!(matches!(err, ParseError::UnexpectedToken { .. }));
}

#[test]
fn display_is_stable_for_cli_consumers() {
    assert_eq!(
        ParseError::UnexpectedEOF { line: 1, column: 2 }.to_string(),
        "Parse: unexpected end of input"
    );
    assert_eq!(
        ParseError::UnexpectedToken {
            token: ")".to_string(),
            line: 1,
            column: 1,
        }
        .to_string(),
        "Parse: unexpected token `)`"
    );
}

#[test]
fn parse_error_preserves_line_and_column() {
    let err = parser::parse("(quote ok)\n\n)").unwrap_err();
    assert_eq!(err.line(), Some(3));
    assert_eq!(err.column(), Some(1));
}
