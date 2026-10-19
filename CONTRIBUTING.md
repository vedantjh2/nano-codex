# Contributing

Keep nano-codex small enough for a human to study. Read [AGENTS.md](AGENTS.md)
for the implementation conventions and [the learning guide](docs/harness-engineering.md)
for the concepts this project teaches.

1. Fork the repository and create a branch for one focused change.
2. Explain the behavior being changed and keep the Rust files concise.
3. Complete the testing requirements below on the final commit.
4. Fill [the PR template](.github/pull_request_template.md), validate the description,
   and open a pull request against `main`.

Automated checks use fake servers and temporary workspaces. Do not commit account
credentials or session traces. Live model tests are optional and explicit.

## Required before opening a PR

These requirements apply to humans and coding agents. Agents must read this guide
before editing, perform the checks themselves, and inspect their output. Do not
open a ready-for-review PR with promised tests, invented output, or unchecked
template items. If execution is blocked, report the blocker and keep the work local
or in a draft; do not claim that it passed.

Run this baseline from the repository root:

```sh
make check
cargo build --locked
python3 tests/first_run.py
python3 examples/demo.py
python3 -m unittest discover -s tests -p 'test_*.py'
```

The demo is an end-to-end test: the real CLI talks HTTP to a scripted server, reads
and edits a temporary file, runs its tests, and returns a final answer. The first-run
test drives a real terminal through connection setup. Neither needs a real account.

Also exercise the behavior your PR changes through its public entry point. Include
one successful scenario and one relevant failure or boundary case. For example,
an edit-tool change must demonstrate both a successful edit and an ambiguous or
missing match; an auth change must demonstrate valid and rejected credentials using
a fake server. A unit test alone does not establish that the full user flow works.
Add a focused automated regression test when fixing a behavioral bug. For docs or
workflow changes, run the baseline and exercise the changed commands or automation.

In the **PR description itself**, provide all of the following:

- The full tested commit SHA, OS, Rust/Python versions, and relevant server/model
  versions. Describe the scenario and fixtures, including temporary setup.
- Exact commands you actually ran, expected and observed results, and the failure
  case's outcome. Include a short redacted output excerpt or a directly linked CI
  run; "tests pass" and unchecked assertions are insufficient.
- Copy-paste commands to reproduce from a clean checkout, including dependencies,
  fixture creation, configuration, and the expected observable outcome. Private
  files, your account, and your local machine state must not be prerequisites.
- Honest limitations: anything not exercised, platform coverage, or optional live
  provider checks. Never put tokens or private traces into the PR.

Commit the change, rerun verification, then record `git rev-parse HEAD`. Every code
push requires checking the new commit and updating the description. Save your PR
description to a file and validate it before publishing:

```sh
python3 scripts/check-pr-evidence.py --body-file /tmp/pr.md --commit "$(git rev-parse HEAD)"
gh pr create --body-file /tmp/pr.md
```

The required **PR evidence** check rejects missing sections, placeholders, missing
commands/results, unchecked attestations, and evidence for an older commit. It reruns
when the description changes or commits are pushed. This checks the evidence's
structure; the owner still reviews whether the scenario and results are convincing.
The workflow reads PR text as data using trusted base-branch code and never runs
commands from the description or executes the contributor's code.

## Review and merging

Every change to `main` must go through a pull request, including changes by the
owner. Direct pushes, force pushes, and deletion of the protected branches are
blocked. Linux CI, macOS CI, and PR evidence must pass, and review conversations
must be resolved. The owner also has to meet these testing requirements.

[Vedant Jhaveri (@vedantjh2)](https://github.com/vedantjh2) is the code owner for
every file. Contributions require his approval, and new commits dismiss previous
approvals. He can merge pull requests, including his own, without another person's
approval; the PR and CI requirements still apply.

Two active GitHub rulesets enforce this policy. The first has no bypass actors;
the second grants only `vedantjh2` a pull-request-only bypass for reviews. Their
definitions are in [.github/rulesets](.github/rulesets). They cover the default
branch plus `main` and `master`. Editing these JSON files does not change GitHub's
settings automatically: an administrator must apply the corresponding ruleset
through the repository's **Settings → Rules → Rulesets** page or GitHub's API.
