"""Classify actual changed paths; incomplete metadata never grants a docs exemption."""
import argparse
import json
from pathlib import Path, PurePosixPath
import re
import subprocess

DOCUMENTS = {".md", ".mdx", ".rst", ".adoc"}
IMAGES = {".png", ".jpg", ".jpeg", ".gif", ".svg", ".webp"}


def docs_only(paths):
    def document(path):
        suffix = PurePosixPath(path).suffix.lower()
        return suffix in DOCUMENTS or (path.startswith("docs/") and suffix in IMAGES)
    return bool(paths) and all(document(path) for path in paths)


def event_docs_only(event):
    if "pull_request" in event:
        pr = event["pull_request"]
        repo = event.get("repository", {}).get("full_name")
        if not repo or "number" not in pr or "changed_files" not in pr:
            return False
        pages = json.loads(subprocess.check_output([
            "gh", "api", f"repos/{repo}/pulls/{pr['number']}/files?per_page=100",
            "--paginate", "--slurp",
        ], text=True))
        files = [file for page in pages for file in page]
        # GitHub caps this API at 3,000 files. A truncated list must not exempt code.
        if len(files) != pr["changed_files"]:
            return False
        paths = []
        for file in files:
            paths.append(file["filename"])
            if "previous_filename" in file:
                paths.append(file["previous_filename"])
        return docs_only(paths)
    before, after = event.get("before", ""), event.get("after", "")
    if not all(re.fullmatch(r"[0-9a-f]{40}", sha) and sha != "0" * 40 for sha in (before, after)):
        return False
    # --no-renames includes both sides of a rename, including code renamed to .md.
    paths = subprocess.check_output([
        "git", "diff", "--no-ext-diff", "--no-textconv", "--name-only", "--no-renames", "-z", before, after,
    ]).decode().split("\0")
    return docs_only([path for path in paths if path])


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--event-file", type=Path, required=True)
    args = parser.parse_args()
    docs = event_docs_only(json.loads(args.event_file.read_text()))
    print(f"docs_only={str(docs).lower()}")
