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

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Contract 3.0 named kind prefix: Parse
        match self {
            ParseError::UnexpectedEOF => write!(f, "Parse: unexpected end of input"),
            ParseError::UnexpectedToken(tok) => write!(f, "Parse: unexpected token `{tok}`"),
        }
    }
}

impl std::error::Error for ParseError {}

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
                tokens.push(String::from_utf8(vec![39u8]).unwrap());
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
            } else if let Some(dec) = parse_decimal_literal(&token) {
                if dec.1 == 1 {
                    Ok(Expr::Integer(dec.0))
                } else {
                    Ok(Expr::Rational(dec.0, dec.1))
                }
            } else {
                Ok(Expr::Symbol(token))
            }
        }
    }
}

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

fn parse_decimal_literal(token: &str) -> Option<(i64, u64)> {
    const MAX_EXP_MAG: i32 = 10_000;

    let lower = token.to_ascii_lowercase();
    let (base_str, exp_str) = match lower.split_once('e') {
        Some((b, e)) => (b, Some(e)),
        None => (lower.as_str(), None),
    };

    let dot_count = base_str.matches('.').count();
    let comma_count = base_str.matches(',').count();
    if dot_count + comma_count > 1 {
        return None;
    }
    if dot_count == 1 && comma_count == 1 {
        return None;
    }

    let scientific_exp: i32 = if let Some(e) = exp_str {
        let digits = e.strip_prefix(['+', '-']).unwrap_or(e);
        if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        e.parse().ok()?
    } else {
        0
    };

    let (mantissa_str, decimal_places) = if let Some((int_part, frac_part)) =
        base_str.split_once('.').or_else(|| base_str.split_once(','))
    {
        if frac_part.is_empty() {
            return None;
        }
        if !frac_part.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        let mut m = String::with_capacity(int_part.len() + frac_part.len());
        m.push_str(int_part);
        m.push_str(frac_part);
        (m, frac_part.len() as i32)
    } else {
        (base_str.to_string(), 0)
    };

    if mantissa_str.is_empty() || mantissa_str == "-" || mantissa_str == "+" {
        return None;
    }
    let mant_digits = mantissa_str.strip_prefix(['+', '-']).unwrap_or(&mantissa_str);
    if mant_digits.is_empty() || !mant_digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }

    let mantissa: i128 = mantissa_str.parse().ok()?;
    let total_exp = scientific_exp.checked_sub(decimal_places)?;
    if !(-MAX_EXP_MAG..=MAX_EXP_MAG).contains(&total_exp) {
        return None;
    }

    let (num, den): (i128, u128) = if total_exp >= 0 {
        let factor = 10i128.checked_pow(total_exp as u32)?;
        (mantissa.checked_mul(factor)?, 1)
    } else {
        let factor = 10u128.checked_pow((-total_exp) as u32)?;
        (mantissa, factor)
    };

    if den == 0 {
        return None;
    }
    let g = gcd_u128(num.unsigned_abs(), den);
    let num = num / g as i128;
    let den = den / g;
    let num_i64 = i64::try_from(num).ok()?;
    let den_u64 = u64::try_from(den).ok()?;
    Some((num_i64, den_u64))
}

fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a
}

fn gcd_u128(mut a: u128, mut b: u128) -> u128 {
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
