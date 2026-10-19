"""Validate PR evidence as text. Never execute contributor-provided commands."""
import argparse
import json
from pathlib import Path
import re
import sys

HEADINGS = ("Change", "Tested commit", "End-to-end test", "Reproduction", "Limitations", "Checklist")
ATTESTATIONS = (
    "I ran the checks on the tested commit and inspected their output.",
    "I exercised the changed behavior end to end, including a failure case.",
    "The reproduction steps need no private files or credentials.",
)


def sections(body):
    """Ignore comments and headings inside fenced command/output blocks."""
    body = re.sub(r"<!--.*?-->", "", body, flags=re.S)
    found, current, fence = {}, None, None
    for line in body.splitlines():
        marker = re.match(r"^\s*(`{3,}|~{3,})", line)
        inside = fence is not None
        if marker:
            token = marker[1]
            if fence is None:
                fence = token
            elif token[0] == fence[0] and len(token) >= len(fence):
                fence = None
        heading = re.fullmatch(r"##\s+(.+?)\s*", line) if not inside and not marker else None
        if heading:
            current = heading[1]
            if current in found:
                raise ValueError(f"Duplicate section: {current}")
            found[current] = []
        elif current:
            found[current].append(line)
    if fence:
        raise ValueError("Close all fenced code blocks")
    return {name: "\n".join(lines).strip() for name, lines in found.items()}


def filled(text):
    return bool(text.strip()) and not re.search(r"\b(TODO|TBD)\b|<fill[^>]*>", text, re.I)


def command_block(text):
    return any(filled(block) for block in re.findall(r"```(?:sh|bash|shell)\n(.*?)\n```", text, re.S))


def validate(body, commit):
    parts = sections(body)
    for heading in HEADINGS:
        if not filled(parts.get(heading, "")):
            raise ValueError(f"Fill the {heading!r} section without placeholders")
    tested = parts["Tested commit"].strip().strip("`")
    if not re.fullmatch(r"[0-9a-f]{40}", tested) or tested != commit:
        raise ValueError("Tested commit must equal the full current PR head SHA; rerun checks after pushes")
    e2e = parts["End-to-end test"]
    for label in ("Environment", "Scenario", "Expected result", "Actual result", "Failure case"):
        field = re.search(rf"^{label}:[ \t]*(\S[^\n]*)$", e2e, re.M)
        if not field or field[1].strip().lower() in {"n/a", "none", "not run"}:
            raise ValueError(f"Provide a concrete {label!r} in End-to-end test")
    for heading in ("End-to-end test", "Reproduction"):
        if not command_block(parts[heading]):
            raise ValueError(f"Include executed shell commands in a fenced sh block in {heading!r}")
    evidence = re.search(r"^Evidence:\s*\n(.*)", e2e, re.M | re.S)
    if not evidence or not (
        re.search(r"```text\n\S[\s\S]*?\n```", evidence[1])
        or re.search(r"https://\S+", evidence[1])
    ):
        raise ValueError("Provide an Evidence: output block or a CI run link")
    for item in ATTESTATIONS:
        if not re.search(rf"^- \[x\] {re.escape(item)}$", parts["Checklist"], re.M):
            raise ValueError(f"Complete the attestation only after performing it: {item}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument("--event-file", type=Path)
    source.add_argument("--body-file", type=Path)
    parser.add_argument("--commit")
    args = parser.parse_args()
    if args.event_file:
        pr = json.loads(args.event_file.read_text())["pull_request"]
        body, commit = pr.get("body") or "", pr["head"]["sha"]
    else:
        body, commit = args.body_file.read_text(), args.commit
        if not commit:
            parser.error("--body-file requires --commit")
    try:
        validate(body, commit)
    except ValueError as error:
        print(f"PR evidence: {error}. See CONTRIBUTING.md.", file=sys.stderr)
        return 1
    print("PASS: PR evidence includes current-commit E2E results and reproduction steps.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
