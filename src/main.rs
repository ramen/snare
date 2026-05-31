use std::borrow::Cow;
use std::env;
use std::io::{self, IsTerminal, Read};

use rustyline::completion::Completer;
use rustyline::error::ReadlineError;
use rustyline::highlight::{CmdKind, Highlighter};
use rustyline::hint::Hinter;
use rustyline::history::DefaultHistory;
use rustyline::validate::Validator;
use rustyline::{Editor, Helper};
use snare::{Engine, Value, is_incomplete};

const RESET: &str = "\x1b[0m";
const PROMPT: &str = "\x1b[1;32m";
const KEYWORD: &str = "\x1b[1;34m";
const STRING: &str = "\x1b[33m";
const NUMBER: &str = "\x1b[36m";
const LITERAL: &str = "\x1b[35m";
const COMMENT: &str = "\x1b[2m";

struct ReplHelper;

impl Helper for ReplHelper {}

impl Completer for ReplHelper {
    type Candidate = String;
}

impl Hinter for ReplHelper {
    type Hint = String;
}

impl Validator for ReplHelper {}

impl Highlighter for ReplHelper {
    fn highlight<'line>(&self, line: &'line str, _position: usize) -> Cow<'line, str> {
        Cow::Owned(highlight(line))
    }

    fn highlight_prompt<'borrow, 'self_ref: 'borrow, 'prompt: 'borrow>(
        &'self_ref self,
        prompt: &'prompt str,
        _default: bool,
    ) -> Cow<'borrow, str> {
        Cow::Owned(format!("{PROMPT}{prompt}{RESET}"))
    }

    fn highlight_char(&self, _line: &str, _position: usize, kind: CmdKind) -> bool {
        kind != CmdKind::ForcedRefresh
    }
}

fn highlight(line: &str) -> String {
    let mut highlighted = String::with_capacity(line.len());
    let mut index = 0;
    while index < line.len() {
        let rest = &line[index..];
        if rest.starts_with("//") {
            push_colored(&mut highlighted, COMMENT, rest);
            break;
        }
        let byte = line.as_bytes()[index];
        if byte == b'"' {
            let end = if rest.starts_with(r#"""""#) {
                multiline_string_end(line, index)
            } else {
                string_end(line, index)
            };
            push_colored(&mut highlighted, STRING, &line[index..end]);
            index = end;
        } else if byte.is_ascii_digit()
            || (byte == b'-'
                && line
                    .as_bytes()
                    .get(index + 1)
                    .is_some_and(u8::is_ascii_digit))
        {
            let end = number_end(line, index);
            push_colored(&mut highlighted, NUMBER, &line[index..end]);
            index = end;
        } else if byte.is_ascii_alphabetic() || byte == b'_' {
            let end = identifier_end(line, index);
            let identifier = &line[index..end];
            match identifier {
                "let" | "fn" | "if" | "then" | "else" | "do" => {
                    push_colored(&mut highlighted, KEYWORD, identifier);
                }
                "true" | "false" | "null" => push_colored(&mut highlighted, LITERAL, identifier),
                _ => highlighted.push_str(identifier),
            }
            index = end;
        } else {
            let character = rest.chars().next().expect("character");
            highlighted.push(character);
            index += character.len_utf8();
        }
    }
    highlighted
}

fn multiline_string_end(line: &str, start: usize) -> usize {
    line[start + 3..]
        .find(r#"""""#)
        .map_or(line.len(), |offset| start + 3 + offset + 3)
}

fn string_end(line: &str, start: usize) -> usize {
    let bytes = line.as_bytes();
    let mut index = start + 1;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index += 2,
            b'"' => return index + 1,
            _ => index += 1,
        }
    }
    bytes.len()
}

fn number_end(line: &str, start: usize) -> usize {
    let bytes = line.as_bytes();
    let mut index = start;
    if bytes[index] == b'-' {
        index += 1;
    }
    while bytes.get(index).is_some_and(u8::is_ascii_digit) {
        index += 1;
    }
    if bytes.get(index) == Some(&b'.') {
        index += 1;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
    }
    if matches!(bytes.get(index), Some(b'e' | b'E')) {
        index += 1;
        if matches!(bytes.get(index), Some(b'+' | b'-')) {
            index += 1;
        }
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
    }
    index
}

fn identifier_end(line: &str, start: usize) -> usize {
    line.as_bytes()[start..]
        .iter()
        .position(|byte| !(byte.is_ascii_alphanumeric() || *byte == b'_'))
        .map_or(line.len(), |offset| start + offset)
}

fn push_colored(output: &mut String, color: &str, text: &str) {
    output.push_str(color);
    output.push_str(text);
    output.push_str(RESET);
}

fn with_trailing_semicolon(source: &str) -> String {
    if source.trim_end().ends_with(';') {
        source.to_owned()
    } else {
        format!("{source};")
    }
}

fn repl_output(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::Array(_) | Value::Object(_) => Some(value.to_pretty_string()),
        value => Some(value.to_string()),
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = Engine::new();
    if let Some(path) = env::args().nth(1) {
        let source = if path == "-" {
            let mut source = String::new();
            io::stdin().read_to_string(&mut source)?;
            Some(source)
        } else {
            None
        };
        let value = match source {
            Some(source) => engine.eval(&source)?,
            None => engine.eval_file(path)?,
        };
        if value.to_string() != "null" {
            println!("{value}");
        }
        return Ok(());
    }
    if !io::stdin().is_terminal() {
        let mut source = String::new();
        io::stdin().read_to_string(&mut source)?;
        println!("{}", engine.eval(&source)?);
        return Ok(());
    }
    println!("Snare 0.1.0");
    let mut editor = Editor::<ReplHelper, DefaultHistory>::new()?;
    editor.set_helper(Some(ReplHelper));
    loop {
        match editor.readline("snare> ") {
            Ok(mut source) => {
                while is_incomplete(&source) {
                    match editor.readline("   ... ") {
                        Ok(line) => {
                            source.push('\n');
                            source.push_str(&line);
                        }
                        Err(ReadlineError::Interrupted) => {
                            source.clear();
                            break;
                        }
                        Err(ReadlineError::Eof) => break,
                        Err(error) => return Err(error.into()),
                    }
                }
                if source.is_empty() {
                    continue;
                }
                let _ = editor.add_history_entry(&source);
                match engine.eval(&with_trailing_semicolon(&source)) {
                    Ok(value) => {
                        if let Some(output) = repl_output(&value) {
                            println!("{output}");
                        }
                    }
                    Err(error) => eprintln!("{error}"),
                }
            }
            Err(ReadlineError::Interrupted) => continue,
            Err(ReadlineError::Eof) => break,
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use snare::Value;

    use super::{COMMENT, KEYWORD, LITERAL, NUMBER, RESET, STRING, highlight, repl_output};

    #[test]
    fn highlights_repl_tokens() {
        assert_eq!(
            highlight(r#"let value = {"ok": true, "count": 1+2} // comment"#),
            format!(
                "{KEYWORD}let{RESET} value = {{{STRING}\"ok\"{RESET}: {LITERAL}true{RESET}, {STRING}\"count\"{RESET}: {NUMBER}1{RESET}+{NUMBER}2{RESET}}} {COMMENT}// comment{RESET}"
            )
        );
    }

    #[test]
    fn highlights_exponents_and_unicode_strings() {
        assert_eq!(
            highlight(r#""cafe \u2615" + -1.5e+2"#),
            format!("{STRING}\"cafe \\u2615\"{RESET} + {NUMBER}-1.5e+2{RESET}")
        );
    }

    #[test]
    fn highlights_multiline_strings() {
        assert_eq!(
            highlight("\"\"\"first\nsecond\"\"\""),
            format!("{STRING}\"\"\"first\nsecond\"\"\"{RESET}")
        );
    }

    #[test]
    fn repl_does_not_print_null_values() {
        assert_eq!(repl_output(&Value::Null), None);
        assert_eq!(repl_output(&Value::Number(42.into())), Some("42".into()));
    }

    #[test]
    fn repl_pretty_prints_arrays_and_objects() {
        let value = Value::from_json(serde_json::json!({
            "items": [1, {"ok": true}]
        }));
        assert_eq!(
            repl_output(&value),
            Some("{\n  \"items\": [\n    1,\n    {\n      \"ok\": true\n    }\n  ]\n}".into())
        );
    }

    #[test]
    fn repl_pretty_prints_objects_containing_functions() {
        let value = Value::Object(
            [
                ("items".into(), Value::Array(vec![Value::Number(1.into())])),
                ("native".into(), Value::native(|_, _| Ok(Value::Null))),
            ]
            .into(),
        );
        assert_eq!(
            repl_output(&value),
            Some("{\n  \"items\": [\n    1\n  ],\n  \"native\": <function>\n}".into())
        );
    }
}
