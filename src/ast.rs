#[derive(Clone, Copy, Debug)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug)]
pub struct Statement {
    pub kind: StatementKind,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum StatementKind {
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
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum ExprKind {
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
    Index(Box<Expr>, Box<Expr>),
    Function(Vec<Parameter>, Box<Expr>),
    If(Box<Expr>, Box<Expr>, Box<Expr>),
    Do(Vec<Statement>, Box<Expr>),
}

#[derive(Clone, Debug)]
pub enum Argument {
    Positional(Expr),
    Named(String, Expr),
}
