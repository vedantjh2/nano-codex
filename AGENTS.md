# Working on nano-codex

This is a learning project for human readers. Prefer direct, ordinary Rust over
frameworks, generic abstractions, or compressed code.

- Keep one concept per source file. Aim for fewer than 200 readable lines; split
  along conceptual boundaries when a file grows. Do not squeeze lines to meet a cap.
- Keep the complete agent loop visible in `src/agent.rs`.
- Explain invariants and tradeoffs in comments; avoid narrating obvious syntax.
- Preserve call/result pairs and opaque provider reasoning state.
- Persist tool intent before executing side effects. Never automatically replay an
  operation whose outcome is unknown after interruption.
- Keep permission policy outside model control. Do not call path checks a sandbox.
- Use temporary workspaces and fake HTTP servers for automated checks. Live model
  calls should be explicit and must not be needed by the offline suite.
- Keep tokens, auth files, and user traces out of source control.
- Verify changes with `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
  and the relevant tests. Keep the offline demo working.
