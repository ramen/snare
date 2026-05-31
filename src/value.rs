use std::collections::BTreeMap;
use std::fmt;
use std::fmt::Write as _;
use std::rc::Rc;

use sqlx::SqlitePool;

use crate::ast::{Expr, Parameter};
use crate::eval::Environment;
use crate::{Error, Result};

pub type ValueMap = BTreeMap<String, Value>;
pub type NamedArguments = ValueMap;
pub type NativeFunction = Rc<dyn Fn(Vec<Value>, NamedArguments) -> Result<Value>>;

#[derive(Clone)]
pub enum Value {
    Null,
    Bool(bool),
    Number(serde_json::Number),
    String(String),
    Array(Vec<Value>),
    Object(ValueMap),
    Function(Rc<Function>),
    Native(NativeFunction),
    Database(SqlitePool),
}

#[derive(Clone)]
pub struct Function {
    pub parameters: Vec<Parameter>,
    pub body: Expr,
    pub environment: Environment,
}

impl Value {
    pub fn native(
        function: impl Fn(Vec<Value>, NamedArguments) -> Result<Value> + 'static,
    ) -> Self {
        Self::Native(Rc::new(function))
    }

    pub fn from_json(value: serde_json::Value) -> Self {
        match value {
            serde_json::Value::Null => Self::Null,
            serde_json::Value::Bool(value) => Self::Bool(value),
            serde_json::Value::Number(value) => Self::Number(value),
            serde_json::Value::String(value) => Self::String(value),
            serde_json::Value::Array(values) => {
                Self::Array(values.into_iter().map(Self::from_json).collect())
            }
            serde_json::Value::Object(values) => Self::Object(
                values
                    .into_iter()
                    .map(|(key, value)| (key, Self::from_json(value)))
                    .collect(),
            ),
        }
    }

    pub fn to_json(&self) -> Result<serde_json::Value> {
        Ok(match self {
            Self::Null => serde_json::Value::Null,
            Self::Bool(value) => serde_json::Value::Bool(*value),
            Self::Number(value) => serde_json::Value::Number(value.clone()),
            Self::String(value) => serde_json::Value::String(value.clone()),
            Self::Array(values) => {
                serde_json::Value::Array(values.iter().map(Self::to_json).collect::<Result<_>>()?)
            }
            Self::Object(values) => serde_json::Value::Object(
                values
                    .iter()
                    .map(|(key, value)| Ok((key.clone(), value.to_json()?)))
                    .collect::<Result<_>>()?,
            ),
            Self::Function(_) | Self::Native(_) => {
                return Err(Error::Type("function is not JSON".into()));
            }
            Self::Database(_) => return Err(Error::Type("database handle is not JSON".into())),
        })
    }

    pub fn truthy(&self) -> bool {
        match self {
            Self::Null => false,
            Self::Bool(value) => *value,
            Self::Number(value) => value.as_f64() != Some(0.0),
            Self::String(value) => !value.is_empty(),
            Self::Array(value) => !value.is_empty(),
            Self::Object(value) => !value.is_empty(),
            Self::Function(_) | Self::Native(_) | Self::Database(_) => true,
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::Bool(_) => "bool",
            Self::Number(_) => "number",
            Self::String(_) => "string",
            Self::Array(_) => "array",
            Self::Object(_) => "object",
            Self::Function(_) | Self::Native(_) => "function",
            Self::Database(_) => "sqlite_database",
        }
    }

    pub fn to_print_string(&self) -> String {
        match self {
            Self::String(value) => value.clone(),
            value => value.to_string(),
        }
    }

    pub fn to_pretty_string(&self) -> String {
        let mut output = String::new();
        self.write_pretty(&mut output, 0)
            .expect("writing to a string cannot fail");
        output
    }

    fn write_pretty(&self, output: &mut String, indent: usize) -> fmt::Result {
        match self {
            Self::Array(values) if values.is_empty() => write!(output, "[]"),
            Self::Array(values) => {
                writeln!(output, "[")?;
                for (index, value) in values.iter().enumerate() {
                    write!(output, "{:width$}", "", width = indent + 2)?;
                    value.write_pretty(output, indent + 2)?;
                    if index + 1 < values.len() {
                        write!(output, ",")?;
                    }
                    writeln!(output)?;
                }
                write!(output, "{:width$}]", "", width = indent)
            }
            Self::Object(values) if values.is_empty() => write!(output, "{{}}"),
            Self::Object(values) => {
                writeln!(output, "{{")?;
                for (index, (key, value)) in values.iter().enumerate() {
                    write!(
                        output,
                        "{:width$}{}: ",
                        "",
                        serde_json::to_string(key).expect("object key is JSON"),
                        width = indent + 2
                    )?;
                    value.write_pretty(output, indent + 2)?;
                    if index + 1 < values.len() {
                        write!(output, ",")?;
                    }
                    writeln!(output)?;
                }
                write!(output, "{:width$}}}", "", width = indent)
            }
            value => write!(output, "{value}"),
        }
    }

    pub fn equal(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Null, Self::Null) => true,
            (Self::Bool(left), Self::Bool(right)) => left == right,
            (Self::Number(left), Self::Number(right)) => left == right,
            (Self::String(left), Self::String(right)) => left == right,
            (Self::Array(left), Self::Array(right)) => {
                left.len() == right.len()
                    && left
                        .iter()
                        .zip(right)
                        .all(|(left, right)| left.equal(right))
            }
            (Self::Object(left), Self::Object(right)) => {
                left.len() == right.len()
                    && left
                        .iter()
                        .all(|(key, left)| right.get(key).is_some_and(|right| left.equal(right)))
            }
            _ => false,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Null => write!(formatter, "null"),
            Self::Bool(value) => write!(formatter, "{value}"),
            Self::Number(value) => write!(formatter, "{value}"),
            Self::String(value) => write!(
                formatter,
                "{}",
                serde_json::to_string(value).expect("string is JSON")
            ),
            Self::Array(values) => {
                write!(formatter, "[")?;
                for (index, value) in values.iter().enumerate() {
                    if index > 0 {
                        write!(formatter, ",")?;
                    }
                    write!(formatter, "{value}")?;
                }
                write!(formatter, "]")
            }
            Self::Object(values) => {
                write!(formatter, "{{")?;
                for (index, (key, value)) in values.iter().enumerate() {
                    if index > 0 {
                        write!(formatter, ",")?;
                    }
                    write!(
                        formatter,
                        "{}:{value}",
                        serde_json::to_string(key).expect("object key is JSON")
                    )?;
                }
                write!(formatter, "}}")
            }
            Self::Function(_) | Self::Native(_) => write!(formatter, "<function>"),
            Self::Database(_) => write!(formatter, "<sqlite database>"),
        }
    }
}
