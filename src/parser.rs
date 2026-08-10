use crate::ast::Expr;


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
            '(' | ')' | '\'' => {
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
        "'" => {
            let next_expr = parse_expr(tokens)?;
            Ok(Expr::List(vec![
                Expr::Symbol("quote".to_string()),
                next_expr,
            ]))
        }
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
