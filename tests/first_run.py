"""Exercise the first-run terminal UI without accounts, GPUs, or third-party packages."""
import json
import os
from pathlib import Path
import pty
import select
import subprocess
import tempfile
import threading
import time
import tomllib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


class Catalog(BaseHTTPRequestHandler):
    def do_GET(self):
        assert self.path == "/v1/models"
        body = json.dumps({"data": [{"id": "test-model"}]}).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *_):
        pass


def exercise(server, provider, choice, default_port):
    binary = Path(__file__).resolve().parents[1] / "target/debug/nano-codex"
    with tempfile.TemporaryDirectory(prefix="nano-codex-setup-test-") as directory:
        root = Path(directory)
        environment = dict(os.environ, NANO_CODEX_HOME=str(root / "settings"))
        for key in ["NANO_CODEX_PROVIDER", "NANO_CODEX_MODEL", "NANO_CODEX_BASE_URL", "NANO_CODEX_API_KEY", "OPENAI_API_KEY"]:
            environment.pop(key, None)
        master, slave = pty.openpty()
        child = subprocess.Popen([str(binary), "-C", directory], stdin=slave, stdout=slave, stderr=slave, env=environment)
        os.close(slave)
        answers = [
            (b"Connection [1]: ", choice.encode() + b"\n"),
            (f"Server port or API URL [{default_port}]: ".encode(), f"{server.server_port}\n".encode()),
            (b"Model number or name [1]: ", b"1\n"),
            (b"nano> ", b"/exit\n"),
        ]
        transcript = b""
        sent = set()
        deadline = time.monotonic() + 15
        try:
            while child.poll() is None and time.monotonic() < deadline:
                if select.select([master], [], [], 0.1)[0]:
                    try:
                        transcript += os.read(master, 65536)
                    except OSError:
                        break
                for prompt, answer in answers:
                    if prompt in transcript and prompt not in sent:
                        os.write(master, answer)
                        sent.add(prompt)
            if child.poll() is None:
                child.kill()
            assert child.wait() == 0, transcript.decode(errors="replace")
        finally:
            if child.poll() is None:
                child.kill()
                child.wait()
            os.close(master)
        config = tomllib.loads((root / "settings/config.toml").read_text())
        assert config["provider"] == provider
        assert config["base_url"] == f"http://127.0.0.1:{server.server_port}/v1"
        assert config["model"] == "test-model"
        print(f"PASS: first-run {provider} connection, port, model discovery, saved settings, and REPL.")


def main():
    server = ThreadingHTTPServer(("127.0.0.1", 0), Catalog)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    try:
        exercise(server, "vllm", "2", 8000)
        exercise(server, "sglang", "3", 30000)
    finally:
        server.shutdown()
        server.server_close()


if __name__ == "__main__":
    main()
