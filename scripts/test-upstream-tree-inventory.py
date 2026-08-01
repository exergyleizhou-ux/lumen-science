#!/usr/bin/env python3
"""Focused contract tests for the non-executable upstream tree inventory."""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
SCRIPT = ROOT / "scripts/generate-upstream-tree-inventory.py"


def invoke(source: Path, output: Path, *extra: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(SCRIPT), "--source-id", "fixture", "--source-root", str(source), "--exact-commit", "a" * 40, "--recorded-at", "2026-08-01T00:00:00+08:00", "--output", str(output), *extra],
        check=False,
        capture_output=True,
        text=True,
    )


def main() -> int:
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp) / "source"
        root.mkdir()
        (root / "code.rs").write_text("fn main() {}\n", encoding="utf-8")
        (root / "skills").mkdir()
        (root / "skills" / "example.md").write_text("skill\n", encoding="utf-8")
        (root / "outside-link").symlink_to("../outside")
        (root / ".git").mkdir()
        (root / ".git" / "ignored").write_text("ignored\n", encoding="utf-8")
        output = Path(tmp) / "inventory.json"
        good = invoke(root, output)
        document = json.loads(output.read_text(encoding="utf-8")) if output.exists() else {}
        entries = {entry["path"]: entry for entry in document.get("entries", [])}
        inside = invoke(root, root / "must-not-write.json")
        too_small = invoke(root, Path(tmp) / "small.json", "--max-file-bytes", "1")
    results = [
        ("regular files are hashed and default quarantined", good.returncode == 0 and entries.get("code.rs", {}).get("sha256") and entries.get("code.rs", {}).get("disposition") == "quarantine"),
        ("skills are classified but not executable", entries.get("skills/example.md", {}).get("candidate_asset_kind") == "skill" and entries.get("skills/example.md", {}).get("execution_authority") == "none"),
        ("symlinks are recorded without traversal", entries.get("outside-link", {}).get("kind") == "symlink" and ".git/ignored" not in entries),
        ("generator refuses to write into upstream checkout", inside.returncode == 1 and "outside source-root" in inside.stdout),
        ("oversized input fails rather than silently omitting bytes", too_small.returncode == 1 and "exceeds review budget" in too_small.stdout),
    ]
    for name, passed in results:
        print(f"  {'ok' if passed else 'FAIL':<4}  {name}")
    print(f"\n{'OK' if all(passed for _, passed in results) else 'FAIL'}: {sum(passed for _, passed in results)}/{len(results)} passed")
    return 0 if all(passed for _, passed in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
