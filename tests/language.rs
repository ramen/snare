use snare::{Engine, is_incomplete};

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
    engine.eval(r#"let db = sqlite_open(":memory:");"#).unwrap();
    engine
        .eval(r#"sqlite_execute(db, "create table notes (id integer, body text)");"#)
        .unwrap();
    engine
        .eval(r#"sqlite_execute(db, "insert into notes values (?, ?)", [1, "hello"]);"#)
        .unwrap();
    assert_eq!(
        engine
            .eval(r#"sqlite_query(db, "select * from notes")"#)
            .unwrap()
            .to_string(),
        r#"[{"body":"hello","id":1}]"#
    );
}
