mod ast;
mod builtins;
mod eval;
mod module;
mod parser;
mod value;

use std::fmt;
use std::path::Path;

pub use module::Module;
pub use parser::is_incomplete;
pub use value::{NamedArguments, Value};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Parse(String),
    Name(String),
    Type(String),
    Call(String),
    Io(std::io::Error),
    Sqlx(sqlx::Error),
    Context { message: String, source: Box<Error> },
}

impl Error {
    pub fn context(self, message: impl Into<String>) -> Self {
        Self::Context {
            message: message.into(),
            source: Box::new(self),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(message) => write!(formatter, "syntax error: {message}"),
            Self::Name(message) => write!(formatter, "name error: {message}"),
            Self::Type(message) => write!(formatter, "type error: {message}"),
            Self::Call(message) => write!(formatter, "call error: {message}"),
            Self::Io(error) => write!(formatter, "I/O error: {error}"),
            Self::Sqlx(error) => write!(formatter, "sqlite error: {error}"),
            Self::Context { message, source } => write!(formatter, "{message}: {source}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Sqlx(error) => Some(error),
            Self::Context { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<sqlx::Error> for Error {
    fn from(error: sqlx::Error) -> Self {
        Self::Sqlx(error)
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
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

    pub fn eval_file(&mut self, path: impl AsRef<Path>) -> Result<Value> {
        let path = path.as_ref();
        let context = || format!("while evaluating {}", path.display());
        let source =
            std::fs::read_to_string(path).map_err(|error| Error::from(error).context(context()))?;
        self.eval(&source).map_err(|error| error.context(context()))
    }

    pub fn set(&mut self, name: impl Into<String>, value: Value) {
        self.environment.set(name, value);
    }

    pub fn register_fn(
        &mut self,
        name: impl Into<String>,
        function: impl Fn(Vec<Value>, NamedArguments) -> Result<Value> + 'static,
    ) {
        self.set(name, Value::native(function));
    }

    pub fn register_module(&mut self, name: impl Into<String>, module: Module) {
        self.set(name, module.into());
    }
}
