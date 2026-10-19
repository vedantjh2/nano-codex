"""Exercise documentation exemptions through the real commands, without GitHub calls."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from test_pr_evidence import BODY, COMMIT, ROOT

SUMMARY = "## Change\nClarify the README reading order.\n"


class ChangeScope(unittest.TestCase):
    def evidence(self, pages, expected, body=SUMMARY, count=None):
        files = [file for page in pages for file in page]
        event = {"repository": {"full_name": "owner/repo"}, "pull_request": {
            "number": 7, "body": body, "head": {"sha": COMMIT},
            "changed_files": len(files) if count is None else count,
        }}
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            fake = root / "gh"
            fake.write_text(f"#!{sys.executable}\nimport os, sys\n"
                            "assert '--paginate' in sys.argv and '--slurp' in sys.argv\n"
                            "print(os.environ['SCOPE_FILES'])\n")
            fake.chmod(0o755)
            payload = root / "event.json"
            payload.write_text(json.dumps(event))
            env = dict(os.environ, PATH=directory + os.pathsep + os.environ["PATH"],
                       SCOPE_FILES=json.dumps(pages))
            result = subprocess.run([sys.executable, str(ROOT / "scripts/check-pr-evidence.py"),
                                     "--event-file", str(payload)], env=env, capture_output=True, text=True)
        self.assertEqual(result.returncode, expected, result.stderr)
        return result

    def test_documentation_needs_only_a_change_summary(self):
        result = self.evidence([[{"filename": "README.md"}, {"filename": "USAGE.md"}],
                                [{"filename": "docs/architecture.svg"}]], 0)
        self.assertIn("test evidence and reproduction steps are not required", result.stdout)

    def test_documentation_still_needs_a_summary(self):
        self.evidence([[{"filename": "README.md"}]], 1, body="")

    def test_mixed_code_workflow_and_dependency_changes_require_full_evidence(self):
        for path in ["src/agent.rs", "scripts/example.py", ".github/workflows/ci.yml",
                     "Cargo.lock", "requirements.txt", "docs/example.py"]:
            with self.subTest(path=path):
                files = [[{"filename": "README.md"}, {"filename": path}]]
                self.evidence(files, 1)
                self.evidence(files, 0, body=BODY)

    def test_renaming_code_to_markdown_does_not_grant_an_exemption(self):
        self.evidence([[{"filename": "docs/example.md", "previous_filename": "src/example.rs"}]], 1)
        self.evidence([[{"filename": "docs/new.md", "previous_filename": "docs/old.md"}]], 0)

    def test_later_pages_and_incomplete_file_lists_cannot_hide_code(self):
        self.evidence([[{"filename": "README.md"}], [{"filename": "src/agent.rs"}]], 1)
        self.evidence([[{"filename": "README.md"}]], 1, count=3001)
        self.evidence([[]], 1)

    def test_claiming_docs_only_in_a_code_pr_does_not_bypass_evidence(self):
        self.evidence([[{"filename": "src/agent.rs"}]], 1,
                      body=SUMMARY + "\nDocumentation-only; tests are not required.\n")

    def test_push_classification_uses_the_real_git_diff(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            def git(*args):
                return subprocess.check_output(["git", *args], cwd=root, text=True).strip()
            def commit():
                git("add", ".")
                git("-c", "user.name=Test", "-c", "user.email=test@example.invalid",
                    "commit", "--quiet", "-m", "fixture")
                return git("rev-parse", "HEAD")
            git("init", "--quiet")
            (root / "README.md").write_text("First.\n")
            first = commit()
            (root / "README.md").write_text("Clarified.\n")
            docs = commit()
            (root / "example.py").write_text("print('example')\n")
            code = commit()
            for before, after, expected in [(first, docs, True), (docs, code, False), ("0" * 40, code, False)]:
                with self.subTest(before=before):
                    event = root / "event.json"
                    event.write_text(json.dumps({"before": before, "after": after}))
                    result = subprocess.run([sys.executable, str(ROOT / "scripts/change_scope.py"),
                                             "--event-file", str(event)], cwd=root, capture_output=True, text=True)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertEqual(result.stdout.strip(), f"docs_only={str(expected).lower()}")


if __name__ == "__main__":
    unittest.main()
