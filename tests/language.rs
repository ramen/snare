use std::cell::Cell;
use std::fs;
use std::rc::Rc;

use snare::{Engine, Module, Value, is_incomplete};
use tempfile::tempdir;

fn eval(source: &str) -> String {
    Engine::new().eval(source).unwrap().to_string()
}

#[test]
fn json_is_the_literal_core() {
    assert_eq!(
        eval(r#"{"name": "Snare", "values": [null, true, 3]}"#),
        r#"{"name":"Snare","values":[null,true,3]}"#
    );
}

#[test]
fn operators_have_familiar_precedence() {
    assert_eq!(eval("1 + 2 * 3 == 7 && !false"), "true");
}

#[test]
fn functions_support_defaults_and_named_arguments() {
    assert_eq!(
        eval(
            "let greet = fn(name, punctuation = \"!\") => name + punctuation; greet(punctuation = \"?\", name = \"hello\")"
        ),
        r#""hello?""#
    );
}

#[test]
fn do_expressions_introduce_a_local_scope() {
    assert_eq!(eval("let x = 1; do { let x = 2; x + 3 }"), "5");
}

#[test]
fn assignment_updates_an_existing_binding() {
    assert_eq!(eval("let x = 1; x = x + 2; x"), "3");
    assert_eq!(eval("let x = 1; do { x = 4; null }; x"), "4");
}

#[test]
fn assignment_does_not_implicitly_declare_names() {
    assert!(Engine::new().eval("x = 1;").is_err());
}

#[test]
fn incomplete_input_can_be_continued_by_the_repl() {
    assert!(is_incomplete("let value = {"));
    assert!(is_incomplete("1 +"));
    assert!(!is_incomplete("1 + 2"));
    assert!(!is_incomplete("let = 1;"));
}

#[test]
fn sqlite_can_be_explored_from_the_language() {
    let mut engine = Engine::new();
    engine.eval(r#"let db = sqlite.open(":memory:");"#).unwrap();
    engine
        .eval(r#"sqlite.execute(db, "create table notes (id integer, body text)");"#)
        .unwrap();
    engine
        .eval(r#"sqlite.execute(db, "insert into notes values (?, ?)", [1, "hello"]);"#)
        .unwrap();
    assert_eq!(
        engine
            .eval(r#"sqlite.query(db, "select * from notes")"#)
            .unwrap()
            .to_string(),
        r#"[{"body":"hello","id":1}]"#
    );
}

#[test]
fn sqlite_supports_runtime_types_and_json_byte_arrays() {
    let mut engine = Engine::new();
    engine.eval(r#"let db = sqlite.open(":memory:");"#).unwrap();
    assert_eq!(
        engine
            .eval(r#"sqlite.query(db, "select ? as value", ["hello"])"#)
            .unwrap()
            .to_string(),
        r#"[{"value":"hello"}]"#
    );
    engine
        .eval(r#"sqlite.execute(db, "create table files (payload blob)");"#)
        .unwrap();
    engine
        .eval(r#"sqlite.execute(db, "insert into files values (?)", [[0, 127, 255]]);"#)
        .unwrap();
    assert_eq!(
        engine
            .eval(r#"sqlite.query(db, "select payload from files")"#)
            .unwrap()
            .to_string(),
        r#"[{"payload":[0,127,255]}]"#
    );
}

#[test]
fn host_functions_can_capture_state_and_receive_named_arguments() {
    let calls = Rc::new(Cell::new(0));
    let host_calls = calls.clone();
    let mut engine = Engine::new();
    engine.register_fn("greet", move |positional, mut named| {
        host_calls.set(host_calls.get() + 1);
        let name = named.remove("name").unwrap();
        assert!(positional.is_empty());
        assert!(named.is_empty());
        let Value::String(name) = name else {
            panic!("name should be a string");
        };
        Ok(Value::String(format!("hello, {name}")))
    });

    assert_eq!(
        engine.eval(r#"greet(name = "Snare")"#).unwrap().to_string(),
        r#""hello, Snare""#
    );
    assert_eq!(calls.get(), 1);
}

#[test]
fn modules_namespace_host_values_and_functions() {
    let mut module = Module::new();
    module
        .set("answer", Value::Number(42.into()))
        .register_fn("double", |positional, named| {
            assert!(named.is_empty());
            let [Value::Number(value)] = positional.as_slice() else {
                panic!("expected one number");
            };
            Ok(Value::Number((value.as_i64().unwrap() * 2).into()))
        });
    let mut engine = Engine::new();
    engine.register_module("host", module);

    assert_eq!(
        engine
            .eval("host.answer + host.double(4)")
            .unwrap()
            .to_string(),
        "50"
    );
}

#[test]
fn sqlite_is_available_as_a_module() {
    let mut engine = Engine::new();
    engine.eval(r#"let db = sqlite.open(":memory:");"#).unwrap();
    assert_eq!(
        engine
            .eval(r#"sqlite.query(db, "select 42 as answer")"#)
            .unwrap()
            .to_string(),
        r#"[{"answer":42}]"#
    );
}

#[test]
fn collection_helpers_work_with_json_values() {
    assert_eq!(eval(r#"keys({"b": 2, "a": 1})"#), r#"["a","b"]"#);
    assert_eq!(eval(r#"values({"b": 2, "a": 1})"#), "[1,2]");
    assert_eq!(eval(r#"pairs({"b": 2, "a": 1})"#), r#"[["a",1],["b",2]]"#);
    assert_eq!(eval(r#"get({"answer": 42}, "answer")"#), "42");
    assert_eq!(eval(r#"get({}, "missing", "default")"#), r#""default""#);
    assert_eq!(eval("get([1, 2, 3], -1)"), "3");
    assert_eq!(eval(r#"has({"answer": 42}, "answer")"#), "true");
    assert_eq!(eval("push([1, 2], 3)"), "[1,2,3]");
}

#[test]
fn collection_callbacks_can_be_snare_functions() {
    assert_eq!(eval("map([1, 2, 3], fn(value) => value * 2)"), "[2,4,6]");
    assert_eq!(
        eval("filter([1, 2, 3, 4], fn(value) => value % 2 == 0)"),
        "[2,4]"
    );
    assert_eq!(
        eval("reduce([1, 2, 3, 4], fn(total, value) => total + value, 0)"),
        "10"
    );
}

#[test]
fn range_generates_integer_arrays() {
    assert_eq!(eval("range(4)"), "[0,1,2,3]");
    assert_eq!(eval("range(2, 5)"), "[2,3,4]");
    assert_eq!(eval("range(5, 0, -2)"), "[5,3,1]");
}

#[test]
fn strings_and_arrays_have_sequence_helpers() {
    assert_eq!(eval("first([1, 2, 3])"), "1");
    assert_eq!(eval("rest([1, 2, 3])"), "[2,3]");
    assert_eq!(eval("slice([1, 2, 3, 4], 1, -1)"), "[2,3]");
    assert_eq!(eval(r#"first("cafe")"#), r#""c""#);
    assert_eq!(eval(r#"rest("cafe")"#), r#""afe""#);
    assert_eq!(eval(r#"slice("cafe", 1, 3)"#), r#""af""#);
}

#[test]
fn string_helpers_are_unicode_aware() {
    assert_eq!(eval(r#"len("cafe\u2615")"#), "5");
    assert_eq!(eval(r#"first("\u2615tea")"#), r#""☕""#);
    assert_eq!(eval(r#"slice("a\u2615b", 1, 2)"#), r#""☕""#);
}

#[test]
fn strings_can_be_split_and_joined() {
    assert_eq!(eval(r#"split(",", "a,b,c")"#), r#"["a","b","c"]"#);
    assert_eq!(eval(r#"join("-", ["a", "b", "c"])"#), r#""a-b-c""#);
}

#[test]
fn strings_have_common_transformations_and_predicates() {
    assert_eq!(eval(r#"lower("Hello")"#), r#""hello""#);
    assert_eq!(eval(r#"upper("Hello")"#), r#""HELLO""#);
    assert_eq!(eval(r#"starts_with("Sn", "Snare")"#), "true");
    assert_eq!(eval(r#"ends_with("are", "Snare")"#), "true");
    assert_eq!(
        eval(r#"replace("world", "Snare", "hello, world")"#),
        r#""hello, Snare""#
    );
}

#[test]
fn globals_exposes_the_live_root_namespace() {
    let mut engine = Engine::new();
    assert_eq!(
        engine
            .eval(r#"has(globals(), "globals")"#)
            .unwrap()
            .to_string(),
        "true"
    );
    assert_eq!(
        engine
            .eval(r#"keys(globals().sqlite)"#)
            .unwrap()
            .to_string(),
        r#"["execute","open","query"]"#
    );
    assert_eq!(
        engine
            .eval(r#"filter(keys(globals()), fn(name) => starts_with("sqlite", name))"#)
            .unwrap()
            .to_string(),
        r#"["sqlite"]"#
    );
    engine.eval("let discovered = 42;").unwrap();
    assert_eq!(
        engine
            .eval(r#"get(globals(), "discovered")"#)
            .unwrap()
            .to_string(),
        "42"
    );
}

#[test]
fn collection_helpers_make_sqlite_rows_easy_to_explore() {
    let mut engine = Engine::new();
    engine.eval(r#"let db = sqlite.open(":memory:");"#).unwrap();
    assert_eq!(
        engine
            .eval(
                r#"map(sqlite.query(db, "select 1 as id union all select 2 as id"), fn(row) => row.id)"#
            )
            .unwrap()
            .to_string(),
        "[1,2]"
    );
}

#[test]
fn files_can_export_first_class_modules() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("math.snare");
    fs::write(
        &path,
        r#"
        let secret = 40;
        {
            "answer": secret + 2,
            "double": fn(value) => value * 2
        }
        "#,
    )
    .unwrap();
    let path = serde_json::to_string(&path.to_string_lossy()).unwrap();

    assert_eq!(
        eval(&format!(
            r#"let math = load({path}); math.answer + math.double(4)"#
        )),
        "50"
    );
}

#[test]
fn file_errors_include_source_context() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("broken.snare");
    fs::write(&path, "missing_name").unwrap();

    let error = Engine::new()
        .eval_file(&path)
        .err()
        .expect("broken module should fail")
        .to_string();
    assert!(error.contains(&format!("while evaluating {}", path.display())));
    assert!(error.contains("name error: unknown name: missing_name"));

    let missing = directory.path().join("missing.snare");
    let source = format!(
        "load({})",
        serde_json::to_string(&missing.to_string_lossy()).unwrap()
    );
    let error = Engine::new()
        .eval(&source)
        .err()
        .expect("missing module should fail")
        .to_string();
    assert!(error.contains(&format!("while loading {}", missing.display())));
    assert!(error.contains("I/O error"));
}

#[test]
fn runtime_errors_include_expression_locations() {
    let error = Engine::new()
        .eval("let answer = 42;\nanswer + missing")
        .err()
        .expect("missing name should fail")
        .to_string();
    assert!(error.contains("name error: unknown name: missing"));
    assert!(error.contains(" --> 2:10"));
    assert!(error.contains("2 | answer + missing"));
    assert!(error.contains("  |          ^^^^^^^"));
}

#[test]
fn loaded_module_errors_point_into_the_module_source() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("broken.snare");
    fs::write(&path, "{\n  \"answer\": missing\n}").unwrap();
    let source = format!(
        "load({})",
        serde_json::to_string(&path.to_string_lossy()).unwrap()
    );

    let error = Engine::new()
        .eval(&source)
        .err()
        .expect("broken module should fail")
        .to_string();
    assert!(error.contains(&format!("while loading {}", path.display())));
    assert!(error.contains(" --> 2:13"));
    assert!(error.contains("2 |   \"answer\": missing"));
}

#[test]
fn function_errors_include_call_frames() {
    let error = Engine::new()
        .eval("let broken = fn() => missing;\nbroken()")
        .err()
        .expect("broken function should fail")
        .to_string();
    assert!(error.contains(" --> 1:22"));
    assert!(error.contains("1 | let broken = fn() => missing;"));
    assert!(error.contains(" called from 2:1"));
    assert!(error.contains("2 | broken()"));
}

#[test]
fn loaded_functions_keep_their_module_source_for_diagnostics() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("broken.snare");
    fs::write(&path, "{\n  \"broken\": fn() => missing\n}").unwrap();
    let source = format!(
        "let module = load({});\nmodule.broken()",
        serde_json::to_string(&path.to_string_lossy()).unwrap()
    );

    let error = Engine::new()
        .eval(&source)
        .err()
        .expect("broken exported function should fail")
        .to_string();
    assert!(error.contains(" --> 2:21"));
    assert!(error.contains(r#"2 |   "broken": fn() => missing"#));
    assert!(error.contains(" called from 2:1"));
    assert!(error.contains("2 | module.broken()"));
}
