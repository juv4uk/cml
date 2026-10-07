use crate::ast::{Expr, NumericBufferLiteral};

// my-lisp language-contract 4.0 makes apostrophe context-sensitive reader
// syntax. At expression start, `'form` is exactly `(quote form)`. Inside an
// identifier, apostrophe remains an ordinary character, so Ukrainian symbols
// such as об'єкт, зв'язок and п'ять remain single identifiers.
//
// CML deliberately implements this reader invariant without claiming full
// contract-4.0 conformance: compatibility.lisp still records older unsupported
// contract requirements separately.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SourceLocation {
    line: usize,
    column: usize,
}

#[derive(Debug)]
pub enum ParseError {
    UnexpectedEOF {
        line: usize,
        column: usize,
    },
    UnexpectedToken {
        token: String,
        line: usize,
        column: usize,
    },
}

impl ParseError {
    fn unexpected_eof(location: SourceLocation) -> Self {
        Self::UnexpectedEOF {
            line: location.line,
            column: location.column,
        }
    }

    fn unexpected_token(token: impl Into<String>, location: SourceLocation) -> Self {
        Self::UnexpectedToken {
            token: token.into(),
            line: location.line,
            column: location.column,
        }
    }

    pub fn line(&self) -> Option<usize> {
        Some(match self {
            Self::UnexpectedEOF { line, .. } | Self::UnexpectedToken { line, .. } => *line,
        })
    }

    pub fn column(&self) -> Option<usize> {
        Some(match self {
            Self::UnexpectedEOF { column, .. } | Self::UnexpectedToken { column, .. } => *column,
        })
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Contract 3.0 named kind prefix: Parse. Location is carried as
        // structured provenance and deliberately does not change this
        // historical human-readable presentation.
        match self {
            ParseError::UnexpectedEOF { .. } => write!(f, "Parse: unexpected end of input"),
            ParseError::UnexpectedToken { token, .. } => {
                write!(f, "Parse: unexpected token `{token}`")
            }
        }
    }
}

impl std::error::Error for ParseError {}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Token {
    text: String,
    location: SourceLocation,
}

struct Tokens {
    inner: std::iter::Peekable<std::vec::IntoIter<Token>>,
    eof: SourceLocation,
}

impl Tokens {
    fn next(&mut self) -> Option<Token> {
        self.inner.next()
    }

    fn peek(&mut self) -> Option<&Token> {
        self.inner.peek()
    }

    fn eof_error(&self) -> ParseError {
        ParseError::unexpected_eof(self.eof)
    }
}

fn push_current(tokens: &mut Vec<Token>, current: &mut String, start: &mut Option<SourceLocation>) {
    if current.is_empty() {
        return;
    }
    let location = start
        .take()
        .expect("a non-empty token must have a source location");
    tokens.push(Token {
        text: std::mem::take(current),
        location,
    });
}

fn tokenize(input: &str) -> Tokens {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut current_start = None;
    let mut in_string = false;
    let mut in_comment = false;
    let mut line = 1usize;
    let mut column = 1usize;

    for c in input.chars() {
        let location = SourceLocation { line, column };

        if in_comment {
            if c == '\n' {
                in_comment = false;
            }
        } else if in_string {
            current.push(c);
            if c == '"' {
                push_current(&mut tokens, &mut current, &mut current_start);
                in_string = false;
            }
        } else {
            match c {
                ';' => {
                    push_current(&mut tokens, &mut current, &mut current_start);
                    in_comment = true;
                }
                '(' | ')' => {
                    push_current(&mut tokens, &mut current, &mut current_start);
                    tokens.push(Token {
                        text: c.to_string(),
                        location,
                    });
                }
                ' ' | '\n' | '\t' | '\r' => {
                    push_current(&mut tokens, &mut current, &mut current_start);
                }
                '"' => {
                    current_start = Some(location);
                    in_string = true;
                    current.push(c);
                }
                '\'' if current.is_empty() => {
                    // Expression-initial apostrophe is reader syntax. If an
                    // identifier is already being accumulated, the same
                    // character falls through to the default arm.
                    tokens.push(Token {
                        text: String::from_utf8(vec![39u8]).unwrap(),
                        location,
                    });
                }
                _ => {
                    if current_start.is_none() {
                        current_start = Some(location);
                    }
                    current.push(c);
                }
            }
        }

        if c == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }

    push_current(&mut tokens, &mut current, &mut current_start);
    Tokens {
        inner: tokens.into_iter().peekable(),
        eof: SourceLocation { line, column },
    }
}

pub fn parse(input: &str) -> Result<Vec<Expr>, ParseError> {
    let mut tokens = tokenize(input);
    let mut exprs = Vec::new();
    while tokens.peek().is_some() {
        exprs.push(parse_expr(&mut tokens)?);
    }
    Ok(exprs)
}

fn canonical_location(span: sens::Span) -> SourceLocation {
    SourceLocation {
        line: 1,
        column: span.start + 1,
    }
}

fn import_canonical_expr(expr: &sens::Expr) -> Result<Expr, ParseError> {
    let location = canonical_location(expr.span);
    match &expr.kind {
        sens::ExprKind::DomainIdentity(identity) => Ok(Expr::DomainIdentity(*identity)),
        sens::ExprKind::List(items) => items
            .iter()
            .map(import_canonical_expr)
            .collect::<Result<Vec<_>, _>>()
            .map(Expr::List),
        sens::ExprKind::Pair(head, tail) => {
            let head = import_canonical_expr(head)?;
            let tail = import_canonical_expr(tail)?;
            Ok(Expr::DottedList(vec![head], Box::new(tail)))
        }
        other => Err(ParseError::unexpected_token(
            format!("unsupported canonical SENS node: {other:?}"),
            location,
        )),
    }
}

/// Parse current exact-domain visible-binary SENS without routing through the
/// historical human/SID8 lexer. D2 structure is owned by the upstream reader;
/// payloads arrive here carrying their exact domain identity.
pub fn parse_canonical_binary(input: &str) -> Result<Vec<Expr>, ParseError> {
    sens::parse_canonical_binary(input)
        .map_err(|error| {
            ParseError::unexpected_token(
                format!("canonical SENS parse failed: {error}"),
                SourceLocation { line: 1, column: 1 },
            )
        })?
        .iter()
        .map(import_canonical_expr)
        .collect()
}

fn parse_expr(tokens: &mut Tokens) -> Result<Expr, ParseError> {
    let token = match tokens.next() {
        Some(token) => token,
        None => return Err(tokens.eof_error()),
    };

    match token.text.as_str() {
        "(" => parse_list(tokens),
        ")" => Err(ParseError::unexpected_token(")", token.location)),
        "'" => {
            let quoted = parse_expr(tokens)?;
            Ok(Expr::List(vec![Expr::Symbol("quote".to_string()), quoted]))
        }
        "#i32" => parse_numeric_buffer(tokens, false),
        "#f32" => parse_numeric_buffer(tokens, true),
        _ => {
            if token.text.starts_with('"') && token.text.ends_with('"') {
                Ok(Expr::String(
                    token.text[1..token.text.len() - 1].to_string(),
                ))
            } else if token.text.len() == 8
                && token.text.bytes().all(|byte| matches!(byte, b'0' | b'1'))
            {
                let upstream = sens::parse(&token.text)
                    .map_err(|_| ParseError::unexpected_token("invalid SID8", token.location))?;
                match upstream.as_slice() {
                    [expr] => match expr.kind {
                        sens::ExprKind::Sid(sid) => Ok(Expr::Sid(sid)),
                        _ => Err(ParseError::unexpected_token(
                            "exact eight-bit binary token must be SID8",
                            token.location,
                        )),
                    },
                    _ => Err(ParseError::unexpected_token(
                        "exact eight-bit binary token must parse as one SID8",
                        token.location,
                    )),
                }
            } else if let Ok(n) = token.text.parse::<i64>() {
                Ok(Expr::Integer(n))
            } else if let Some(rat) = parse_rational_literal(&token.text) {
                Ok(Expr::Rational(rat.0, rat.1))
            } else if let Some(dec) = parse_decimal_literal(&token.text) {
                if dec.1 == 1 {
                    Ok(Expr::Integer(dec.0))
                } else {
                    Ok(Expr::Rational(dec.0, dec.1))
                }
            } else {
                Ok(Expr::Symbol(token.text))
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

    let (mantissa_str, decimal_places) = if let Some((int_part, frac_part)) = base_str
        .split_once('.')
        .or_else(|| base_str.split_once(','))
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
    let mant_digits = mantissa_str
        .strip_prefix(['+', '-'])
        .unwrap_or(&mantissa_str);
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

fn parse_numeric_buffer(tokens: &mut Tokens, f32_elements: bool) -> Result<Expr, ParseError> {
    match tokens.next() {
        Some(token) if token.text == "(" => {}
        Some(token) => {
            return Err(ParseError::unexpected_token(
                format!(
                    "expected '(' after numeric buffer tag, found {}",
                    token.text
                ),
                token.location,
            ));
        }
        None => return Err(tokens.eof_error()),
    }

    if f32_elements {
        let mut values = Vec::new();
        loop {
            let token = match tokens.next() {
                Some(token) => token,
                None => return Err(tokens.eof_error()),
            };
            if token.text == ")" {
                return Ok(Expr::NumericBuffer(NumericBufferLiteral::F32(values)));
            }
            if token.text == "(" {
                return Err(ParseError::unexpected_token(
                    "numeric buffer elements must be scalar",
                    token.location,
                ));
            }
            let value = token.text.parse::<f32>().map_err(|_| {
                ParseError::unexpected_token(
                    format!("invalid f32 buffer element: {}", token.text),
                    token.location,
                )
            })?;
            if !value.is_finite() {
                return Err(ParseError::unexpected_token(
                    format!("non-finite f32 buffer element: {}", token.text),
                    token.location,
                ));
            }
            values.push(value.to_bits());
        }
    }

    let mut values = Vec::new();
    loop {
        let token = match tokens.next() {
            Some(token) => token,
            None => return Err(tokens.eof_error()),
        };
        if token.text == ")" {
            return Ok(Expr::NumericBuffer(NumericBufferLiteral::I32(values)));
        }
        let value = token.text.parse::<i32>().map_err(|_| {
            ParseError::unexpected_token(
                format!("invalid i32 buffer element: {}", token.text),
                token.location,
            )
        })?;
        values.push(value);
    }
}

fn parse_list(tokens: &mut Tokens) -> Result<Expr, ParseError> {
    let mut list = Vec::new();
    while let Some(peeked) = tokens.peek() {
        if peeked.text == ")" {
            tokens.next();
            return Ok(Expr::List(list));
        } else if peeked.text == "." {
            tokens.next();
            let dotted = parse_expr(tokens)?;
            let closing = match tokens.next() {
                Some(token) => token,
                None => return Err(tokens.eof_error()),
            };
            if closing.text != ")" {
                return Err(ParseError::unexpected_token(
                    format!("Expected ')', found {}", closing.text),
                    closing.location,
                ));
            }
            return Ok(Expr::DottedList(list, Box::new(dotted)));
        }
        list.push(parse_expr(tokens)?);
    }
    Err(tokens.eof_error())
}

#[cfg(test)]
mod comment_tests {
    use super::*;

    // Real cml bug found while binary-searching why the whole of my-lisp's
    // lib/meta-eval.lisp failed to compile ("special forms are not callable
    // values" on a bare Symbol("lambda")): the tokenizer had no `;`
    // line-comment handling at all, so ordinary prose in doc comments (e.g.
    // "... variadic/dotted lambda parameter binding ...") was tokenized as
    // source code. Every ordinary word became its own top-level Symbol
    // expression, and any comment word that happened to match a special-form
    // name (lambda/cond/def/...) was then rejected by lower_symbol as an
    // unbound special-form value. The file was never wrong.
    #[test]
    fn trailing_line_comment_after_an_expression_is_ignored() {
        let exprs = parse("(quote a) ; this is a comment\n(quote b)").unwrap();
        assert_eq!(
            exprs,
            vec![
                Expr::List(vec![Expr::Symbol("quote".into()), Expr::Symbol("a".into())]),
                Expr::List(vec![Expr::Symbol("quote".into()), Expr::Symbol("b".into())]),
            ]
        );
    }

    #[test]
    fn comment_containing_a_special_form_name_is_not_parsed_as_code() {
        // Before the fix this produced a bare `Symbol("lambda")` top-level
        // expression from the comment text alone.
        let exprs = parse("; variadic/dotted lambda parameter binding\n(quote ok)").unwrap();
        assert_eq!(
            exprs,
            vec![Expr::List(vec![
                Expr::Symbol("quote".into()),
                Expr::Symbol("ok".into())
            ])]
        );
    }

    #[test]
    fn semicolon_inside_a_string_literal_is_not_a_comment() {
        let exprs = parse("(quote \"a;b\")").unwrap();
        assert_eq!(
            exprs,
            vec![Expr::List(vec![
                Expr::Symbol("quote".into()),
                Expr::String("a;b".into())
            ])]
        );
    }

    #[test]
    fn a_file_consisting_only_of_comments_parses_as_empty() {
        let exprs = parse("; just a comment\n; another one").unwrap();
        assert!(exprs.is_empty());
    }
}

#[cfg(test)]
mod exact_domain_identity_tests {
    use super::*;

    #[test]
    fn canonical_d3_quote_empty_preserves_domain_identity() {
        let exprs = parse_canonical_binary("10 001 00 000 01")
            .expect("current exact-domain source must parse");
        assert_eq!(exprs.len(), 1);
        let Expr::List(items) = &exprs[0] else {
            panic!("expected one canonical list");
        };
        assert_eq!(items.len(), 2);
        let Expr::DomainIdentity(identity) = &items[0] else {
            panic!("D3 quote head must remain DomainIdentity");
        };
        assert_eq!((identity.width(), identity.packed_bits()), (3, 0b001));
        assert!(matches!(&items[1], Expr::List(empty) if empty.is_empty()));
    }

    #[test]
    fn equal_payloads_across_widths_do_not_collapse() {
        let exprs = parse_canonical_binary("001 0001").expect("D3/D4 exact identities must parse");
        let [Expr::DomainIdentity(d3), Expr::DomainIdentity(d4)] = exprs.as_slice() else {
            panic!("expected two exact domain identities");
        };
        assert_eq!(d3.packed_bits(), d4.packed_bits());
        assert_eq!((d3.width(), d4.width()), (3, 4));
        assert_ne!(d3, d4);
    }

    #[test]
    fn legacy_human_parser_does_not_guess_three_bit_domain_identity() {
        let exprs = parse("001").expect("historical human parser behavior");
        assert_eq!(exprs, vec![Expr::Integer(1)]);
    }
}

#[cfg(test)]
mod sid8_identity_tests {
    use super::*;

    #[test]
    fn exact_bare_eight_bit_binary_token_parses_as_typed_sid8() {
        let exprs = parse("00000101").expect("exact bare SID8 must parse");
        assert_eq!(exprs, vec![Expr::Sid(sens::sid!(00000101))]);
    }

    #[test]
    fn quoted_eight_bit_text_remains_string_and_never_mints_sid8() {
        let exprs = parse("\"00000101\"").expect("quoted text must parse");
        assert_eq!(exprs, vec![Expr::String("00000101".to_string())]);
    }

    #[test]
    fn decimal_101_is_not_sid_00000101() {
        let exprs = parse("101").expect("decimal integer must parse");
        assert_eq!(exprs, vec![Expr::Integer(101)]);
    }

    #[test]
    fn explicit_plus_disambiguates_eight_binary_digits_as_decimal_integer() {
        let exprs = parse("+10000000").expect("signed decimal integer must parse");
        assert_eq!(exprs, vec![Expr::Integer(10_000_000)]);
    }

    #[test]
    fn wrong_width_binary_digit_token_is_not_sid8() {
        let exprs = parse("0000101").expect("seven-bit spelling remains ordinary numeric token");
        assert_eq!(exprs, vec![Expr::Integer(101)]);
    }
}
