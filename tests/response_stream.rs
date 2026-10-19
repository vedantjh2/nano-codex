mod common;
use common::{Server, event};
use nano_codex::{provider::responses, ui::Ui};
use serde_json::{Value, json};

async fn consume(events: Vec<Value>) -> anyhow::Result<nano_codex::protocol::Completion> {
    let server = Server::start(vec![(200, events.into_iter().map(event).collect())]);
    let response = reqwest::get(&server.url).await.unwrap();
    let result = responses::consume(response, &Ui::new(true)).await;
    server.finish();
    result
}

fn item(id: &str) -> Value {
    json!({"type":"function_call","id":id,"call_id":"call1","name":"read_file","arguments":"{\"path\":\"x\"}"})
}

#[tokio::test]
async fn codex_terminal_envelopes_can_omit_output_and_status() {
    let completion = consume(vec![
        json!({"type":"response.created","response":{"id":"response1"}}),
        json!({"type":"response.output_item.done","output_index":0,"item":{"id":"reason1","type":"reasoning","encrypted_content":"opaque","summary":[]}}),
        json!({"type":"response.output_item.added","output_index":1,"item":item("function1")}),
        json!({"type":"response.output_item.done","output_index":1,"item":item("function1")}),
        json!({"type":"response.completed","response":{"id":"response1","usage":{"input_tokens":10}}}),
    ]).await.unwrap();
    assert_eq!(completion.calls[0].name, "read_file");
    assert_eq!(completion.items[0]["encrypted_content"], "opaque");
    assert_eq!(completion.usage["input_tokens"], 10);
}

#[tokio::test]
async fn codex_empty_final_output_does_not_erase_finalized_items() {
    let completion = consume(vec![
        json!({"type":"response.output_item.done","output_index":0,"item":item("function1")}),
        json!({"type":"response.completed","response":{"status":"completed","output":[]}}),
    ])
    .await
    .unwrap();
    assert_eq!(completion.calls.len(), 1);
}

#[tokio::test]
async fn unfinished_items_and_conflicting_response_identities_are_rejected() {
    let cases = [
        vec![
            json!({"type":"response.output_item.added","output_index":0,"item":item("function1")}),
            json!({"type":"response.completed","response":{"id":"response1"}}),
        ],
        vec![
            json!({"type":"response.created","response":{"id":"response1"}}),
            json!({"type":"response.completed","response":{"id":"response2","output":[item("function1")]}}),
        ],
        vec![
            json!({"type":"response.output_item.added","output_index":0,"item":item("function1")}),
            json!({"type":"response.output_item.done","output_index":0,"item":item("function2")}),
        ],
        vec![
            json!({"type":"response.output_item.done","output_index":0,"item":item("function1")}),
            json!({"type":"response.completed","response":{"status":"completed","output":[item("function2")]}}),
        ],
    ];
    for events in cases {
        assert!(consume(events).await.is_err());
    }
}
