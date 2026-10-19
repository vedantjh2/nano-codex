# Harness engineering, through nano-codex

Read `agent.rs` first. Follow one tool call outward through the modules, then come
back to the loop. The harness is the program around the model: it decides what the
model sees, which actions can happen, what gets remembered, and when work stops.

## 1. State machine and termination

An iteration requests a completion. A completion contains text, calls, or both.
Calls are observations-to-be: they do nothing until the harness validates and
executes them. Their results become the next request's input. A completed response
without calls ends the task. Reaching `max_steps` is an explicit stop, not success.

**Read:** `agent.rs`, `protocol.rs`.

**Interview:** Why is one model request insufficient for a coding agent? What
prevents an infinite tool loop? Does a model saying “done” establish correctness?

## 2. Tool contracts and authority

Schemas describe the model-facing interface. Rust deserialization checks actual
arguments. The configured approval policy decides authority; the model's text does
not. Errors and denials become tool results, allowing the model to correct an
argument or explain the blocker without crashing the session.

`edit_file` requires one exact match. Zero matches usually means stale context;
multiple matches mean ambiguous intent. Both are rejected. A tool error can occur
after partial side effects, so a failed shell command is not proof that nothing
changed.

**Read:** `tools/mod.rs`, `tools/files.rs`, `tools/paths.rs`.

**Interview:** Why are a JSON schema, an approval prompt, and a sandbox three
different mechanisms? Why must denial be checked outside the model? What are the
limits of canonicalizing paths when another process can change symlinks?

## 3. Streaming is a protocol, not just printing

Network chunks can end in the middle of a UTF-8 character, JSON string, or SSE
event. SSE events can contain several data lines. Chat tool calls can interleave
by index, and each argument arrives as string fragments.

The CLI displays text immediately but executes calls only after a successful
terminal event. Chat requires its finish reason and `[DONE]`; Responses requires
`response.completed`. A disconnect, failed event, or length stop discards the
partial completion. There is no inference retry after a stream starts.

**Read:** `provider/sse.rs`, `provider/chat.rs`, `provider/responses.rs`.

**Interview:** What breaks if each TCP chunk is parsed as JSON? When can you safely
execute a streamed tool call? Why might retrying a partially displayed response
produce duplicate text or repeated billing?

## 4. Reasoning state and provider boundaries

Both protocols express tool use, but their wire contracts differ. Responses emits
typed items and correlates calls with `call_id`. Chat uses assistant `tool_calls`
and tool messages with `tool_call_id`. Responses reasoning items are replayed
unchanged; local `reasoning_content` is retained when provided.

The loop handles `Message` and `ToolCall`. Only adapters construct provider JSON.
Keeping adapters separate prevents a convenient common interface from silently
discarding state that one provider needs.

**Interview:** What belongs in a provider abstraction? Which data should remain
opaque? Why can dropping reasoning items break later tool turns?

## 5. Durable intent and uncertain side effects

The journal records a completed assistant message before any of its tools run.
Each result is persisted after execution. On resume, unanswered calls receive an
interruption result. They are never automatically executed.

There is an unavoidable crash window between a tool's external effect and recording
its result. A shell command might have committed a change even though no result
was saved. This design prefers an honest “inspect before retrying” over claiming
exactly-once execution. A lock prevents two processes from writing one session.
Only an incomplete final journal record is discarded after a crash.

**Read:** `session.rs`; then the resume tests in `tests/state.rs`.

**Interview:** How would you make a payment tool idempotent? Why isn't a filesystem
journal sufficient to guarantee exactly-once shell execution? How do you recover
from a crash after the first of several requested tools?

## 6. Context is a limited resource

Instructions form a stable prefix. The serialized-byte budget includes instructions,
tool definitions, transcript, and a small allowance for wire overhead. It is not
an exact token count. Whole old user turns can be replaced by bounded excerpts;
the active task and its call/result groups remain intact.

This trades recall for simplicity. Older command output is still in the journal
but no longer in the model's context. If the active task itself is too large, the
agent stops. Raising the local budget does not raise the model's context window.

**Read:** `context.rs`.

**Interview:** What facts must survive summarization? Why can dropping one tool
result invalidate an otherwise sensible context? How would you evaluate compaction
quality, or implement exact token accounting across different models?

## 7. Authentication and secrets

An API key and a ChatGPT subscription token represent different grants. Local
servers have a third credential path. A provider never falls back to sending an
OpenAI secret to an arbitrary cluster URL.

Codex account compatibility delegates sign-in and isolated-session renewal to
the official Codex CLI. Linked existing sessions remain read-only. The separate
native OAuth example below shows how the protocol works without that dependency.

OAuth uses a random state to bind the callback, PKCE to bind the code exchange,
and a nonce to bind the ID token. OIDC signature, issuer, audience, expiration,
and subject checks precede trusting identity. Scope checks establish permission
to use the plan. A process lock serializes refresh-token rotation.

**Read:** `auth/codex.rs`, `auth/codex_source.rs`, `auth/codex_rpc.rs`, then
`auth/oauth.rs`, `auth/tokens.rs`, `auth/store.rs`.

**Interview:** Why isn't base64-decoding a JWT enough? What race occurs if two
processes refresh one rotating token? What happens if refresh succeeds remotely
and the process crashes before saving the new token?

## 8. Execution lifetime and budgets

Shell stdout and stderr are drained concurrently, with only a bounded prefix kept.
Reading one pipe to completion before the other can deadlock. A new process group
allows cancellation and timeout to kill ordinary descendants as well as the shell.
A child that deliberately detaches needs stronger OS isolation.

Budgets bound model steps, context, output, request duration, and shell runtime.
They make runaway behavior visible and recoverable. They do not establish that a
task is correct: the model still needs to run appropriate checks and report their
results.

**Read:** `tools/shell.rs`, `config.rs`.

**Interview:** Why can killing only the immediate child leak work? Which resources
would you meter in a hosted agent? How should cancellation interact with approval
prompts and operations that cannot be rolled back?

## 9. Performance and evaluation

Streaming lowers perceived latency; it does not make unsafe partial calls valid.
A stable instruction/tool prefix gives an inference server an opportunity to reuse
cached prefill. The harness does not claim a cache hit or own a KV cache. Speculative
decoding belongs behind the inference API in vLLM/SGLang.

For a concurrency exercise, batch consecutive independent reads with a small
worker limit. Keep mutations ordered and persist each result. Measure total task
latency, time to first text, tool runtime, model steps, input/output tokens, and
verified task success. Faster final text without a passing task is not an improvement.

The fake-server tests establish harness behavior, not coding intelligence. Build
a model evaluation set separately: tiny repositories, a task prompt, hidden checks,
a clean starting state, and a reproducible budget. Retain failed traces; categorize
model reasoning failures separately from harness protocol or execution failures.
