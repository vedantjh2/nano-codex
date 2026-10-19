//! Terminal rendering is an observer; it never decides which tools may run.
use anyhow::Result;
use serde_json::{Value, json};
use std::io::{self, IsTerminal, Write};
use tokio::io::{AsyncBufReadExt, BufReader, Stdin};

pub struct Ui {
    pub json: bool,
    pub terminal: bool,
    input: BufReader<Stdin>,
}

impl Ui {
    pub fn new(json: bool) -> Self {
        Self {
            json,
            terminal: io::stdin().is_terminal(),
            input: BufReader::new(tokio::io::stdin()),
        }
    }

    pub fn event(&self, kind: &str, value: Value) {
        if self.json {
            println!("{}", json!({"event":kind,"data":value}));
        } else if let Some(text) = value.as_str() {
            eprintln!("[{kind}] {}", visible(text));
        } else {
            eprintln!("[{kind}] {value}");
        }
    }

    pub fn delta(&self, text: &str) {
        if self.json {
            self.event("text_delta", json!(text));
        } else {
            print!("{}", visible(text));
            let _ = io::stdout().flush();
        }
    }

    pub fn end_text(&self) {
        if !self.json {
            println!();
            let _ = io::stdout().flush();
        }
    }

    pub async fn line(&mut self, prompt: &str) -> Result<Option<String>> {
        eprint!("{prompt}");
        io::stderr().flush()?;
        let mut line = String::new();
        if self.input.read_line(&mut line).await? == 0 {
            return Ok(None);
        }
        Ok(Some(line.trim_end().to_owned()))
    }
}

// Models and tool output are untrusted terminal text. Escape OSC/ANSI/control bytes.
pub fn visible(text: &str) -> String {
    text.chars()
        .flat_map(|c| {
            if c.is_control() && c != '\n' && c != '\t' {
                c.escape_default().collect::<Vec<_>>()
            } else {
                vec![c]
            }
        })
        .collect()
}
