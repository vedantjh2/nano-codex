mod common;
use common::{Server, event};
use nano_codex::{
    protocol::{Message, ToolCall},
    provider::{chat, responses, sse::Decoder},
    ui::Ui,
};
use serde_json::json;

#[test]
fn sse_decodes_crlf_multiline_comments_and_split_utf8() {
    let input = ":keepalive\r\ndata: hé\r\ndata: llo\r\n\r\ndata: next\n\n";
    let mut decoder = Decoder::default();
    let mut events = Vec::new();
    for byte in input.as_bytes() {
        events.extend(decoder.feed(&[*byte]).unwrap());
    }
    assert_eq!(events, ["hé\nllo", "next"]);
}

#[test]
fn interleaved_chat_calls_and_reasoning_survive_replay() {
    let ui = Ui::new(true);
    let mut assembly = chat::Assembly::default();
    assembly
        .push(
            &json!({"choices":[{"delta":{"reasoning_content":"reason", "tool_calls":[
                {"index":1,"id":"b","function":{"name":"read_file","arguments":"{\"path\":"}},
                {"index":0,"id":"a","function":{"name":"list_files","arguments":"{}"}}
            ]}}]}),
            &ui,
        )
        .unwrap();
    assembly.push(&json!({"choices":[{"delta":{"tool_calls":[{"index":1,"function":{"arguments":"\"x\"}"}}]},"finish_reason":"tool_calls"}]}), &ui).unwrap();
    let completion = assembly.finish().unwrap();
    assert_eq!(completion.calls[0].id, "a");
    assert_eq!(completion.calls[1].arguments, "{\"path\":\"x\"}");
    let request = chat::request("model", "instructions", &[completion.message()]);
    assert_eq!(request["messages"][1]["reasoning_content"], "reason");
}

#[test]
fn truncated_and_duplicate_calls_are_rejected() {
    let ui = Ui::new(true);
    let mut assembly = chat::Assembly::default();
    assert!(
        assembly
            .push(
                &json!({"choices":[{"delta":{},"finish_reason":"length"}]}),
                &ui
            )
            .is_err()
    );
    assert!(chat::Assembly::default().finish().is_err());
    let completion = nano_codex::protocol::Completion {
        calls: vec![
            ToolCall {
                id: "same".into(),
                name: "x".into(),
                arguments: "{}".into()
            };
            2
        ],
        ..Default::default()
    };
    assert!(completion.validate().is_err());
}

#[test]
fn responses_preserves_reasoning_and_matches_function_outputs() {
    let raw = json!({"status":"completed","output":[
        {"type":"reasoning","id":"r1","summary":[],"encrypted_content":"opaque"},
        {"type":"function_call","id":"f1","call_id":"c1","name":"read_file","arguments":"{\"path\":\"x\"}"}
    ]});
    let completion = responses::completed(&raw).unwrap();
    let request = responses::request(
        "model",
        "instructions",
        &[
            completion.message(),
            Message::Tool {
                call_id: "c1".into(),
                content: "file contents".into(),
            },
        ],
        true,
    );
    assert_eq!(request["input"][0]["encrypted_content"], "opaque");
    assert_eq!(request["input"][2]["call_id"], "c1");
    assert_eq!(request["tools"][0]["type"], "namespace");
    assert_eq!(request["store"], false);
    assert!(request.get("max_output_tokens").is_none());
    assert!(request.get("previous_response_id").is_none());
}

#[tokio::test]
async fn responses_requires_a_completed_terminal_event() {
    for terminal in [
        "response.failed",
        "response.incomplete",
        "disconnected",
        "response.completed",
    ] {
        let mut body = event(json!({"type":"response.output_text.delta","delta":"hello"}));
        if terminal != "disconnected" {
            body += &event(
                json!({"type":terminal,"response":{"status":"completed","output":[
                    {"type":"message","content":[{"type":"output_text","text":"hello"}]}
                ]}}),
            );
        }
        let server = Server::start(vec![(200, body)]);
        let response = reqwest::get(&server.url).await.unwrap();
        let result = responses::consume(response, &Ui::new(true)).await;
        assert_eq!(result.is_ok(), terminal == "response.completed");
        server.finish();
    }
}
