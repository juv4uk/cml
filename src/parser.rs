use crate::ast::{Expr, NumericBufferLiteral};

// my-lisp's language-contract.my bumped 1.0 -> 2.0 (commit d287a16,
// "complete quote migration"): `'` is no longer reader shorthand for
// `quote` -- it's now a plain identifier character (enabling symbols
// like Ukrainian об'єкт/зв'язок/п'ять), matching real my-lisp's own
// parser test `apostrophe_is_no_longer_quote_sugar_but_part_of_symbol`.
// `tokenize` therefore folds `'` into whatever token it's touching
// instead of splitting on it, and `parse_expr` has no `"'"` case --
// this file used to auto-expand a leading `'` into `(quote ...)`, which
// would have silently misparsed any symbol using an apostrophe under
// the current contract.

#[derive(Debug)]
pub enum ParseError {
    UnexpectedEOF,
    UnexpectedToken(String),
}

fn tokenize(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_string = false;

    for c in input.chars() {
        if in_string {
            current.push(c);
            if c == '"' {
                tokens.push(current.clone());
                current.clear();
                in_string = false;
            }
            continue;
        }

        match c {
            '(' | ')' => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
                tokens.push(c.to_string());
            }
            ' ' | '\n' | '\t' | '\r' => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
            }
            '"' => {
                in_string = true;
                current.push(c);
            }
            _ => {
                current.push(c);
            }
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

pub fn parse(input: &str) -> Result<Vec<Expr>, ParseError> {
    let tokens = tokenize(input);
    let mut it = tokens.into_iter().peekable();
    let mut exprs = Vec::new();
    while it.peek().is_some() {
        exprs.push(parse_expr(&mut it)?);
    }
    Ok(exprs)
}

fn parse_expr(tokens: &mut std::iter::Peekable<std::vec::IntoIter<String>>) -> Result<Expr, ParseError> {
    let token = tokens.next().ok_or(ParseError::UnexpectedEOF)?;

    match token.as_str() {
        "(" => parse_list(tokens),
        ")" => Err(ParseError::UnexpectedToken(")".to_string())),
        "#i32" => parse_numeric_buffer(tokens, false),
        "#f32" => parse_numeric_buffer(tokens, true),
        _ => {
            if token.starts_with('"') && token.ends_with('"') {
                Ok(Expr::String(token[1..token.len() - 1].to_string()))
            } else if let Ok(n) = token.parse::<i64>() {
                Ok(Expr::Integer(n))
            } else {
                Ok(Expr::Symbol(token))
            }
        }
    }
}

fn parse_numeric_buffer(
    tokens: &mut std::iter::Peekable<std::vec::IntoIter<String>>,
    f32_elements: bool,
) -> Result<Expr, ParseError> {
    match tokens.next().as_deref() {
        Some("(") => {}
        Some(token) => {
            return Err(ParseError::UnexpectedToken(format!(
                "expected '(' after numeric buffer tag, found {token}"
            )))
        }
        None => return Err(ParseError::UnexpectedEOF),
    }

    if f32_elements {
        let mut values = Vec::new();
        loop {
            let token = tokens.next().ok_or(ParseError::UnexpectedEOF)?;
            if token == ")" {
                return Ok(Expr::NumericBuffer(NumericBufferLiteral::F32(values)));
            }
            if token == "(" {
                return Err(ParseError::UnexpectedToken(
                    "numeric buffer elements must be scalar".to_string(),
                ));
            }
            let value = token.parse::<f32>().map_err(|_| {
                ParseError::UnexpectedToken(format!("invalid f32 buffer element: {token}"))
            })?;
            if !value.is_finite() {
                return Err(ParseError::UnexpectedToken(format!(
                    "non-finite f32 buffer element: {token}"
                )));
            }
            values.push(value.to_bits());
        }
    }

    let mut values = Vec::new();
    loop {
        let token = tokens.next().ok_or(ParseError::UnexpectedEOF)?;
        if token == ")" {
            return Ok(Expr::NumericBuffer(NumericBufferLiteral::I32(values)));
        }
        let value = token.parse::<i32>().map_err(|_| {
            ParseError::UnexpectedToken(format!("invalid i32 buffer element: {token}"))
        })?;
        values.push(value);
    }
}

fn parse_list(tokens: &mut std::iter::Peekable<std::vec::IntoIter<String>>) -> Result<Expr, ParseError> {
    let mut list = Vec::new();
    while let Some(peeked) = tokens.peek() {
        if peeked == ")" {
            tokens.next();
            return Ok(Expr::List(list));
        } else if peeked == "." {
            tokens.next();
            let dotted = parse_expr(tokens)?;
            let closing = tokens.next().ok_or(ParseError::UnexpectedEOF)?;
            if closing != ")" {
                return Err(ParseError::UnexpectedToken(format!("Expected ')', found {}", closing)));
            }
            return Ok(Expr::DottedList(list, Box::new(dotted)));
        }
        list.push(parse_expr(tokens)?);
    }
    Err(ParseError::UnexpectedEOF)
}
