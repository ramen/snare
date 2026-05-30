use std::collections::BTreeMap;
use std::fmt;
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
