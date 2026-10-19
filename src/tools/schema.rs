//! Function schemas are the model's user interface. Keep them aligned with execution.
use serde_json::{Value, json};

fn function(name: &str, description: &str, properties: Value, required: &[&str]) -> Value {
    json!({
        "type":"function", "name":name, "description":description, "strict":false,
        "parameters":{"type":"object","properties":properties,"required":required,"additionalProperties":false}
    })
}

pub fn definitions() -> Vec<Value> {
    let string = json!({"type":"string"});
    vec![
        function(
            "list_files",
            "List one directory, relative to the workspace.",
            json!({"path":string}),
            &["path"],
        ),
        function(
            "read_file",
            "Read UTF-8 text with line numbers. offset is zero-based; lines defaults to 200.",
            json!({
                "path":string,
                "offset":{"type":"integer","minimum":0,"default":0},
                "lines":{"type":"integer","minimum":1,"maximum":1000,"default":200}
            }),
            &["path"],
        ),
        function(
            "search",
            "Search workspace text for a literal string, honoring .gitignore. Output is bounded.",
            json!({"query":string}),
            &["query"],
        ),
        function(
            "write_file",
            "Create a new UTF-8 file. Refuses to overwrite an existing file. Parents must exist.",
            json!({"path":string,"content":string}),
            &["path", "content"],
        ),
        function(
            "edit_file",
            "Replace exactly one occurrence of old_text in an existing file. Read the file first.",
            json!({"path":string,"old_text":string,"new_text":string}),
            &["path", "old_text", "new_text"],
        ),
        function(
            "shell",
            "Execute a shell command from the workspace. Requires approval; has full user permissions, network access, and no OS sandbox. Output and runtime are bounded.",
            json!({"command":string}),
            &["command"],
        ),
    ]
}
