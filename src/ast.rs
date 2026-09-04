#[derive(Debug, Clone, PartialEq)]
pub enum NumericBufferLiteral {
    I32(Vec<i32>),
    /// Stored IEEE-754 binary32 bits, preserving signed zero exactly.
    F32(Vec<u32>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Integer(i64),
    /// Exact rational numeral `n/d` (d > 0, gcd-reduced at parse time).
    Rational(i64, u64),
    Symbol(String),
    List(Vec<Expr>),
    DottedList(Vec<Expr>, Box<Expr>), // (a b . c)
    String(String),
    NumericBuffer(NumericBufferLiteral),
}

impl Expr {
    pub fn is_symbol(&self, expected: &str) -> bool {
        match self {
            Expr::Symbol(s) => s == expected,
            _ => false,
        }
    }
}
