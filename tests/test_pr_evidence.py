"""Exercise the real evidence-checking command with temporary GitHub event files."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
COMMIT = "a" * 40
BODY = f"""## Change
Reject incomplete PR descriptions before merging.
## Tested commit
{COMMIT}
## End-to-end test
Environment: macOS, Python 3.12; no model or account required.
Scenario: Run the checker as a subprocess with a temporary PR event.
Expected result: Complete evidence exits zero; an old SHA exits one.
Actual result: Both exit codes matched and no contributor commands executed.
Failure case: A stale tested SHA was rejected with a rerun-checks message.
Commands:
```sh
python3 -m unittest discover -s tests -p 'test_pr_evidence.py'
```
Evidence:
```text
OK: positive, missing, stale, placeholder, and inert-command cases passed.
```
## Reproduction
Use Python 3.11+ from a clean checkout of this PR; no packages or private files.
```sh
python3 -m unittest discover -s tests -p 'test_pr_evidence.py'
```
Expected output: all unittest cases pass.
## Limitations
The checker validates structure; owner review assesses the evidence's substance.
## Checklist
- [x] I ran the checks on the tested commit and inspected their output.
- [x] I exercised the changed behavior end to end, including a failure case.
- [x] The reproduction steps need no private files or credentials.
"""


class EvidenceCheck(unittest.TestCase):
    def check_body(self, body, success, commit=COMMIT):
        with tempfile.TemporaryDirectory() as directory:
            event = Path(directory) / "event.json"
            event.write_text(json.dumps({"pull_request": {"body": body, "head": {"sha": commit}}}))
            checked = subprocess.run(
                [sys.executable, str(ROOT / "scripts/check-pr-evidence.py"), "--event-file", str(event)],
                capture_output=True, text=True,
            )
        self.assertEqual(checked.returncode, 0 if success else 1, checked.stderr)
        return checked

    def test_complete_evidence_passes_both_cli_input_modes(self):
        self.check_body(BODY, True)
        with tempfile.TemporaryDirectory() as directory:
            body = Path(directory) / "body.md"
            body.write_text(BODY)
            checked = subprocess.run(
                [sys.executable, str(ROOT / "scripts/check-pr-evidence.py"),
                 "--body-file", str(body), "--commit", COMMIT], capture_output=True, text=True,
            )
            self.assertEqual(checked.returncode, 0, checked.stderr)

    def test_empty_body_and_untouched_template_fail(self):
        self.check_body(None, False)
        self.check_body((ROOT / ".github/pull_request_template.md").read_text(), False)

    def test_evidence_must_match_the_current_commit(self):
        checked = self.check_body(BODY, False, commit="b" * 40)
        self.assertIn("current PR head SHA", checked.stderr)

    def test_missing_results_failure_case_and_commands_fail(self):
        for text in ["Actual result:", "Failure case:", "Environment:"]:
            with self.subTest(text=text):
                self.check_body(BODY.replace(text, "Other:"), False)
        self.check_body(BODY.replace("```sh", "```text"), False)
        self.check_body(BODY.replace("Actual result: Both exit codes matched and no contributor commands executed.",
                                     "Actual result:"), False)

    def test_output_or_a_ci_link_is_required(self):
        start, rest = BODY.split("Evidence:\n", 1)
        _, tail = rest.split("## Reproduction", 1)
        self.check_body(start + "Evidence:\nNo output.\n## Reproduction" + tail, False)
        self.check_body(start + "Evidence:\nhttps://github.com/owner/repo/actions/runs/123\n## Reproduction" + tail, True)

    def test_checkboxes_comments_and_duplicate_headings_cannot_fill_the_template(self):
        self.check_body(BODY.replace("- [x]", "- [ ]", 1), False)
        self.check_body("<!--" + BODY + "-->", False)
        self.check_body(BODY + "\n## Change\nDuplicate.", False)
        self.check_body(BODY.replace("## Change", "## Change\nTODO: explain"), False)

    def test_fenced_headings_are_data_and_commands_are_never_executed(self):
        with tempfile.TemporaryDirectory() as directory:
            marker = Path(directory) / "must-not-exist"
            malicious = f"## Change\ntouch {marker}\n$(touch {marker})\n"
            self.check_body(BODY.replace("python3 -m unittest", malicious + "python3 -m unittest", 1), True)
            self.assertFalse(marker.exists())


if __name__ == "__main__":
    unittest.main()
