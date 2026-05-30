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

JSON-shaped values have a small collection toolkit:

```snare
keys({"b": 2, "a": 1})                         // ["a", "b"]
pairs({"a": 1})                                // [["a", 1]]
get({"answer": 42}, "answer")                  // 42
get([1, 2, 3], -1)                             // 3
push([1, 2], 3)                                // [1, 2, 3]
set({"a": 1}, "b", 2)                          // {"a": 1, "b": 2}
remove({"a": 1, "b": 2}, "a")                  // {"b": 2}
merge({"a": 1}, {"b": 2})                      // {"a": 1, "b": 2}
contains([1, 2, 3], 2)                         // true
sort([3, 1, 2])                                // [1, 2, 3]
sum([1, 2, 3])                                 // 6
product([2, 3, 4])                             // 24
min([3, 1, 2])                                 // 1
max([3, 1, 2])                                 // 3
any([null, 0, "yes"])                          // true
all([1, true, "yes"])                          // true
type_of({"answer": 42})                        // "object"
is_type([1, 2, 3], "array")                    // true
string(42)                                     // "42"
number("42.5")                                 // 42.5
bool([])                                       // false
map([1, 2, 3], fn(value) => value * 2)         // [2, 4, 6]
filter([1, 2, 3, 4], fn(value) => value > 2)   // [3, 4]
range(1, 6, 2)                                 // [1, 3, 5]
reduce([1, 2, 3], fn(total, value) => total + value, 0) // 6
first([1, 2, 3])                               // 1
rest([1, 2, 3])                                // [2, 3]
slice([1, 2, 3, 4], 1, -1)                     // [2, 3]
split(",", "a,b,c")                            // ["a", "b", "c"]
join("-", ["a", "b", "c"])                     // "a-b-c"
lower("Hello")                                 // "hello"
starts_with("Sn", "Snare")                     // true
replace("world", "Snare", "hello, world")      // "hello, Snare"
```

`print` writes strings without JSON quotes or escapes and separates multiple
arguments with spaces:

```snare
print("hello", "world")                        // prints: hello world
```

The global namespace is inspectable from the REPL:

```snare
keys(globals())
keys(globals().sqlite)                         // ["execute", "open", "query"]
```

SQLite is intentionally present in the first milestone:

```snare
let db = sqlite.open("notes.db");
sqlite.execute(db, "create table if not exists notes (body text)");
sqlite.execute(db, "insert into notes values (?)", ["hello"]);
sqlite.query(db, "select rowid, body from notes");
```

SQLite `NULL`, integer, real, text, and BLOB values map to JSON-shaped Snare
values. BLOBs use byte arrays, so they can be inspected and passed back to
SQLite without adding a non-JSON literal type:

```snare
sqlite.execute(db, "insert into files values (?)", [[0, 127, 255]]);
sqlite.query(db, "select payload from files");
// [{"payload": [0, 127, 255]}]
```

## Embed it

```rust
let mut engine = snare::Engine::new();
let value = engine.eval(r#"{"answer": 6 * 7}"#)?;
println!("{value}");
```

Host applications can read values defined by scripts:

```rust
engine.eval(r#"let answer = {"value": 42};"#)?;
let answer = engine.get("answer").expect("answer");
let globals = engine.globals();
```

Host applications can register Rust closures as Snare functions. The callback
receives positional and named arguments separately and may capture application
state:

```rust
engine.register_fn("greet", |_positional, mut named| {
    let name = named.remove("name").expect("name");
    let snare::Value::String(name) = name else {
        return Err(snare::Error::Type("name must be a string".into()));
    };
    Ok(snare::Value::String(format!("Hello, {name}!")))
});

engine.eval(r#"greet(name = "world")"#)?;
```

Host applications can also expose namespaced modules:

```rust
let mut app = snare::Module::new();
app.set("version", snare::Value::String("1.0".into()));
app.register_fn("greet", |_positional, _named| {
    Ok(snare::Value::String("Hello!".into()))
});
engine.register_module("app", app);

engine.eval("app.greet()")?;
```

Snare files can expose modules too. A module file returns an object:

```snare
// math.snare
{
  "double": fn(value) => value * 2
}
```

Load it from a script or the REPL:

```snare
let math = load("math.snare");
math.double(21)
```

The current implementation is a deliberately small kernel. File and module
errors include their source path, runtime errors point to the failing
expression, and function errors include call frames. Useful next language work
includes a broader standard library.
