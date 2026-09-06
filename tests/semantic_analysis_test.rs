use cml::lower::{self, LowerErrorKind};
use cml::parser;
use cml::semantic::{self, SemanticErrorKind};

#[test]
fn accepts_the_current_single_body_lexical_subset() {
    let source =
        "(def length (lambda (values) (cond ((atom values) 0) (t (+ 1 (length (cdr values)))))))";
    let expressions = parser::parse(source).unwrap();
    semantic::analyze_program(&expressions).unwrap();
    lower::lower_program(&expressions).unwrap();
}

#[test]
fn rejects_duplicate_lambda_parameters_before_lowering() {
    let expressions = parser::parse("(lambda (x x) x)").unwrap();
    let error = semantic::analyze_program(&expressions).unwrap_err();
    assert_eq!(error.kind, SemanticErrorKind::DuplicateParameter);

    let lower_error = lower::lower_program(&expressions).unwrap_err();
    assert_eq!(lower_error.kind, LowerErrorKind::Semantic);
}

#[test]
fn rejects_parameter_collisions_created_by_cml_symbol_normalization() {
    let expressions = parser::parse("(lambda (x X) x)").unwrap();
    let error = semantic::analyze_program(&expressions).unwrap_err();
    assert_eq!(error.kind, SemanticErrorKind::DuplicateParameter);
}

#[test]
fn rejects_multi_body_lambda_instead_of_silently_dropping_expressions() {
    let expressions = parser::parse("(lambda (x) (def y x) y)").unwrap();
    let error = semantic::analyze_program(&expressions).unwrap_err();
    assert_eq!(error.kind, SemanticErrorKind::UnsupportedSequentialBody);

    let lower_error = lower::lower_program(&expressions).unwrap_err();
    assert_eq!(lower_error.kind, LowerErrorKind::Semantic);
}

#[test]
fn quoted_lambda_shaped_data_is_not_treated_as_executable_code() {
    let expressions = parser::parse("(quote (lambda (x x) x))").unwrap();
    semantic::analyze_program(&expressions).unwrap();
}

#[test]
fn rejects_quoted_string_literal() {
    let expressions = parser::parse("(quote \"hello\")").unwrap();
    let error = semantic::analyze_program(&expressions).unwrap_err();
    assert_eq!(error.kind, SemanticErrorKind::UnquotedStringLiteral);
}

#[test]
fn rejects_quoted_string_in_list() {
    let expressions = parser::parse("(quote (a \"hello\" b))").unwrap();
    let error = semantic::analyze_program(&expressions).unwrap_err();
    assert_eq!(error.kind, SemanticErrorKind::UnquotedStringLiteral);
}

#[test]
fn rejects_f32_numeric_buffer() {
    let expressions = parser::parse("#f32(1.0 2.0 3.0)").unwrap();
    let error = semantic::analyze_program(&expressions).unwrap_err();
    assert_eq!(error.kind, SemanticErrorKind::UnsupportedF32Buffer);
}
