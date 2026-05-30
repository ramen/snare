use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use crate::ast::{Argument, Expr, Statement};
use crate::value::{Function, Value};
use crate::{Error, Result};

#[derive(Clone, Default)]
pub struct Environment(Rc<RefCell<Scope>>);

#[derive(Default)]
struct Scope {
    parent: Option<Environment>,
    values: BTreeMap<String, Value>,
}

impl Environment {
    pub fn child(&self) -> Self {
        Self(Rc::new(RefCell::new(Scope {
            parent: Some(self.clone()),
            values: BTreeMap::new(),
        })))
    }

    pub fn set(&self, name: impl Into<String>, value: Value) {
        self.0.borrow_mut().values.insert(name.into(), value);
    }

    pub fn assign(&self, name: &str, value: Value) -> Result<()> {
        let parent = {
            let mut scope = self.0.borrow_mut();
            if scope.values.contains_key(name) {
                scope.values.insert(name.to_owned(), value);
                return Ok(());
            }
            scope.parent.clone()
        };
        parent
            .ok_or_else(|| Error::Name(format!("unknown name: {name}")))?
            .assign(name, value)
    }

    pub fn get(&self, name: &str) -> Option<Value> {
        let scope = self.0.borrow();
        scope
            .values
            .get(name)
            .cloned()
            .or_else(|| scope.parent.as_ref()?.get(name))
    }
}

pub fn eval_statements(statements: &[Statement], environment: &Environment) -> Result<Value> {
    let mut result = Value::Null;
    for statement in statements {
        result = match statement {
            Statement::Let(name, expr) => {
                let value = eval(expr, environment)?;
                environment.set(name, value);
                Value::Null
            }
            Statement::Assign(name, expr) => {
                let value = eval(expr, environment)?;
                environment.assign(name, value)?;
                Value::Null
            }
            Statement::Expr(expr) => eval(expr, environment)?,
        };
    }
    Ok(result)
}

pub fn eval(expr: &Expr, environment: &Environment) -> Result<Value> {
    match expr {
        Expr::Null => Ok(Value::Null),
        Expr::Bool(value) => Ok(Value::Bool(*value)),
        Expr::Number(value) => Ok(Value::Number(value.clone())),
        Expr::String(value) => Ok(Value::String(value.clone())),
        Expr::Array(values) => values
            .iter()
            .map(|value| eval(value, environment))
            .collect::<Result<_>>()
            .map(Value::Array),
        Expr::Object(values) => values
            .iter()
            .map(|(key, value)| Ok((key.clone(), eval(value, environment)?)))
            .collect::<Result<_>>()
            .map(Value::Object),
        Expr::Identifier(name) => environment
            .get(name)
            .ok_or_else(|| Error::Name(format!("unknown name: {name}"))),
        Expr::Unary(operator, expr) => eval_unary(operator, eval(expr, environment)?),
        Expr::Binary(left, operator, right) => {
            let left = eval(left, environment)?;
            if operator == "&&" && !left.truthy() {
                return Ok(left);
            }
            if operator == "||" && left.truthy() {
                return Ok(left);
            }
            eval_binary(left, operator, eval(right, environment)?)
        }
        Expr::Member(object, name) => match eval(object, environment)? {
            Value::Object(mut values) => values
                .remove(name)
                .ok_or_else(|| Error::Name(format!("object has no member: {name}"))),
            _ => Err(Error::Type("member access expects an object".into())),
        },
        Expr::Function(parameters, body) => Ok(Value::Function(Rc::new(Function {
            parameters: parameters.clone(),
            body: *body.clone(),
            environment: environment.clone(),
        }))),
        Expr::Call(function, arguments) => {
            call(eval(function, environment)?, arguments, environment)
        }
        Expr::If(condition, then_expr, else_expr) => {
            if eval(condition, environment)?.truthy() {
                eval(then_expr, environment)
            } else {
                eval(else_expr, environment)
            }
        }
        Expr::Do(statements, result) => {
            let environment = environment.child();
            eval_statements(statements, &environment)?;
            eval(result, &environment)
        }
    }
}

fn call(function: Value, arguments: &[Argument], environment: &Environment) -> Result<Value> {
    let mut positional = vec![];
    let mut named = BTreeMap::new();
    for argument in arguments {
        match argument {
            Argument::Positional(expr) if named.is_empty() => {
                positional.push(eval(expr, environment)?)
            }
            Argument::Positional(_) => {
                return Err(Error::Call(
                    "positional argument follows named argument".into(),
                ));
            }
            Argument::Named(name, expr) => {
                if named
                    .insert(name.clone(), eval(expr, environment)?)
                    .is_some()
                {
                    return Err(Error::Call(format!("duplicate argument: {name}")));
                }
            }
        }
    }
    match function {
        Value::Native(function) => function(positional, named),
        Value::Function(function) => call_function(&function, positional, named),
        _ => Err(Error::Type("value is not callable".into())),
    }
}

fn call_function(
    function: &Function,
    positional: Vec<Value>,
    mut named: BTreeMap<String, Value>,
) -> Result<Value> {
    if positional.len() > function.parameters.len() {
        return Err(Error::Call("too many positional arguments".into()));
    }
    let environment = function.environment.child();
    for (index, parameter) in function.parameters.iter().enumerate() {
        let value = if let Some(value) = positional.get(index) {
            if named.contains_key(&parameter.name) {
                return Err(Error::Call(format!(
                    "multiple values for argument: {}",
                    parameter.name
                )));
            }
            value.clone()
        } else if let Some(value) = named.remove(&parameter.name) {
            value
        } else if let Some(default) = &parameter.default {
            eval(default, &environment)?
        } else {
            return Err(Error::Call(format!("missing argument: {}", parameter.name)));
        };
        environment.set(&parameter.name, value);
    }
    if let Some(name) = named.keys().next() {
        return Err(Error::Call(format!("unexpected argument: {name}")));
    }
    eval(&function.body, &environment)
}

fn eval_unary(operator: &str, value: Value) -> Result<Value> {
    match operator {
        "!" => Ok(Value::Bool(!value.truthy())),
        "-" => number(value).and_then(|value| json_number(-value)),
        _ => unreachable!("unary operator"),
    }
}

fn eval_binary(left: Value, operator: &str, right: Value) -> Result<Value> {
    match operator {
        "&&" | "||" => Ok(right),
        "==" => Ok(Value::Bool(equal(&left, &right))),
        "!=" => Ok(Value::Bool(!equal(&left, &right))),
        "+" => match (left, right) {
            (Value::String(left), Value::String(right)) => Ok(Value::String(left + &right)),
            (left, right) => json_number(number(left)? + number(right)?),
        },
        "-" => json_number(number(left)? - number(right)?),
        "*" => json_number(number(left)? * number(right)?),
        "/" => {
            let right = number(right)?;
            if right == 0.0 {
                return Err(Error::Type("division by zero".into()));
            }
            json_number(number(left)? / right)
        }
        "%" => json_number(number(left)? % number(right)?),
        "<" | "<=" | ">" | ">=" => {
            let (left, right) = (number(left)?, number(right)?);
            Ok(Value::Bool(match operator {
                "<" => left < right,
                "<=" => left <= right,
                ">" => left > right,
                ">=" => left >= right,
                _ => unreachable!(),
            }))
        }
        _ => unreachable!("binary operator"),
    }
}

fn number(value: Value) -> Result<f64> {
    match value {
        Value::Number(value) => value
            .as_f64()
            .ok_or_else(|| Error::Type("invalid number".into())),
        _ => Err(Error::Type("expected a number".into())),
    }
}

fn json_number(value: f64) -> Result<Value> {
    if value.fract() == 0.0 && value >= i64::MIN as f64 && value <= i64::MAX as f64 {
        return Ok(Value::Number((value as i64).into()));
    }
    serde_json::Number::from_f64(value)
        .map(Value::Number)
        .ok_or_else(|| Error::Type("number is not representable as JSON".into()))
}

fn equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Null, Value::Null) => true,
        (Value::Bool(left), Value::Bool(right)) => left == right,
        (Value::Number(left), Value::Number(right)) => left == right,
        (Value::String(left), Value::String(right)) => left == right,
        (Value::Array(left), Value::Array(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right)
                    .all(|(left, right)| equal(left, right))
        }
        (Value::Object(left), Value::Object(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .all(|(key, left)| right.get(key).is_some_and(|right| equal(left, right)))
        }
        _ => false,
    }
}
