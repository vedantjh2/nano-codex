"""Opt-in live test: a real model must read an unknown value through the CLI tool."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import urllib.request
import uuid


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:18000/v1")
    parser.add_argument("--provider", choices=["vllm", "sglang"], default="vllm")
    parser.add_argument("--model")
    parser.add_argument("--binary", default=shutil.which("nano-codex"))
    args = parser.parse_args()
    if not args.binary:
        raise SystemExit("Install the CLI first: make install")
    headers = {}
    if os.environ.get("NANO_CODEX_API_KEY"):
        headers["Authorization"] = "Bearer " + os.environ["NANO_CODEX_API_KEY"]
    request = urllib.request.Request(args.base_url.rstrip("/") + "/models", headers=headers)
    with urllib.request.urlopen(request, timeout=10) as response:
        models = json.load(response)["data"]
    model = args.model or models[0]["id"]
    with tempfile.TemporaryDirectory(prefix="nano-codex-live-local-") as directory:
        workspace = Path(directory)
        nonce = "NANO_CODEX_" + uuid.uuid4().hex
        (workspace / "nonce.txt").write_text(nonce + "\n")
        environment = dict(os.environ, NANO_CODEX_HOME=str(workspace / "settings"))
        command = [
            args.binary, "--provider", args.provider, "--model", model,
            "--base-url", args.base_url, "--approval", "read-only", "--max-steps", "5",
            "--json", "-C", directory, "exec",
            "Call read_file with path nonce.txt, then reply with exactly the contents of that file.",
        ]
        result = subprocess.run(command, env=environment, capture_output=True, text=True, timeout=180)
        if result.returncode:
            raise SystemExit(result.stderr + result.stdout[-3000:])
        events = [json.loads(line) for line in result.stdout.splitlines()]
        tools = [event["data"]["name"] for event in events if event["event"] == "tool_call"]
        answer = "".join(event["data"] for event in events if event["event"] == "text_delta")
        if "read_file" not in tools:
            raise SystemExit("The model finished without requesting read_file. Answer: " + answer[:1000])
        if nonce not in answer:
            observations = [event["data"] for event in events if event["event"] in ("tool_call", "tool_result")]
            raise SystemExit("The model did not return the observed value. Trace: " + json.dumps(observations)[:2000] + " Answer: " + answer[:1000])
        print(f"PASS: {model} read a temporary file through {args.provider} and returned its value.")


if __name__ == "__main__":
    main()
