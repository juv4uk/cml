use crate::ast::{Expr, NumericBufferLiteral};

// my-lisp language-contract 4.0 makes apostrophe context-sensitive reader
// syntax. At expression start, `'form` is exactly `(quote form)`. Inside an
// identifier, apostrophe remains an ordinary character, so Ukrainian symbols
// such as об'єкт, зв'язок and п'ять remain single identifiers.
//
// CML deliberately implements this reader invariant without claiming full
// contract-4.0 conformance: compatibility.my still records older unsupported
// contract requirements separately.

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
            '\'' if current.is_empty() => {
                // Expression-initial apostrophe is reader syntax. If an
                // identifier is already being accumulated, the same character
                // falls through to the default arm and remains part of it.
                tokens.push("'".to_string());
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

fn parse_expr(
    tokens: &mut std::iter::Peekable<std::vec::IntoIter<String>>,
) -> Result<Expr, ParseError> {
    let token = tokens.next().ok_or(ParseError::UnexpectedEOF)?;

    match token.as_str() {
        "(" => parse_list(tokens),
        ")" => Err(ParseError::UnexpectedToken(")".to_string())),
        "'" => {
            let quoted = parse_expr(tokens)?;
            Ok(Expr::List(vec![Expr::Symbol("quote".to_string()), quoted]))
        }
        "#i32" => parse_numeric_buffer(tokens, false),
        "#f32" => parse_numeric_buffer(tokens, true),
        _ => {
            if token.starts_with('"') && token.ends_with('"') {
                Ok(Expr::String(token[1..token.len() - 1].to_string()))
            } else if let Ok(n) = token.parse::<i64>() {
                Ok(Expr::Integer(n))
            } else if let Some(rat) = parse_rational_literal(&token) {
                Ok(Expr::Rational(rat.0, rat.1))
            } else {
                Ok(Expr::Symbol(token))
            }
        }
    }
}

/// Parses an exact rational numeral token of the form `n/d` (e.g. `1/2`,
/// `-3/4`). Returns `(numerator, denominator)` reduced by gcd with a strictly
/// positive denominator, or `None` if the token is not a rational numeral.
/// A bare `/` (division operator) and non-numeric tokens are rejected.
fn parse_rational_literal(token: &str) -> Option<(i64, u64)> {
    let (num_str, den_str) = token.split_once('/')?;
    if num_str.is_empty() || den_str.is_empty() {
        return None;
    }
    let num = num_str.parse::<i64>().ok()?;
    let den = den_str.parse::<u64>().ok()?;
    if den == 0 {
        return None;
    }
    let g = gcd(num.unsigned_abs(), den);
    Some((num / g as i64, den / g))
}

fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a
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
            )));
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

fn parse_list(
    tokens: &mut std::iter::Peekable<std::vec::IntoIter<String>>,
) -> Result<Expr, ParseError> {
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
                return Err(ParseError::UnexpectedToken(format!(
                    "Expected ')', found {}",
                    closing
                )));
            }
            return Ok(Expr::DottedList(list, Box::new(dotted)));
        }
        list.push(parse_expr(tokens)?);
    }
    Err(ParseError::UnexpectedEOF)
}
