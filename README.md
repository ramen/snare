# Snare

Snare is an experimental embeddable scripting language with JSON at its core.
JSON literals retain JSON syntax exactly: object keys are always quoted. Around
that core, Snare adds bindings, functions, named and optional parameters,
familiar infix operators, a REPL, and an early SQLite bridge.

```snare
let greet = fn(name, punctuation = "!") => {
  "message": "Hello, " + name + punctuation
};

greet(name = "world")
```

## Run it

```sh
cargo run
cargo run -- examples/sqlite.snare
echo '1 + 2 * 3' | cargo run --quiet
```

## Language sketch

```snare
null
true
42
"text"
[1, 2, 3]
{"quoted": "keys only"}

let double = fn(value) => value * 2;
double(21)

let count = 1;
count = count + 1;

if len([1, 2]) > 0 then "yes" else "no"
do { let x = 2; x * 3 }
```

SQLite is intentionally present in the first milestone:

```snare
let db = sqlite_open("notes.db");
sqlite_execute(db, "create table if not exists notes (body text)");
sqlite_execute(db, "insert into notes values (?)", ["hello"]);
sqlite_query(db, "select rowid, body from notes");
```

## Embed it

```rust
let mut engine = snare::Engine::new();
let value = engine.eval(r#"{"answer": 6 * 7}"#)?;
println!("{value}");
```

The current implementation is a deliberately small kernel. Useful next
language work includes first-class host function registration, richer SQLite
values, and a module system.
