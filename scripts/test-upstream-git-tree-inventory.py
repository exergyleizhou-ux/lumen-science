#!/usr/bin/env python3
"""Focused regressions for metadata-only source inventory."""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
SCRIPT = ROOT / "scripts/generate-upstream-git-tree-inventory.py"


def run(tree: dict[str, object]) -> tuple[subprocess.CompletedProcess[str], dict[str, object]]:
    with tempfile.TemporaryDirectory() as tmp:
        tree_path = Path(tmp) / "tree.json"
        output = Path(tmp) / "inventory.json"
        tree_path.write_text(json.dumps(tree), encoding="utf-8")
        proc = subprocess.run([sys.executable, str(SCRIPT), "--source-id", "fixture", "--exact-commit", "a" * 40, "--recorded-at", "2026-08-01T00:00:00+08:00", "--tree-json", str(tree_path), "--output", str(output)], check=False, capture_output=True, text=True)
        value = json.loads(output.read_text(encoding="utf-8")) if output.exists() else {}
    return proc, value


def main() -> int:
    good, value = run({"tree": [{"path": "skills/locked.md", "type": "blob", "sha": "b" * 40, "size": 42}, {"path": "src/run.py", "type": "blob", "sha": "c" * 40, "size": 9}, {"path": "src", "type": "tree", "sha": "d" * 40}]})
    bad, _ = run({"tree": [{"path": "../escape", "type": "blob", "sha": "b" * 40}]})
    entries = {entry["path"]: entry for entry in value.get("entries", [])}
    results = [
        ("metadata-only blob inventory succeeds", good.returncode == 0 and len(entries) == 2),
        ("restricted-looking skill records no content and stays quarantined", entries.get("skills/locked.md", {}).get("kind") == "git-blob-metadata-only" and entries.get("skills/locked.md", {}).get("disposition") == "quarantine" and "sha256" not in entries.get("skills/locked.md", {})),
        ("unsafe paths fail closed", bad.returncode == 1 and "unsafe path" in bad.stdout),
    ]
    for name, passed in results:
        print(f"  {'ok' if passed else 'FAIL':<4}  {name}")
    print(f"\n{'OK' if all(passed for _, passed in results) else 'FAIL'}: {sum(passed for _, passed in results)}/{len(results)} passed")
    return 0 if all(passed for _, passed in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
