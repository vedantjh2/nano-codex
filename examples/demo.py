"""Run the real CLI against a scripted model; no account, GPU, or packages needed."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from threading import Thread

ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target/debug/nano-codex"


def tool(call_id, name, arguments):
    return {
        "tool_calls": [{
            "index": 0,
            "id": call_id,
            "type": "function",
            "function": {"name": name, "arguments": json.dumps(arguments)},
        }]
    }


class ScriptedModel(BaseHTTPRequestHandler):
    step = 0
    script = [
        tool("read", "read_file", {"path": "maths.py"}),
        tool("edit", "edit_file", {
            "path": "maths.py", "old_text": "return a - b", "new_text": "return a + b",
        }),
        tool("test", "shell", {"command": "python3 -m unittest -v"}),
        {"content": "Fixed add() and ran the unit tests successfully."},
    ]

    def do_POST(self):
        request = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        assert self.path == "/v1/chat/completions"
        if ScriptedModel.step == 3:
            assert "exit=0" in request["messages"][-1]["content"]
        delta = self.script[ScriptedModel.step]
        ScriptedModel.step += 1
        reason = "tool_calls" if "tool_calls" in delta else "stop"
        event = {"choices": [{"delta": delta, "finish_reason": reason}]}
        body = f"data: {json.dumps(event)}\n\ndata: [DONE]\n\n".encode()
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *_):
        pass


def main():
    binary = str(BINARY) if BINARY.exists() else shutil.which("nano-codex")
    if not binary:
        raise SystemExit("Build the CLI first: cargo build --locked")
    server = ThreadingHTTPServer(("127.0.0.1", 0), ScriptedModel)
    Thread(target=server.serve_forever, daemon=True).start()
    try:
        with tempfile.TemporaryDirectory(prefix="nano-codex-demo-") as directory:
            workspace = Path(directory)
            (workspace / "maths.py").write_text("def add(a, b):\n    return a - b\n")
            (workspace / "test_maths.py").write_text(
                "import unittest\nfrom maths import add\n\n"
                "class AdditionTest(unittest.TestCase):\n"
                "    def test_positive(self):\n        self.assertEqual(add(2, 3), 5)\n"
                "    def test_negative(self):\n        self.assertEqual(add(-2, -3), -5)\n"
            )
            environment = dict(os.environ, NANO_CODEX_HOME=str(workspace / "config"))
            environment.pop("NANO_CODEX_API_KEY", None)
            command = [
                binary, "--provider", "vllm", "--model", "scripted-demo",
                "--base-url", f"http://127.0.0.1:{server.server_port}/v1",
                "--approval", "auto", "-C", directory, "exec", "Fix add() and test it.",
            ]
            print("Scripted model → real HTTP → real CLI → real file edit + tests", flush=True)
            subprocess.run(command, env=environment, check=True)
            assert "return a + b" in (workspace / "maths.py").read_text()
            print("\nDemo passed. The temporary workspace will now be removed.")
    finally:
        server.shutdown()
        server.server_close()


if __name__ == "__main__":
    main()
