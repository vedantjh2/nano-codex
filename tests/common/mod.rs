#![allow(dead_code)]
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::{Duration, Instant},
};

pub struct Request {
    pub path: String,
    pub body: Value,
    pub headers: String,
}
pub struct Server {
    pub url: String,
    thread: thread::JoinHandle<Vec<Request>>,
}

impl Server {
    pub fn start(responses: Vec<(u16, String)>) -> Self {
        Self::serve(responses, true)
    }

    pub fn without_content_type(responses: Vec<(u16, String)>) -> Self {
        Self::serve(responses, false)
    }

    fn serve(responses: Vec<(u16, String)>, content_type: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let thread = thread::spawn(move || {
            let mut requests = Vec::new();
            for (status, body) in responses {
                let deadline = Instant::now() + Duration::from_secs(10);
                let mut socket = loop {
                    if let Ok((socket, _)) = listener.accept() {
                        break socket;
                    }
                    assert!(Instant::now() < deadline, "Expected another HTTP request");
                    thread::sleep(Duration::from_millis(5));
                };
                socket.set_nonblocking(false).unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut header = Vec::new();
                let mut byte = [0];
                while !header.ends_with(b"\r\n\r\n") {
                    socket.read_exact(&mut byte).unwrap();
                    header.push(byte[0]);
                    assert!(header.len() < 32_768);
                }
                let headers = String::from_utf8(header).unwrap();
                let length = headers
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .map(|v| v.trim().parse::<usize>().unwrap())
                    })
                    .unwrap_or(0);
                let mut input = vec![0; length];
                socket.read_exact(&mut input).unwrap();
                let path = headers
                    .lines()
                    .next()
                    .unwrap()
                    .split_whitespace()
                    .nth(1)
                    .unwrap()
                    .into();
                let parsed = if input.is_empty() {
                    Value::Null
                } else {
                    serde_json::from_slice(&input).unwrap()
                };
                requests.push(Request {
                    path,
                    body: parsed,
                    headers,
                });
                let header = if content_type {
                    "Content-Type: text/event-stream\r\n"
                } else {
                    ""
                };
                let response = format!(
                    "HTTP/1.1 {status} Test\r\n{header}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = socket.write_all(response.as_bytes());
            }
            requests
        });
        Self { url, thread }
    }

    pub fn finish(self) -> Vec<Request> {
        self.thread.join().unwrap()
    }
}

pub fn event(value: Value) -> String {
    format!("data: {value}\n\n")
}

pub fn call(id: &str, name: &str, args: Value) -> String {
    let args = args.to_string();
    let half = args.len() / 2;
    event(
        json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":id,"function":{"name":name,"arguments":&args[..half]}}]},"finish_reason":null}]}),
    ) + &event(
        json!({"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":&args[half..]}}]},"finish_reason":"tool_calls"}]}),
    ) + "data: [DONE]\n\n"
}

pub fn answer(text: &str) -> String {
    event(json!({"choices":[{"delta":{"content":text},"finish_reason":"stop"}]}))
        + &event(json!({"choices":[],"usage":{"prompt_tokens":10,"completion_tokens":5}}))
        + "data: [DONE]\n\n"
}

pub fn config(root: &std::path::Path) -> nano_codex::config::Config {
    use nano_codex::config::{Approval, Config, ProviderKind};
    Config {
        provider: ProviderKind::Vllm,
        model: "test-model".into(),
        base_url: "http://127.0.0.1:8000/v1".into(),
        cwd: root.canonicalize().unwrap(),
        approval: Approval::Auto,
        max_steps: 10,
        context_bytes: 20_000,
        tool_timeout: 2,
    }
}
