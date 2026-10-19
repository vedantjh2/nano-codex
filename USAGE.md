# Running nano-codex

For the harness concepts and code reading order, start with [README.md](README.md).
This guide covers installation, terminal use, and model connections. Account
authentication is covered at the end.

## Install and launch

Use macOS or Linux with `/bin/sh`, `make`, and `git`:

```sh
git clone https://github.com/vedantjh2/nano-codex.git
cd nano-codex
make install
nano-codex
```

`make install` installs stable Rust if needed, builds this checkout, and installs
`~/.local/bin/nano-codex`. Add that directory to your PATH if prompted. Run the same
command after updating the checkout to rebuild and reinstall.

First launch offers OpenAI/ChatGPT, vLLM, or SGLang. For a local server, enter its
port or API URL, choose a served model, and enter the REPL. The server must already
be running. For account connections, see [OpenAI account access](#openai-account-access).
Run `nano-codex setup` to change the connection later.

## Offline walkthrough

From the checkout, with Rust installed and Python 3.11+ available:

```sh
make build
python3 examples/demo.py
```

No account, model download, GPU, or third-party Python packages are needed. The
demo starts a scripted HTTP model server, runs the real CLI in a temporary
workspace, reads and fixes a buggy addition function, then runs its tests.
Expect tool-call/result events, `exit=0`, and `Demo passed`. The fixture is removed
afterward. This demonstrates the harness; the responses are scripted.

## Connected terminal use

After choosing a connection:

```sh
nano-codex
nano-codex -C ./project 'Find and fix the failing test'
nano-codex --approval read-only exec 'Review the implementation'
nano-codex --approval auto --json exec 'Fix the failing test' > events.jsonl
nano-codex resume SESSION_UUID 'Inspect the workspace and continue'
```

`exec` runs one task and exits. The REPL supports `/new`, `/status`, `/help`, and
`/exit`. Ctrl-C stops the CLI and terminates the active shell's process group.
The CLI prints a session UUID; journals live in
`<workspace>/.nano-codex/sessions/`. Resume using the original workspace, provider,
model, and API endpoint. Interrupted calls are marked uncertain; inspect their
effects before retrying them.

### Approvals and execution

| Policy | Behavior |
| --- | --- |
| `ask` (default) | Reads are allowed. Each write, edit, or shell command requires an explicit `y` in a terminal. |
| `read-only` | Write, edit, and shell calls return denial results. |
| `auto` | Write, edit, and shell calls are authorized without a prompt. |

Without a terminal, `ask` denies operations that need approval. Shell commands
have your user permissions, network access, and access outside the workspace.
**There is no OS sandbox.** Use a disposable container for untrusted repositories.
Journals and JSONL events can contain source code and command output; keep them
private and out of commits.

## Connect a local inference server

Start a tool-capable model in vLLM or SGLang, then run `nano-codex setup` and select
the matching provider. The server needs the tool parser and chat template for
that model. Both adapters use `/v1/chat/completions` and require structured tool
calls; JSON or tool syntax printed as ordinary assistant text is not executed.

| Provider | Default API root | Server configuration reference |
| --- | --- | --- |
| vLLM | `http://127.0.0.1:8000/v1` | [Tool calling](https://docs.vllm.ai/en/latest/features/tool_calling/) |
| SGLang | `http://127.0.0.1:30000/v1` | [Tool parsers](https://docs.sglang.io/docs/advanced_features/tool_parser) |

To inspect a server without saving settings:

```sh
nano-codex --provider vllm --base-url http://127.0.0.1:8000/v1 models
nano-codex --provider sglang --base-url http://127.0.0.1:30000/v1 models
```

Use the server's exact served model name with `--model`, or select it through
setup. For remote clusters, supply their API root with `--base-url`. If a cluster
requires a key, set `NANO_CODEX_API_KEY`; local providers do not receive OpenAI
account credentials or `OPENAI_API_KEY`.

### Apple Silicon with vLLM-Metal

Install the [vLLM-Metal plugin](https://github.com/vllm-project/vllm-metal) using its
official instructions. The launcher expects its default `~/.venv-vllm-metal`
environment; override that path with `VLLM_METAL_HOME` if needed.

```sh
./scripts/serve-mac.sh
# Serves mlx-community/Qwen3-0.6B-4bit at http://127.0.0.1:18000/v1
```

In another terminal, from this checkout:

```sh
nano-codex setup                # Choose vLLM, then port 18000
python3 scripts/smoke-local.py  # Live file-tool round trip against this server
```

The server downloads and caches the model on its first run. Override its model
and port with `NANO_CODEX_LOCAL_MODEL` and `NANO_CODEX_LOCAL_PORT`. The smoke test is
separate from offline CI and does not download a model itself. Small models can
demonstrate tool use while remaining unreliable at larger coding tasks.

## Configuration and limits

Optional `~/.nano-codex/config.toml`:

```toml
provider = "vllm"
model = "your-served-model-name"
base_url = "http://127.0.0.1:8000/v1"
approval = "ask"
max_steps = 40
context_bytes = 180000
tool_timeout = 30
```

CLI options override environment variables, which override saved settings and
defaults. `NANO_CODEX_PROVIDER`, `NANO_CODEX_MODEL`, and `NANO_CODEX_BASE_URL` select
the connection. `NANO_CODEX_HOME` changes the settings/auth directory. Store secrets
in environment variables or credential storage, never in TOML.

Project `AGENTS.md` instructions load from the repository root through the starting
directory, with more specific instructions last. Exploring another directory does
not automatically reload nested instructions. Tool output is bounded to about
16 KB, text files to 1 MiB, inference requests to 180 seconds, and shell runtime to
`tool_timeout`. Context is budgeted in serialized bytes; increasing it does not
increase the model's context window.

If a local connection reports a missing model, list its catalog or repeat setup.
If a resume fails, check the original workspace/provider/model/endpoint. For
contribution checks and reproducible test evidence, see [CONTRIBUTING.md](CONTRIBUTING.md).

## OpenAI account access

These connections are supporting infrastructure. You can study and run the
offline harness walkthrough without them.

### Codex account sign-in

Install the official Codex CLI before using browser or device sign-in:

```sh
nano-codex login                 # Browser sign-in through Codex
nano-codex login --device-auth   # Headless device sign-in through Codex
nano-codex login --from-codex    # Link an existing file-backed Codex account
nano-codex login status
nano-codex models
nano-codex
nano-codex logout
```

The `codex` provider makes its own inference requests using Codex's account API.
The official CLI manages sign-in and renewal for isolated sessions under
`~/.nano-codex/codex-login/`. Linked existing credentials remain read-only; logout
disconnects nano-codex without signing the shared Codex account out. Keyring-only
accounts can use isolated sign-in. Compatibility was checked against Codex
0.163.0 and depends on its account API.

### OpenAI API key

```sh
export OPENAI_API_KEY='your-api-key'
nano-codex setup
# Choose OpenAI/ChatGPT, then the API-key option and a tool-capable Responses model.
```

API-key usage is billed to your OpenAI API account. This uses the `openai` provider
and the public Responses API.

For the independent educational OAuth example, use `nano-codex login --standalone`.
It uses the separate `chatgpt` provider and is subject to sign-in availability.
See [the authentication notes](docs/authentication.md) for the contracts, credential
ownership, and source references.
