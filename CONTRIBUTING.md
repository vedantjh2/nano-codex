# Contributing

Keep nano-codex small enough for a human to study. Read [AGENTS.md](AGENTS.md)
for the implementation conventions and [the learning guide](docs/harness-engineering.md)
for the concepts this project teaches.

1. Fork the repository and create a branch for one focused change.
2. Explain the behavior being changed and keep the Rust files concise.
3. Run `make check`, `cargo build --locked`, `python3 tests/first_run.py`,
   and `python3 examples/demo.py`.
4. Open a pull request against `main`, including the reason and verification.

Automated checks use fake servers and temporary workspaces. Do not commit account
credentials or session traces. Live model tests are optional and explicit.

## Review and merging

Every change to `main` must go through a pull request, including changes by the
owner. Direct pushes, force pushes, and deletion of the protected branches are
blocked. Both Linux and macOS CI must pass, and review conversations must be resolved.

[Vedant Jhaveri (@vedantjh2)](https://github.com/vedantjh2) is the code owner for
every file. Contributions require his approval, and new commits dismiss previous
approvals. He can merge pull requests, including his
own, without another person's approval; the PR and CI requirements still apply.

Two active GitHub rulesets enforce this policy. The first has no bypass actors;
the second grants only `vedantjh2` a pull-request-only bypass for reviews. Their
definitions are in [.github/rulesets](.github/rulesets). They cover the default
branch plus `main` and `master`. Editing these JSON files does not change GitHub's
settings automatically: an administrator must apply the corresponding ruleset
through the repository's **Settings → Rules → Rulesets** page or GitHub's API.
