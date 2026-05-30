mod ast;
mod builtins;
mod eval;
mod parser;
mod value;

use std::fmt;

pub use value::Value;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Parse(String),
    Name(String),
    Type(String),
    Call(String),
    Sqlx(sqlx::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(message) => write!(formatter, "syntax error: {message}"),
            Self::Name(message) => write!(formatter, "name error: {message}"),
            Self::Type(message) => write!(formatter, "type error: {message}"),
            Self::Call(message) => write!(formatter, "call error: {message}"),
            Self::Sqlx(error) => write!(formatter, "sqlite error: {error}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<sqlx::Error> for Error {
    fn from(error: sqlx::Error) -> Self {
        Self::Sqlx(error)
    }
}

pub struct Engine {
    environment: eval::Environment,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine {
    pub fn new() -> Self {
        let environment = eval::Environment::default();
        builtins::install(&environment);
        Self { environment }
    }

    pub fn eval(&mut self, source: &str) -> Result<Value> {
        eval::eval_statements(&parser::parse(source)?, &self.environment)
    }

    pub fn set(&mut self, name: impl Into<String>, value: Value) {
        self.environment.set(name, value);
    }
}
