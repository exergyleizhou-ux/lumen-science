#!/usr/bin/env python3
"""Focused negative tests for the external Cargo path-dependency gate."""

from __future__ import annotations

import subprocess
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
VERIFY = ROOT / "scripts/verify-no-external-cargo-path-deps.py"


def run(root: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(VERIFY), "--root", str(root)],
        check=False,
        capture_output=True,
        text=True,
    )


def write_manifest(root: Path, dependency: str) -> None:
    manifest = root / "agent/crates/example/Cargo.toml"
    manifest.parent.mkdir(parents=True)
    manifest.write_text(
        "[package]\nname = 'example'\nversion = '0.1.0'\n\n[dependencies]\ndep = " + dependency + "\n",
        encoding="utf-8",
    )


def main() -> int:
    results: list[tuple[str, bool]] = []
    results.append(("checked-in manifests have no external dependency paths", run(ROOT).returncode == 0))
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        write_manifest(root, "{ path = '../internal' }")
        results.append(("workspace-internal relative dependency is allowed", run(root).returncode == 0))
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        write_manifest(root, "{ path = '/Users/lei/code/lumen' }")
        results.append(("absolute local Lumen path fails closed", run(root).returncode != 0))
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        write_manifest(root, "{ path = '../../../../outside' }")
        results.append(("escaping relative dependency fails closed", run(root).returncode != 0))

    for name, passed in results:
        print(f"  {'ok' if passed else 'FAIL':<4}  {name}")
    print(f"\n{'OK' if all(passed for _, passed in results) else 'FAIL'}: {sum(passed for _, passed in results)}/{len(results)} passed")
    return 0 if all(passed for _, passed in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
