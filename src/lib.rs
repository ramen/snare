mod ast;
mod builtins;
mod eval;
mod module;
mod parser;
mod value;

use std::fmt;
use std::path::Path;

use ast::Span;
pub use module::Module;
pub use parser::is_incomplete;
pub use value::{NamedArguments, Value, ValueMap};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Parse(String),
    Name(String),
    Type(String),
    Call(String),
    Io(std::io::Error),
    Sqlx(sqlx::Error),
    Context {
        message: String,
        source: Box<Error>,
    },
    Located {
        span: Span,
        source: Box<Error>,
    },
    Frame {
        span: Span,
        source: Box<Error>,
    },
    Diagnostic {
        line: usize,
        column: usize,
        width: usize,
        excerpt: String,
        source: Box<Error>,
    },
    DiagnosticFrame {
        line: usize,
        column: usize,
        excerpt: String,
        source: Box<Error>,
    },
}

impl Error {
    pub fn context(self, message: impl Into<String>) -> Self {
        Self::Context {
            message: message.into(),
            source: Box::new(self),
        }
    }

    fn at(self, span: Span) -> Self {
        if self.has_location() {
            self
        } else {
            Self::Located {
                span,
                source: Box::new(self),
            }
        }
    }

    pub(crate) fn frame(self, span: Span) -> Self {
        Self::Frame {
            span,
            source: Box::new(self),
        }
    }

    fn has_location(&self) -> bool {
        match self {
            Self::Located { .. } | Self::Diagnostic { .. } | Self::DiagnosticFrame { .. } => true,
            Self::Context { source, .. } | Self::Frame { source, .. } => source.has_location(),
            _ => false,
        }
    }

    pub(crate) fn with_source(self, source_text: &str) -> Self {
        match self {
            Self::Located { span, source } => {
                let (line, column, excerpt, line_end) = location(source_text, span);
                let width = source_text[span.start.min(source_text.len())
                    ..span
                        .end
                        .min(line_end)
                        .max(span.start.min(source_text.len()))]
                    .chars()
                    .count()
                    .max(1);
                Self::Diagnostic {
                    line,
                    column,
                    width,
                    excerpt,
                    source,
                }
            }
            Self::Frame { span, source } => {
                let (line, column, excerpt, _) = location(source_text, span);
                Self::DiagnosticFrame {
                    line,
                    column,
                    excerpt,
                    source: Box::new(source.with_source(source_text)),
                }
            }
            Self::Context { message, source } => Self::Context {
                message,
                source: Box::new(source.with_source(source_text)),
            },
            error => error,
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
            Self::Located { source, .. } => write!(formatter, "{source}"),
            Self::Diagnostic {
                line,
                column,
                width,
                excerpt,
                source,
            } => write!(
                formatter,
                "{source}\n --> {line}:{column}\n  |\n{line} | {excerpt}\n  | {}{}",
                " ".repeat(column.saturating_sub(1)),
                "^".repeat(*width)
            ),
            Self::Frame { source, .. } => write!(formatter, "{source}"),
            Self::DiagnosticFrame {
                line,
                column,
                excerpt,
                source,
            } => write!(
                formatter,
                "{source}\n called from {line}:{column}\n{line} | {excerpt}\n  | {}^",
                " ".repeat(column.saturating_sub(1))
            ),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Sqlx(error) => Some(error),
            Self::Context { source, .. } => Some(source),
            Self::Located { source, .. }
            | Self::Frame { source, .. }
            | Self::Diagnostic { source, .. }
            | Self::DiagnosticFrame { source, .. } => Some(source),
            _ => None,
        }
    }
}

fn location(source_text: &str, span: Span) -> (usize, usize, String, usize) {
    let start = span.start.min(source_text.len());
    let before = &source_text[..start];
    let line = before.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let line_start = before.rfind('\n').map_or(0, |index| index + 1);
    let line_end = source_text[start..]
        .find('\n')
        .map_or(source_text.len(), |index| start + index);
    let excerpt = source_text[line_start..line_end].to_owned();
    let column = source_text[line_start..start].chars().count() + 1;
    (line, column, excerpt, line_end)
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
            .map_err(|error| error.with_source(source))
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
