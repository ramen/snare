use std::cell::RefCell;
use std::rc::Rc;

use crate::ast::{Argument, Expr, ExprKind, Statement, StatementKind};
use crate::value::{Function, NamedArguments, Value, ValueMap};
use crate::{Error, Result};

#[derive(Clone, Default)]
pub struct Environment(Rc<RefCell<Scope>>);

#[derive(Default)]
struct Scope {
    parent: Option<Environment>,
    source: Option<Rc<str>>,
    values: ValueMap,
}

impl Environment {
    pub fn child(&self) -> Self {
        Self(Rc::new(RefCell::new(Scope {
            parent: Some(self.clone()),
            source: None,
            values: ValueMap::new(),
        })))
    }

    pub fn child_with_source(&self, source: impl Into<Rc<str>>) -> Self {
        Self(Rc::new(RefCell::new(Scope {
            parent: Some(self.clone()),
            source: Some(source.into()),
            values: ValueMap::new(),
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

    pub fn globals(&self) -> ValueMap {
        let scope = self.0.borrow();
        match &scope.parent {
            Some(parent) => parent.globals(),
            None => scope.values.clone(),
        }
    }

    fn source(&self) -> Option<Rc<str>> {
        let scope = self.0.borrow();
        scope
            .source
            .clone()
            .or_else(|| scope.parent.as_ref()?.source())
    }
}

pub fn eval_statements(statements: &[Statement], environment: &Environment) -> Result<Value> {
    let mut result = Value::Null;
    for statement in statements {
        result = match &statement.kind {
            StatementKind::Let(name, expr) => {
                let value = eval(expr, environment)?;
                environment.set(name, value);
                Value::Null
            }
            StatementKind::Assign(name, expr) => {
                let value = eval(expr, environment)?;
                environment
                    .assign(name, value)
                    .map_err(|error| error.at(statement.span))?;
                Value::Null
            }
            StatementKind::Expr(expr) => eval(expr, environment)?,
        };
    }
    Ok(result)
}

pub fn eval(expr: &Expr, environment: &Environment) -> Result<Value> {
    eval_inner(expr, environment).map_err(|error| {
        let error = error.at(expr.span);
        match environment.source() {
            Some(source) => error.with_source(&source),
            None => error,
        }
    })
}

fn eval_inner(expr: &Expr, environment: &Environment) -> Result<Value> {
    match &expr.kind {
        ExprKind::Null => Ok(Value::Null),
        ExprKind::Bool(value) => Ok(Value::Bool(*value)),
        ExprKind::Number(value) => Ok(Value::Number(value.clone())),
        ExprKind::String(value) => Ok(Value::String(value.clone())),
        ExprKind::Array(values) => values
            .iter()
            .map(|value| eval(value, environment))
            .collect::<Result<_>>()
            .map(Value::Array),
        ExprKind::Object(values) => values
            .iter()
            .map(|(key, value)| Ok((key.clone(), eval(value, environment)?)))
            .collect::<Result<_>>()
            .map(|m| Value::Object(m, None)),
        ExprKind::Identifier(name) => environment
            .get(name)
            .ok_or_else(|| Error::Name(format!("unknown name: {name}"))),
        ExprKind::Unary(operator, expr) => eval_unary(operator, eval(expr, environment)?),
        ExprKind::Binary(left, operator, right) => {
            let left = eval(left, environment)?;
            if operator == "&&" && !left.truthy() {
                return Ok(left);
            }
            if operator == "||" && left.truthy() {
                return Ok(left);
            }
            eval_binary(left, operator, eval(right, environment)?)
        }
        ExprKind::Member(object, name) => match eval(object, environment)? {
            Value::Object(mut values, _) => values
                .remove(name)
                .ok_or_else(|| Error::Name(format!("object has no member: {name}"))),
            _ => Err(Error::Type("member access expects an object".into())),
        },
        ExprKind::Index(collection, index) => {
            lookup_index(eval(collection, environment)?, eval(index, environment)?)
        }
        ExprKind::Function(parameters, body) => Ok(Value::Function(Rc::new(Function {
            parameters: parameters.clone(),
            body: *body.clone(),
            environment: environment.clone(),
        }))),
        ExprKind::Call(function, arguments) => {
            call(eval(function, environment)?, arguments, environment)
                .map_err(|error| error.frame(expr.span))
        }
        ExprKind::If(condition, then_expr, else_expr) => {
            if eval(condition, environment)?.truthy() {
                eval(then_expr, environment)
            } else {
                eval(else_expr, environment)
            }
        }
        ExprKind::Do(statements) => {
            let environment = environment.child();
            eval_statements(statements, &environment)
        }
    }
}

fn lookup_index(collection: Value, index: Value) -> Result<Value> {
    match (collection, index) {
        (Value::Array(values), Value::Number(index)) => values
            .get(array_index(values.len(), &index)?)
            .cloned()
            .ok_or_else(|| Error::Name("array index out of bounds".into())),
        (Value::Object(mut values, _), Value::String(key)) => values
            .remove(&key)
            .ok_or_else(|| Error::Name(format!("object has no member: {key}"))),
        (Value::Array(_), _) => Err(Error::Type("array index must be an integer".into())),
        (Value::Object(_, _), _) => Err(Error::Type("object index must be a string".into())),
        _ => Err(Error::Type("indexing expects an array or object".into())),
    }
}

fn array_index(len: usize, index: &serde_json::Number) -> Result<usize> {
    let index = index
        .as_i64()
        .ok_or_else(|| Error::Type("array index must be an integer".into()))?;
    if index < 0 {
        len.checked_sub(index.unsigned_abs() as usize)
            .ok_or_else(|| Error::Name("array index out of bounds".into()))
    } else {
        usize::try_from(index).map_err(|_| Error::Name("array index out of bounds".into()))
    }
}

fn call(function: Value, arguments: &[Argument], environment: &Environment) -> Result<Value> {
    let mut positional = vec![];
    let mut named = NamedArguments::new();
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
    call_value(function, positional, named)
}

pub fn call_value(function: Value, positional: Vec<Value>, named: NamedArguments) -> Result<Value> {
    match function {
        Value::Native(function) => function(positional, named),
        Value::Function(function) => call_function(&function, positional, named),
        _ => Err(Error::Type("value is not callable".into())),
    }
}

fn call_function(
    function: &Function,
    positional: Vec<Value>,
    mut named: NamedArguments,
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
        "==" => Ok(Value::Bool(left.equal(&right))),
        "!=" => Ok(Value::Bool(!left.equal(&right))),
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
