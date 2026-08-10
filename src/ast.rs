#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Integer(i64),
    Symbol(String),
    List(Vec<Expr>),
    DottedList(Vec<Expr>, Box<Expr>), // (a b . c)
    String(String),
}

impl Expr {
    pub fn is_symbol(&self, expected: &str) -> bool {
        match self {
            Expr::Symbol(s) => s == expected,
            _ => false,
        }
    }
}
