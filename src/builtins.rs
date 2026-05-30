use std::collections::BTreeMap;
use std::sync::OnceLock;

use sqlx::{Column, Row, TypeInfo, ValueRef};
use tokio::runtime::Runtime;

use crate::eval::Environment;
use crate::value::Value;
use crate::{Error, Module, Result};

pub fn install(environment: &Environment) {
    environment.set("print", Value::native(print));
    environment.set("len", Value::native(len));

    let open = Value::native(sqlite_open);
    let query = Value::native(sqlite_query);
    let execute = Value::native(sqlite_execute);
    let mut sqlite = Module::new();
    sqlite
        .set("open", open.clone())
        .set("query", query.clone())
        .set("execute", execute.clone());
    environment.set("sqlite", sqlite.into());

    environment.set("sqlite_open", open);
    environment.set("sqlite_query", query);
    environment.set("sqlite_execute", execute);
}

fn runtime() -> &'static Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| Runtime::new().expect("tokio runtime"))
}

fn print(positional: Vec<Value>, named: BTreeMap<String, Value>) -> Result<Value> {
    reject_named(&named)?;
    expect_len(&positional, 1)?;
    println!("{}", positional[0]);
    Ok(Value::Null)
}

fn len(positional: Vec<Value>, named: BTreeMap<String, Value>) -> Result<Value> {
    reject_named(&named)?;
    expect_len(&positional, 1)?;
    let len = match &positional[0] {
        Value::String(value) => value.len(),
        Value::Array(value) => value.len(),
        Value::Object(value) => value.len(),
        _ => return Err(Error::Type("len expects a string, array, or object".into())),
    };
    Ok(Value::Number(len.into()))
}

fn sqlite_open(positional: Vec<Value>, named: BTreeMap<String, Value>) -> Result<Value> {
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
    Ok(Value::Database(pool))
}

fn sqlite_query(positional: Vec<Value>, named: BTreeMap<String, Value>) -> Result<Value> {
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

fn sqlite_execute(positional: Vec<Value>, named: BTreeMap<String, Value>) -> Result<Value> {
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
    let mut object = BTreeMap::new();
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
    Ok(Value::Object(object))
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

fn reject_named(named: &BTreeMap<String, Value>) -> Result<()> {
    if named.is_empty() {
        Ok(())
    } else {
        Err(Error::Call(
            "native function does not accept named arguments".into(),
        ))
    }
}
