# nano-codex

A small coding agent in Rust, built to teach harness engineering. Inspired by
[Codex](https://github.com/openai/codex) and the readable, working-implementation
approach of [nano-vllm](https://github.com/GeeeekExplorer/nano-vllm).

The model proposes actions. The harness owns the conversation, permissions,
execution, recovery, and stopping conditions. Start with [`src/agent.rs`](src/agent.rs):
it contains the entire agent loop. Each source file covers one concept.

## Try it

Requires a current stable Rust toolchain and macOS or Linux (`/bin/sh`).

```sh
git clone https://github.com/vedantjh2/nano-codex.git
cd nano-codex
make install
nano-codex
```

`make install` installs Rust if needed, builds this checkout, and installs
`~/.local/bin/nano-codex`. Add that directory to your PATH if the installer tells
you to. On first launch, setup offers OpenAI/ChatGPT, vLLM, or SGLang. For local
servers it asks for the port or API URL, lists served models, and saves your choice.
Run `nano-codex setup` to change the connection later.

For an offline demonstration:

```sh
cargo build --locked
python3 examples/demo.py
```

The demo uses a **scripted model**, a temporary workspace, and a real HTTP server.
It reads a buggy Python function, edits it, runs its tests, and returns an answer.
No GPU, account, API key, or Python packages are needed. This tests the harness;
it does not measure model intelligence.

## Connect a model

### OpenAI API account

```sh
export OPENAI_API_KEY='your-api-key'
nano-codex models
nano-codex --model gpt-6.1-sol
nano-codex exec 'Explain this repository'
```

API-key usage is billed to your OpenAI API account.

### Continue with ChatGPT

```sh
nano-codex login
# Uses the installed Codex CLI's browser sign-in in an isolated auth directory.
nano-codex login --device-auth     # Headless sign-in through Codex
nano-codex login --from-codex      # Link an existing file-backed Codex account
nano-codex login status
nano-codex models
nano-codex
nano-codex logout
```

The `codex` provider follows the actual Codex account transport: its account
endpoint, bearer token, account-ID header, model catalog, and streamed Responses.
The official Codex CLI manages browser/device sign-in and renewal of isolated
sessions. Nano-codex still makes its own model requests and executes its own tools.
Fresh sign-ins live under `~/.nano-codex/codex-login/`. Existing Codex credentials
are read without copying or rotating their refresh token. Logout disconnects a
shared login without signing Codex out. Keyring-only users can use the isolated
login path. This compatibility route was checked against Codex 0.163.0; it depends
on Codex's account API rather than the public API-key contract.

`nano-codex login --standalone` keeps the native educational OAuth implementation:
dynamic registration, PKCE, state, nonce, OIDC verification, and rotating refresh.
It uses the distinct `chatgpt` provider and public Responses API, subject to
[OpenAI's open-source sign-in availability](https://developers.openai.com/siwc/token-sharing-open-source/sign-in).
See [the authentication comparison](docs/authentication.md) for the exact
differences, source references, and credential ownership rules.

### vLLM

Run a tool-capable model with the corresponding parser and chat template:

```sh
vllm serve Qwen/Qwen2.5-Coder-7B-Instruct \
  --enable-auto-tool-choice --tool-call-parser hermes --port 8000

nano-codex --provider vllm models
nano-codex --provider vllm --model Qwen/Qwen2.5-Coder-7B-Instruct
```

### SGLang

```sh
python3 -m sglang.launch_server \
  --model-path Qwen/Qwen2.5-Coder-7B-Instruct \
  --tool-call-parser qwen25 --port 30000

nano-codex --provider sglang models
nano-codex --provider sglang --model Qwen/Qwen2.5-Coder-7B-Instruct
```

### Test vLLM on an Apple Silicon Mac

The [vLLM-Metal plugin](https://github.com/vllm-project/vllm-metal) provides native
Apple Silicon serving. After installing it with its official installer:

```sh
./scripts/serve-mac.sh
# Serves mlx-community/Qwen3-0.6B-4bit at http://127.0.0.1:18000/v1
```

In another terminal:

```sh
nano-codex setup                 # Choose vLLM, then port 18000
python3 scripts/smoke-local.py   # Live file-tool round trip against this server
```

The live smoke test downloads nothing itself and is separate from offline CI.
The server downloads and caches the small model on its first run. Override its
defaults with `NANO_CODEX_LOCAL_MODEL` and `NANO_CODEX_LOCAL_PORT`. Small models
demonstrate the protocol; they may be unreliable at larger coding tasks. The
initial Qwen2.5-Coder-1.5B smoke attempt emitted JSON as ordinary text instead of
structured calls, which the harness correctly left unexecuted.

Server flags and supported parsers depend on the server version and model. Follow
the [vLLM tool-calling](https://docs.vllm.ai/en/latest/features/tool_calling/) or
[SGLang tool-parser](https://docs.sglang.io/docs/advanced_features/tool_parser)
documentation for your deployment. The CLI needs structured tool calls, not tool
syntax embedded in ordinary text. Both local providers use `/v1/chat/completions`.

For a remote cluster, add `--base-url https://cluster.example/v1`. Set
`NANO_CODEX_API_KEY` if the cluster requires authentication. Local providers never
receive `OPENAI_API_KEY` or ChatGPT credentials.

## Everyday use

```sh
nano-codex -C ./project 'Find and fix the failing test'
nano-codex --approval read-only exec 'Review the implementation'
nano-codex --approval auto --json exec 'Fix the failing test' > events.jsonl
nano-codex resume SESSION_UUID 'Inspect the workspace and continue'
```

In the REPL: `/new`, `/status`, `/help`, `/exit`. Ctrl-C stops the CLI and terminates
the active shell's process group. Resume with the original provider, model, API
endpoint, and workspace. Session IDs are printed at startup; journals are stored
under `<workspace>/.nano-codex/sessions/`.

Approvals default to `ask`: file reads are allowed; every write, edit, or shell
command needs an explicit `y`. Without a terminal, those operations return denial
results to the model. `read-only` denies them; `auto` authorizes them. Shell commands
have your user permissions, including network and access outside the workspace.
File-tool path checks are guardrails; **there is no OS sandbox**. Use a disposable
container for untrusted repositories. Session logs can contain source code and
tool output; treat them as private.

## Configuration

Optional `~/.nano-codex/config.toml`:

```toml
provider = "vllm"
model = "Qwen/Qwen2.5-Coder-7B-Instruct"
base_url = "http://127.0.0.1:8000/v1"
approval = "ask"
max_steps = 40
context_bytes = 180000
tool_timeout = 30
```

CLI options override environment variables, which override this file and defaults.
`NANO_CODEX_PROVIDER`, `NANO_CODEX_MODEL`, and `NANO_CODEX_BASE_URL` configure the
connection; `NANO_CODEX_HOME` changes the config/auth directory. Secrets stay in
environment variables or the credential file, never in TOML.

`AGENTS.md` files are loaded from the project root through the starting directory,
with the more specific instructions last. Nested directories explored later do
not automatically change the instruction prefix; inspect their instructions before
editing. Tool results are bounded to about 16 KB, text files to 1 MiB, HTTP inference
to 180 seconds, and shell commands to `tool_timeout` seconds.

## Read and learn

| Concept | Read |
| --- | --- |
| Observe → act → observe, termination, step budgets | [agent.rs](src/agent.rs) |
| Messages, calls, and results | [protocol.rs](src/protocol.rs) |
| Schema validation, approvals, tool errors | [tools/mod.rs](src/tools/mod.rs) |
| Paths, exact edits, process cleanup | [tools/](src/tools/) |
| Wire formats, streaming, bounded retries | [provider/](src/provider/) |
| Instructions and whole-turn context compaction | [context.rs](src/context.rs) |
| Durable intent, replay, interrupted side effects | [session.rs](src/session.rs) |
| OAuth, OIDC, refresh, credential isolation | [auth/](src/auth/) |

[The learning guide](docs/harness-engineering.md) explains the invariants, tradeoffs,
interview questions, and extension exercises. [The design plan](docs/design.md)
records the planning subagent's priorities and the scope of this implementation.

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
```

Tests use loopback HTTP servers and temporary workspaces. They cover both local
provider adapters end to end and OpenAI streaming/replay contracts. Live account
and inference checks are opt-in and separate from the offline suite.

## Deliberate boundaries

This is an educational CLI, not full Codex feature parity. It has no TUI, MCP,
subagents, IDE integration, browser tools, or OS sandbox. Tools execute sequentially.
Context compaction keeps bounded excerpts of older user messages and final answers,
then retains the complete active task. It is lossy; old tool output remains only in
the journal. An oversized active task stops with a clear error rather than silently
dropping its tool results. The budget uses bytes, not a model-specific tokenizer.

Streaming and a stable prompt prefix demonstrate harness-level latency concepts.
Speculative decoding and KV-cache scheduling belong to the inference server; a
connected vLLM/SGLang cluster can use them without changing this harness. There is
no pretend decoder in this CLI. The guide includes concrete harness extensions
that teach concurrency, caching, and evaluation without obscuring the core loop.

Contributions go through pull requests. See [CONTRIBUTING.md](CONTRIBUTING.md)
for the review policy and development checks.
