use std::env;
use std::fs;
use std::io::{self, IsTerminal, Read};

use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;
use snare::Engine;

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
            source
        } else {
            fs::read_to_string(path)?
        };
        let value = engine.eval(&source)?;
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
    let mut editor = DefaultEditor::new()?;
    loop {
        match editor.readline("snare> ") {
            Ok(line) => {
                let _ = editor.add_history_entry(&line);
                match engine.eval(&line) {
                    Ok(value) => println!("{value}"),
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
