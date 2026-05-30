#[derive(Clone, Debug)]
pub enum Statement {
    Let(String, Expr),
    Assign(String, Expr),
    Expr(Expr),
}

#[derive(Clone, Debug)]
pub struct Parameter {
    pub name: String,
    pub default: Option<Expr>,
}

#[derive(Clone, Debug)]
pub enum Expr {
    Null,
    Bool(bool),
    Number(serde_json::Number),
    String(String),
    Array(Vec<Expr>),
    Object(Vec<(String, Expr)>),
    Identifier(String),
    Unary(String, Box<Expr>),
    Binary(Box<Expr>, String, Box<Expr>),
    Call(Box<Expr>, Vec<Argument>),
    Member(Box<Expr>, String),
    Function(Vec<Parameter>, Box<Expr>),
    If(Box<Expr>, Box<Expr>, Box<Expr>),
    Do(Vec<Statement>, Box<Expr>),
}

#[derive(Clone, Debug)]
pub enum Argument {
    Positional(Expr),
    Named(String, Expr),
}
