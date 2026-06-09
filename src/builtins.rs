use std::cmp::Ordering;
use std::sync::OnceLock;

use sqlx::{Column, Row, TypeInfo, ValueRef};
use tokio::runtime::Runtime;

use crate::eval::{Environment, call_value};
use crate::value::{NamedArguments, Value, ValueMap};
use crate::{Error, Module, Result, parser};

pub fn install(environment: &Environment) {
    environment.set("print", Value::native(print));
    environment.set("type_of", Value::native(type_of));
    environment.set("is_type", Value::native(is_type));
    environment.set("get_tag", Value::native(get_tag));
    environment.set("set_tag", Value::native(set_tag));
    environment.set("string", Value::native(string));
    environment.set("number", Value::native(to_number));
    environment.set("bool", Value::native(bool));
    environment.set("not", Value::native(not));
    environment.set("abs", Value::native(abs));
    environment.set("sum", Value::native(sum));
    environment.set("product", Value::native(product));
    environment.set("min", Value::native(min));
    environment.set("max", Value::native(max));
    environment.set("any", Value::native(any));
    environment.set("all", Value::native(all));
    environment.set("len", Value::native(len));
    environment.set("keys", Value::native(keys));
    environment.set("values", Value::native(values));
    environment.set("pairs", Value::native(pairs));
    environment.set("get", Value::native(get));
    environment.set("has", Value::native(has));
    environment.set("set", Value::native(set));
    environment.set("remove", Value::native(remove));
    environment.set("merge", Value::native(merge));
    environment.set("contains", Value::native(contains));
    environment.set("push", Value::native(push));
    environment.set("append", Value::native(append));
    environment.set("first", Value::native(first));
    environment.set("rest", Value::native(rest));
    environment.set("slice", Value::native(slice));
    environment.set("map", Value::native(map));
    environment.set("filter", Value::native(filter));
    environment.set("sort", Value::native(sort));
    environment.set("range", Value::native(range));
    environment.set("reduce", Value::native(reduce));
    environment.set("join", Value::native(join));
    environment.set("split", Value::native(split));
    environment.set("lower", Value::native(lower));
    environment.set("upper", Value::native(upper));
    environment.set("starts_with", Value::native(starts_with));
    environment.set("ends_with", Value::native(ends_with));
    environment.set("replace", Value::native(replace));
    let globals_environment = environment.clone();
    environment.set(
        "globals",
        Value::native(move |positional, named| globals(&globals_environment, positional, named)),
    );
    let load_environment = environment.clone();
    environment.set(
        "load",
        Value::native(move |positional, named| load(&load_environment, positional, named)),
    );

    let open = Value::native(sqlite_open);
    let mut sqlite = Module::new();
    sqlite.set("open", open);
    environment.set("sqlite", sqlite.into());
}

fn runtime() -> &'static Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| Runtime::new().expect("tokio runtime"))
}

fn print(positional: Vec<Value>, named: NamedArguments) -> Result<Value> {
    reject_named(&named)?;
    println!(
        "{}",
        positional
            .iter()
            .map(Value::to_print_string)
            .collect::<Vec<_>>()
            .join(" ")
    );
    Ok(Value::Null)
}

fn type_of(positional: Vec<Value>, named: NamedArguments) -> Result<Value> {
    reject_named(&named)?;
    expect_len(&positional, 1)?;
    Ok(Value::String(positional[0].type_name().into()))
}

fn is_type(positional: Vec<Value>, named: NamedArguments) -> Result<Value> {
    reject_named(&named)?;
    expect_len(&positional, 2)?;
    let Value::String(expected) = &positional[1] else {
        return Err(Error::Type("is_type expects a type name string".into()));
    };
    Ok(Value::Bool(positional[0].type_name() == expected))
}

fn get_tag(positional: Vec<Value>, named: NamedArguments) -> Result<Value> {
    reject_named(&named)?;
    expect_len(&positional, 1)?;
    match &positional[0] {
        Value::Object(_, Some(tag)) => Ok(Value::String(tag.clone())),
        _ => Ok(Value::Null)
    }
}

fn set_tag(positional: Vec<Value>, named: NamedArguments) -> Result<Value> {
    reject_named(&named)?;
    expect_len(&positional, 2)?;
    let Value::Object(map, _) = &positional[0] else {
        return Err(Error::Type("set_tag expects an object".into()))
    };
    let Value::String(tag) = &positional[1] else {
        return Err(Error::Type("set_tag expects a string".into()))
    };
    Ok(Value::Object(map.clone(), Some(tag.clone())))
}

fn string(positional: Vec<Value>, named: NamedArguments) -> Result<Value> {
    reject_named(&named)?;
    expect_len(&positional, 1)?;
    Ok(Value::String(match &positional[0] {
        Value::String(value) => value.clone(),
        value => value.to_string(),
    }))
}

fn to_number(positional: Vec<Value>, named: NamedArguments) -> Result<Value> {
    reject_named(&named)?;
    expect_len(&positional, 1)?;
    match &positional[0] {
        Value::Number(value) => Ok(Value::Number(value.clone())),
        Value::String(value) => value
            .parse()
            .map(Value::Number)
            .map_err(|_| Error::Type("number expects a JSON number string".into())),
        _ => Err(Error::Type("number expects a number or string".into())),
    }
}

fn bool(positional: Vec<Value>, named: NamedArguments) -> Result<Value> {
    reject_named(&named)?;
    expect_len(&positional, 1)?;
    Ok(Value::Bool(positional[0].truthy()))
}

fn not(positional: Vec<Value>, named: NamedArguments) -> Result<Value> {
    reject_named(&named)?;
    expect_len(&positional, 1)?;
    Ok(Value::Bool(!positional[0].truthy()))
}

fn abs(positional: Vec<Value>, named: NamedArguments) -> Result<Value> {
    reject_named(&named)?;
    expect_len(&positional, 1)?;
    match &positional[0] {
        Value::Number(value) if value.is_i64() => value
            .as_i64()
            .expect("i64")
            .checked_abs()
            .map(|value| Value::Number(value.into()))
            .ok_or_else(|| Error::Type("abs overflowed".into())),
        Value::Number(value) => json_number(value.as_f64().expect("f64").abs()),
        _ => Err(Error::Type("abs expects a number".into())),
    }
}

fn sum(positional: Vec<Value>, named: NamedArguments) -> Result<Value> {
    numeric_fold(positional, named, "sum", 0.0, |total, value| total + value)
}

fn product(positional: Vec<Value>, named: NamedArguments) -> Result<Value> {
    numeric_fold(positional, named, "product", 1.0, |total, value| {
        total * value
    })
}

fn min(positional: Vec<Value>, named: NamedArguments) -> Result<Value> {
    extremum(positional, named, "min", Ordering::Less)
}

fn max(positional: Vec<Value>, named: NamedArguments) -> Result<Value> {
    extremum(positional, named, "max", Ordering::Greater)
}

fn any(positional: Vec<Value>, named: NamedArguments) -> Result<Value> {
    truthy_array(positional, named, "any", |values| {
        values.iter().any(Value::truthy)
    })
}

fn all(positional: Vec<Value>, named: NamedArguments) -> Result<Value> {
    truthy_array(positional, named, "all", |values| {
        values.iter().all(Value::truthy)
    })
}

fn len(positional: Vec<Value>, named: NamedArguments) -> Result<Value> {
    reject_named(&named)?;
    expect_len(&positional, 1)?;
    let len = match &positional[0] {
        Value::String(value) => value.chars().count(),
        Value::Array(value) => value.len(),
        Value::Object(value, _) => value.len(),
        _ => return Err(Error::Type("len expects a string, array, or object".into())),
    };
    Ok(Value::Number(len.into()))
}

fn keys(positional: Vec<Value>, named: NamedArguments) -> Result<Value> {
    reject_named(&named)?;
    expect_len(&positional, 1)?;
    let Value::Object(values, _) = &positional[0] else {
        return Err(Error::Type("keys expects an object".into()));
    };
    Ok(Value::Array(
        values.keys().cloned().map(Value::String).collect(),
    ))
}

fn values(positional: Vec<Value>, named: NamedArguments) -> Result<Value> {
    reject_named(&named)?;
    expect_len(&positional, 1)?;
    let Value::Object(values, _) = &positional[0] else {
        return Err(Error::Type("values expects an object".into()));
    };
    Ok(Value::Array(values.values().cloned().collect()))
}

fn pairs(positional: Vec<Value>, named: NamedArguments) -> Result<Value> {
    reject_named(&named)?;
    expect_len(&positional, 1)?;
    let Value::Object(values, _) = &positional[0] else {
        return Err(Error::Type("pairs expects an object".into()));
    };
    Ok(Value::Array(
        values
            .iter()
            .map(|(key, value)| Value::Array(vec![Value::String(key.clone()), value.clone()]))
            .collect(),
    ))
}

fn get(positional: Vec<Value>, named: NamedArguments) -> Result<Value> {
    reject_named(&named)?;
    if !(2..=3).contains(&positional.len()) {
        return Err(Error::Call(
            "get expects a collection, key, and optional default".into(),
        ));
    }
    let default = positional.get(2).cloned().unwrap_or(Value::Null);
    match (&positional[0], &positional[1]) {
        (Value::Object(values, _), Value::String(key)) => {
            Ok(values.get(key).cloned().unwrap_or(default))
        }
        (Value::Array(values), Value::Number(index)) => {
            Ok(array_index(values, index).cloned().unwrap_or(default))
        }
        _ => Err(Error::Type(
            "get expects an object and string key, or an array and integer index".into(),
        )),
    }
}

fn has(positional: Vec<Value>, named: NamedArguments) -> Result<Value> {
    reject_named(&named)?;
    expect_len(&positional, 2)?;
    match (&positional[0], &positional[1]) {
        (Value::Object(values, _), Value::String(key)) => Ok(Value::Bool(values.contains_key(key))),
        (Value::Array(values), Value::Number(index)) => {
            Ok(Value::Bool(array_index(values, index).is_some()))
        }
        _ => Err(Error::Type(
            "has expects an object and string key, or an array and integer index".into(),
        )),
    }
}

fn set(positional: Vec<Value>, named: NamedArguments) -> Result<Value> {
    reject_named(&named)?;
    expect_len(&positional, 3)?;
    let [Value::Object(values, tag), Value::String(key), value] = positional.as_slice() else {
        return Err(Error::Type(
            "set expects an object, string key, and value".into(),
        ));
    };
    let mut result = values.clone();
    result.insert(key.clone(), value.clone());
    Ok(Value::Object(result, tag.clone()))
}

fn remove(positional: Vec<Value>, named: NamedArguments) -> Result<Value> {
    reject_named(&named)?;
    expect_len(&positional, 2)?;
    let [Value::Object(values, tag), Value::String(key)] = positional.as_slice() else {
        return Err(Error::Type(
            "remove expects an object and string key".into(),
        ));
    };
    let mut result = values.clone();
    result.remove(key);
    Ok(Value::Object(result, tag.clone()))
}

fn merge(positional: Vec<Value>, named: NamedArguments) -> Result<Value> {
    reject_named(&named)?;
    expect_len(&positional, 2)?;
    let [Value::Object(left, left_tag), Value::Object(right, right_tag)] = positional.as_slice() else {
        return Err(Error::Type("merge expects two objects".into()));
    };
    let mut result = left.clone();
    result.extend(right.clone());
    Ok(Value::Object(result, left_tag.clone().or(right_tag.clone())))
}

fn contains(positional: Vec<Value>, named: NamedArguments) -> Result<Value> {
    reject_named(&named)?;
    expect_len(&positional, 2)?;
    let result = match (&positional[0], &positional[1]) {
        (Value::Array(values), expected) => values.iter().any(|value| value.equal(expected)),
        (Value::String(value), Value::String(expected)) => value.contains(expected),
        _ => {
            return Err(Error::Type(
                "contains expects an array and value, or two strings".into(),
            ));
        }
    };
    Ok(Value::Bool(result))
}

fn push(positional: Vec<Value>, named: NamedArguments) -> Result<Value> {
    reject_named(&named)?;
    expect_len(&positional, 2)?;
    let Value::Array(values) = &positional[0] else {
        return Err(Error::Type("push expects an array".into()));
    };
    let mut result = values.clone();
    result.push(positional[1].clone());
    Ok(Value::Array(result))
}

fn append(positional: Vec<Value>, named: NamedArguments) -> Result<Value> {
    reject_named(&named)?;
    expect_len(&positional, 1)?;
    let Value::Array(arrays) = &positional[0] else {
        return Err(Error::Type("append expects an array of arrays".into()));
    };
    let mut result = vec![];
    for array in arrays {
        let Value::Array(values) = array else {
            return Err(Error::Type("append expects an array of arrays".into()));
        };
        result.extend(values.clone());
    }
    Ok(Value::Array(result))
}

fn first(positional: Vec<Value>, named: NamedArguments) -> Result<Value> {
    reject_named(&named)?;
    expect_len(&positional, 1)?;
    match &positional[0] {
        Value::Array(values) => Ok(values.first().cloned().unwrap_or(Value::Null)),
        Value::String(value) => Ok(value
            .chars()
            .next()
            .map(|character| Value::String(character.to_string()))
            .unwrap_or(Value::Null)),
        _ => Err(Error::Type("first expects an array or string".into())),
    }
}

fn rest(positional: Vec<Value>, named: NamedArguments) -> Result<Value> {
    reject_named(&named)?;
    expect_len(&positional, 1)?;
    match &positional[0] {
        Value::Array(values) => Ok(Value::Array(values.get(1..).unwrap_or_default().to_vec())),
        Value::String(value) => Ok(Value::String(value.chars().skip(1).collect())),
        _ => Err(Error::Type("rest expects an array or string".into())),
    }
}

fn slice(positional: Vec<Value>, named: NamedArguments) -> Result<Value> {
    reject_named(&named)?;
    if !(2..=3).contains(&positional.len()) {
        return Err(Error::Call(
            "slice expects a collection, start, and optional stop".into(),
        ));
    }
    let start = integer(&positional[1])?;
    match &positional[0] {
        Value::Array(values) => {
            let range = slice_range(values.len(), start, positional.get(2))?;
            Ok(Value::Array(values[range].to_vec()))
        }
        Value::String(value) => {
            let characters: Vec<_> = value.chars().collect();
            let range = slice_range(characters.len(), start, positional.get(2))?;
            Ok(Value::String(characters[range].iter().collect()))
        }
        _ => Err(Error::Type("slice expects an array or string".into())),
    }
}

fn map(positional: Vec<Value>, named: NamedArguments) -> Result<Value> {
    reject_named(&named)?;
    expect_len(&positional, 2)?;
    let Value::Array(values) = &positional[0] else {
        return Err(Error::Type("map expects an array".into()));
    };
    let function = &positional[1];
    values
        .iter()
        .map(|value| call_value(function.clone(), vec![value.clone()], NamedArguments::new()))
        .collect::<Result<_>>()
        .map(Value::Array)
}

fn filter(positional: Vec<Value>, named: NamedArguments) -> Result<Value> {
    reject_named(&named)?;
    expect_len(&positional, 2)?;
    let Value::Array(values) = &positional[0] else {
        return Err(Error::Type("filter expects an array".into()));
    };
    let function = &positional[1];
    let mut result = vec![];
    for value in values {
        if call_value(function.clone(), vec![value.clone()], NamedArguments::new())?.truthy() {
            result.push(value.clone());
        }
    }
    Ok(Value::Array(result))
}

fn sort(positional: Vec<Value>, named: NamedArguments) -> Result<Value> {
    reject_named(&named)?;
    expect_len(&positional, 1)?;
    let Value::Array(values) = &positional[0] else {
        return Err(Error::Type("sort expects an array".into()));
    };
    let mut values = values.clone();
    let mut error = None;
    values.sort_by(|left, right| {
        if error.is_some() {
            return Ordering::Equal;
        }
        match compare(left, right) {
            Ok(ordering) => ordering,
            Err(compare_error) => {
                error = Some(compare_error);
                Ordering::Equal
            }
        }
    });
    if let Some(error) = error {
        return Err(error);
    }
    Ok(Value::Array(values))
}

fn compare(left: &Value, right: &Value) -> Result<Ordering> {
    match (left, right) {
        (Value::Null, Value::Null) => Ok(Ordering::Equal),
        (Value::Bool(left), Value::Bool(right)) => Ok(left.cmp(right)),
        (Value::Number(left), Value::Number(right)) => left
            .as_f64()
            .and_then(|left| right.as_f64().and_then(|right| left.partial_cmp(&right)))
            .ok_or_else(|| Error::Type("cannot compare invalid numbers".into())),
        (Value::String(left), Value::String(right)) => Ok(left.cmp(right)),
        _ => Err(Error::Type(
            "sort expects scalar values of a consistent type".into(),
        )),
    }
}

fn numeric_fold(
    positional: Vec<Value>,
    named: NamedArguments,
    name: &str,
    initial: f64,
    operation: impl Fn(f64, f64) -> f64,
) -> Result<Value> {
    reject_named(&named)?;
    expect_len(&positional, 1)?;
    let Value::Array(values) = &positional[0] else {
        return Err(Error::Type(format!("{name} expects an array")));
    };
    values
        .iter()
        .try_fold(initial, |total, value| Ok(operation(total, number(value)?)))
        .and_then(json_number)
}

fn extremum(
    positional: Vec<Value>,
    named: NamedArguments,
    name: &str,
    ordering: Ordering,
) -> Result<Value> {
    reject_named(&named)?;
    expect_len(&positional, 1)?;
    let Value::Array(values) = &positional[0] else {
        return Err(Error::Type(format!("{name} expects an array")));
    };
    let Some(mut result) = values.first().cloned() else {
        return Ok(Value::Null);
    };
    for value in &values[1..] {
        if compare(value, &result)? == ordering {
            result = value.clone();
        }
    }
    Ok(result)
}

fn truthy_array(
    positional: Vec<Value>,
    named: NamedArguments,
    name: &str,
    predicate: impl FnOnce(&[Value]) -> bool,
) -> Result<Value> {
    reject_named(&named)?;
    expect_len(&positional, 1)?;
    let Value::Array(values) = &positional[0] else {
        return Err(Error::Type(format!("{name} expects an array")));
    };
    Ok(Value::Bool(predicate(values)))
}

fn range(positional: Vec<Value>, named: NamedArguments) -> Result<Value> {
    reject_named(&named)?;
    let (mut current, stop, step) = match positional.as_slice() {
        [stop] => (0, integer(stop)?, 1),
        [start, stop] => (integer(start)?, integer(stop)?, 1),
        [start, stop, step] => (integer(start)?, integer(stop)?, integer(step)?),
        _ => {
            return Err(Error::Call(
                "range expects stop, start and stop, or start, stop, and step".into(),
            ));
        }
    };
    if step == 0 {
        return Err(Error::Type("range step cannot be zero".into()));
    }
    let mut values = vec![];
    while (step > 0 && current < stop) || (step < 0 && current > stop) {
        values.push(Value::Number(current.into()));
        current = current
            .checked_add(step)
            .ok_or_else(|| Error::Type("range overflowed".into()))?;
    }
    Ok(Value::Array(values))
}

fn reduce(positional: Vec<Value>, named: NamedArguments) -> Result<Value> {
    reject_named(&named)?;
    expect_len(&positional, 3)?;
    let Value::Array(values) = &positional[0] else {
        return Err(Error::Type("reduce expects an array".into()));
    };
    let function = &positional[1];
    let mut result = positional[2].clone();
    for value in values {
        result = call_value(
            function.clone(),
            vec![result, value.clone()],
            NamedArguments::new(),
        )?;
    }
    Ok(result)
}

fn join(positional: Vec<Value>, named: NamedArguments) -> Result<Value> {
    reject_named(&named)?;
    expect_len(&positional, 2)?;
    let Value::String(separator) = &positional[0] else {
        return Err(Error::Type("join expects a separator string".into()));
    };
    let Value::Array(values) = &positional[1] else {
        return Err(Error::Type("join expects an array of strings".into()));
    };
    let strings = values
        .iter()
        .map(|value| match value {
            Value::String(value) => Ok(value.as_str()),
            _ => Err(Error::Type("join expects an array of strings".into())),
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Value::String(strings.join(separator)))
}

fn split(positional: Vec<Value>, named: NamedArguments) -> Result<Value> {
    reject_named(&named)?;
    expect_len(&positional, 2)?;
    let Value::String(separator) = &positional[0] else {
        return Err(Error::Type("split expects a separator string".into()));
    };
    let Value::String(value) = &positional[1] else {
        return Err(Error::Type("split expects a string".into()));
    };
    Ok(Value::Array(
        value
            .split(separator)
            .map(|value| Value::String(value.to_owned()))
            .collect(),
    ))
}

fn lower(positional: Vec<Value>, named: NamedArguments) -> Result<Value> {
    string_transform(positional, named, "lower", str::to_lowercase)
}

fn upper(positional: Vec<Value>, named: NamedArguments) -> Result<Value> {
    string_transform(positional, named, "upper", str::to_uppercase)
}

fn starts_with(positional: Vec<Value>, named: NamedArguments) -> Result<Value> {
    string_predicate(positional, named, "starts_with", |value, pattern| {
        value.starts_with(pattern)
    })
}

fn ends_with(positional: Vec<Value>, named: NamedArguments) -> Result<Value> {
    string_predicate(positional, named, "ends_with", |value, pattern| {
        value.ends_with(pattern)
    })
}

fn replace(positional: Vec<Value>, named: NamedArguments) -> Result<Value> {
    reject_named(&named)?;
    expect_len(&positional, 3)?;
    let [Value::String(from), Value::String(to), Value::String(value)] = positional.as_slice()
    else {
        return Err(Error::Type("replace expects three strings".into()));
    };
    Ok(Value::String(value.replace(from, to)))
}

fn globals(
    environment: &Environment,
    positional: Vec<Value>,
    named: NamedArguments,
) -> Result<Value> {
    reject_named(&named)?;
    expect_len(&positional, 0)?;
    Ok(Value::Object(environment.globals(), Some("module".to_string())))
}

fn string_transform(
    positional: Vec<Value>,
    named: NamedArguments,
    name: &str,
    transform: impl FnOnce(&str) -> String,
) -> Result<Value> {
    reject_named(&named)?;
    expect_len(&positional, 1)?;
    let Value::String(value) = &positional[0] else {
        return Err(Error::Type(format!("{name} expects a string")));
    };
    Ok(Value::String(transform(value)))
}

fn string_predicate(
    positional: Vec<Value>,
    named: NamedArguments,
    name: &str,
    predicate: impl FnOnce(&str, &str) -> bool,
) -> Result<Value> {
    reject_named(&named)?;
    expect_len(&positional, 2)?;
    let [Value::String(pattern), Value::String(value)] = positional.as_slice() else {
        return Err(Error::Type(format!("{name} expects two strings")));
    };
    Ok(Value::Bool(predicate(value, pattern)))
}

fn integer(value: &Value) -> Result<i64> {
    let Value::Number(value) = value else {
        return Err(Error::Type("expected an integer".into()));
    };
    value
        .as_i64()
        .ok_or_else(|| Error::Type("expected an integer".into()))
}

fn number(value: &Value) -> Result<f64> {
    let Value::Number(value) = value else {
        return Err(Error::Type("expected a number".into()));
    };
    value
        .as_f64()
        .ok_or_else(|| Error::Type("invalid number".into()))
}

fn json_number(value: f64) -> Result<Value> {
    if value.fract() == 0.0 && value >= i64::MIN as f64 && value <= i64::MAX as f64 {
        return Ok(Value::Number((value as i64).into()));
    }
    serde_json::Number::from_f64(value)
        .map(Value::Number)
        .ok_or_else(|| Error::Type("number is not representable as JSON".into()))
}

fn slice_range(len: usize, start: i64, stop: Option<&Value>) -> Result<std::ops::Range<usize>> {
    let start = bounded_index(len, start);
    let stop = stop
        .map(integer)
        .transpose()?
        .map_or(len, |stop| bounded_index(len, stop));
    Ok(start.min(stop)..stop)
}

fn bounded_index(len: usize, index: i64) -> usize {
    if index < 0 {
        len.saturating_sub(index.unsigned_abs() as usize)
    } else {
        usize::try_from(index).unwrap_or(usize::MAX).min(len)
    }
}

fn load(environment: &Environment, positional: Vec<Value>, named: NamedArguments) -> Result<Value> {
    reject_named(&named)?;
    expect_len(&positional, 1)?;
    let Value::String(path) = &positional[0] else {
        return Err(Error::Type("load expects a path string".into()));
    };
    let context = || format!("while loading {path}");
    let source =
        std::fs::read_to_string(path).map_err(|error| Error::from(error).context(context()))?;
    let environment = environment.child_with_source(source.clone());
    let statements = parser::parse(&source).map_err(|error| error.context(context()))?;
    crate::eval::eval_statements(&statements, &environment)
        .map_err(|error| error.with_source(&source))
        .map_err(|error| error.context(context()))
}

fn array_index<'a>(values: &'a [Value], index: &serde_json::Number) -> Option<&'a Value> {
    let index = index.as_i64()?;
    let index = if index < 0 {
        values.len().checked_sub(index.unsigned_abs() as usize)?
    } else {
        usize::try_from(index).ok()?
    };
    values.get(index)
}

fn sqlite_open(positional: Vec<Value>, named: NamedArguments) -> Result<Value> {
    reject_named(&named)?;
    expect_len(&positional, 1)?;
    let Value::String(path) = &positional[0] else {
        return Err(Error::Type("sqlite_open expects a path string".into()));
    };
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true);
    let pool = runtime().block_on(
        sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options),
    )?;
    Ok(Value::Object(ValueMap::from([
        (
            "execute".into(),
            sqlite_database_method(&pool, sqlite_execute),
        ),
        ("query".into(), sqlite_database_method(&pool, sqlite_query)),
    ]), Some("module".to_string())))
}

fn sqlite_database_method(
    pool: &sqlx::SqlitePool,
    function: fn(Vec<Value>, NamedArguments) -> Result<Value>,
) -> Value {
    let pool = pool.clone();
    Value::native(move |mut positional, named| {
        positional.insert(0, Value::Database(pool.clone()));
        function(positional, named)
    })
}

fn sqlite_query(positional: Vec<Value>, named: NamedArguments) -> Result<Value> {
    reject_named(&named)?;
    if !(2..=3).contains(&positional.len()) {
        return Err(Error::Call(
            "sqlite_query expects database, SQL, and optional parameters".into(),
        ));
    }
    let (pool, sql) = database_and_sql(&positional)?;
    let mut query = sqlx::query(sql);
    if let Some(parameters) = positional.get(2) {
        query = bind_parameters(query, parameters)?;
    }
    let rows = runtime().block_on(query.fetch_all(pool))?;
    rows.iter()
        .map(row_to_value)
        .collect::<Result<_>>()
        .map(Value::Array)
}

fn sqlite_execute(positional: Vec<Value>, named: NamedArguments) -> Result<Value> {
    reject_named(&named)?;
    if !(2..=3).contains(&positional.len()) {
        return Err(Error::Call(
            "sqlite_execute expects database, SQL, and optional parameters".into(),
        ));
    }
    let (pool, sql) = database_and_sql(&positional)?;
    let mut query = sqlx::query(sql);
    if let Some(parameters) = positional.get(2) {
        query = bind_parameters(query, parameters)?;
    }
    let result = runtime().block_on(query.execute(pool))?;
    Ok(Value::Number(result.rows_affected().into()))
}

fn bind_parameters<'query>(
    mut query: sqlx::query::Query<'query, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'query>>,
    parameters: &'query Value,
) -> Result<sqlx::query::Query<'query, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'query>>> {
    let Value::Array(parameters) = parameters else {
        return Err(Error::Type("SQLite parameters must be an array".into()));
    };
    for parameter in parameters {
        query = match parameter {
            Value::Null => query.bind(Option::<String>::None),
            Value::Bool(value) => query.bind(*value),
            Value::Number(value) if value.is_i64() => query.bind(value.as_i64().expect("i64")),
            Value::Number(value) if value.is_u64() => {
                query.bind(i64::try_from(value.as_u64().expect("u64")).map_err(|_| {
                    Error::Type("SQLite integers must fit in a signed 64-bit value".into())
                })?)
            }
            Value::Number(value) => query.bind(value.as_f64().expect("f64")),
            Value::String(value) => query.bind(value),
            Value::Array(value) => query.bind(byte_array(value)?),
            _ => {
                return Err(Error::Type(
                    "SQLite parameters must be JSON scalars or byte arrays".into(),
                ));
            }
        }
    }
    Ok(query)
}

fn database_and_sql(positional: &[Value]) -> Result<(&sqlx::SqlitePool, &str)> {
    let Value::Database(pool) = &positional[0] else {
        return Err(Error::Type(
            "first argument must be a SQLite database".into(),
        ));
    };
    let Value::String(sql) = &positional[1] else {
        return Err(Error::Type("second argument must be a SQL string".into()));
    };
    Ok((pool, sql))
}

fn row_to_value(row: &sqlx::sqlite::SqliteRow) -> Result<Value> {
    let mut object = ValueMap::new();
    for (index, column) in row.columns().iter().enumerate() {
        let raw = row.try_get_raw(index)?;
        let value = if raw.is_null() {
            Value::Null
        } else {
            match raw.type_info().name() {
                "INTEGER" => Value::Number(row.try_get::<i64, _>(index)?.into()),
                "REAL" => serde_json::Number::from_f64(row.try_get(index)?)
                    .map(Value::Number)
                    .ok_or_else(|| Error::Type("SQLite returned a non-JSON number".into()))?,
                "TEXT" => Value::String(row.try_get(index)?),
                "BLOB" => Value::Array(
                    row.try_get::<Vec<u8>, _>(index)?
                        .into_iter()
                        .map(|byte| Value::Number(byte.into()))
                        .collect(),
                ),
                name => return Err(Error::Type(format!("unsupported SQLite type: {name}"))),
            }
        };
        object.insert(column.name().to_owned(), value);
    }
    Ok(Value::Object(object, None))
}

fn byte_array(values: &[Value]) -> Result<Vec<u8>> {
    values
        .iter()
        .map(|value| {
            let Value::Number(value) = value else {
                return Err(Error::Type(
                    "SQLite byte arrays must contain integers from 0 to 255".into(),
                ));
            };
            value
                .as_u64()
                .and_then(|value| u8::try_from(value).ok())
                .ok_or_else(|| {
                    Error::Type("SQLite byte arrays must contain integers from 0 to 255".into())
                })
        })
        .collect()
}

fn expect_len(positional: &[Value], expected: usize) -> Result<()> {
    if positional.len() == expected {
        Ok(())
    } else {
        Err(Error::Call(format!(
            "expected {expected} positional argument(s)"
        )))
    }
}

fn reject_named(named: &NamedArguments) -> Result<()> {
    if named.is_empty() {
        Ok(())
    } else {
        Err(Error::Call(
            "native function does not accept named arguments".into(),
        ))
    }
}
