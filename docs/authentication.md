# OpenAI authentication: three different contracts

The OpenAI SDK accepts a bearer credential; it does not turn a ChatGPT subscription
into an API key or perform the Codex browser login. Nano-codex keeps these paths
explicit so the credential reaches the endpoint that understands its grant.

| Provider | Credential owner | Inference endpoint |
| --- | --- | --- |
| `openai` | `OPENAI_API_KEY` | `https://api.openai.com/v1/responses` |
| `codex` | Official Codex account login | `https://chatgpt.com/backend-api/codex/responses` |
| `chatgpt` | Native open-source OAuth registration | `https://api.openai.com/v1/responses` |
| `vllm` / `sglang` | Optional `NANO_CODEX_API_KEY` | Configured `/v1/chat/completions` |

## Codex account support

`login` and `login --device-auth` invoke the installed Codex CLI with an isolated
`CODEX_HOME` and file-backed credential storage. This uses the real browser/device
flow without reusing Codex's OAuth client ID in a new login implementation.
The small bridge reads the resulting access token and account ID for nano-codex's
own HTTP requests. Near expiry, Codex's `account/read` RPC performs renewal in that
isolated home. Codex owns validation, rotation, and storage of these credentials.

`login --from-codex` instead records the location of an existing file-backed login.
It never copies the refresh token. It reloads access credentials for each request,
so renewal by the running Codex session becomes visible. If the shared token needs
renewal, the CLI asks you to open Codex or create an isolated nano-codex session.
It does not race the active Codex process by rotating its shared refresh token.
Keyring-only existing logins should use a fresh isolated sign-in.

Logout delegates to Codex only for nano-codex's isolated home. A linked shared
login is disconnected locally. Your original Codex login remains usable.

The account backend and `client_version` model query are compatibility details,
not a public API-key interface. The comparison uses
[Codex commit 89461ea](https://github.com/openai/codex/tree/89461ea6833832b0551ce24ee32205824d5c108d):

- [Provider selection and account endpoint](https://github.com/openai/codex/blob/89461ea6833832b0551ce24ee32205824d5c108d/codex-rs/model-provider-info/src/lib.rs)
- [Browser login](https://github.com/openai/codex/blob/89461ea6833832b0551ce24ee32205824d5c108d/codex-rs/login/src/server.rs)
- [Device login](https://github.com/openai/codex/blob/89461ea6833832b0551ce24ee32205824d5c108d/codex-rs/login/src/device_code_auth.rs)
- [Account API parameters](https://github.com/openai/codex/blob/89461ea6833832b0551ce24ee32205824d5c108d/codex-rs/app-server-protocol/src/protocol/v2/account.rs)

## Native OAuth example

`login --standalone` demonstrates authentication without a Codex dependency. It
implements the separately documented dynamic open-source registration flow,
including fresh state, nonce, PKCE, signed ID-token validation, granted-scope
checking, protected storage, and serialized refreshes. It uses its issued client
ID and the public API, not Codex's fixed client ID or account backend.
Availability depends on the account and OpenAI's rollout.

See OpenAI's [registration](https://developers.openai.com/siwc/token-sharing-open-source/sign-in)
and [inference contract](https://developers.openai.com/siwc/token-sharing-open-source/models-and-inference).

## Stream handling checked against the SDK

The Python SDK and Codex both retain finalized `response.output_item.done` events.
A successful terminal event may omit output items that have already arrived.
Nano-codex now retains those items, validates response/item identities, rejects
unfinished output, and preserves opaque reasoning state across tool calls.
It still requires a successful terminal event before executing a tool.

Sources: [OpenAI Python SDK response accumulation](https://github.com/openai/openai-python/blob/main/src/openai/lib/streaming/responses/_responses.py)
and [Codex stream parsing and fixtures](https://github.com/openai/codex/blob/89461ea6833832b0551ce24ee32205824d5c108d/codex-rs/codex-api/src/sse/responses.rs).
