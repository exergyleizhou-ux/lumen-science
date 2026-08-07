#!/usr/bin/env python3
"""Negative corpus for the canonical execution-book verifier."""

from __future__ import annotations

import hashlib
import json
import shutil
import subprocess
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
VERIFY = ROOT / "scripts/verify-nextgen-canonical-book.py"
LOCK_REL = Path("docs/science/5.0/NEXTGEN_CANONICAL_EXECUTION_BOOK.lock.json")


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def stage() -> tuple[tempfile.TemporaryDirectory[str], Path, Path, Path]:
    temp = tempfile.TemporaryDirectory()
    root = Path(temp.name)
    lock = json.loads((ROOT / LOCK_REL).read_text(encoding="utf-8"))
    paths = [lock["book_path"], *lock["pointer_files"], str(LOCK_REL)]
    for relative in paths:
        source = ROOT / relative
        target = root / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)
    subprocess.run(["git", "-C", str(root), "init", "-q"], check=True)
    source_objects = subprocess.run(
        ["git", "-C", str(ROOT), "rev-parse", "--git-path", "objects"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()
    alternate = root / ".git/objects/info/alternates"
    alternate.parent.mkdir(parents=True, exist_ok=True)
    alternate.write_text(str(Path(source_objects).resolve()) + "\n", encoding="utf-8")
    source_head = subprocess.run(
        ["git", "-C", str(ROOT), "rev-parse", "HEAD"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()
    subprocess.run(["git", "-C", str(root), "update-ref", "refs/heads/test", source_head], check=True)
    subprocess.run(["git", "-C", str(root), "symbolic-ref", "HEAD", "refs/heads/test"], check=True)
    book_path = root / lock["book_path"]
    lock_path = root / LOCK_REL
    return temp, root, book_path, lock_path


def run(root: Path, lock_path: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["python3", str(VERIFY), "--root", str(root), "--lock", str(lock_path)],
        check=False,
        capture_output=True,
        text=True,
    )


def expect_failure(label: str, mutate) -> None:  # type: ignore[no-untyped-def]
    temp, root, book_path, lock_path = stage()
    try:
        mutate(book_path, lock_path, root)
        result = run(root, lock_path)
        if result.returncode == 0:
            raise AssertionError(f"{label}: verifier unexpectedly passed")
    finally:
        temp.cleanup()


def update_book_hash(book_path: Path, lock_path: Path) -> None:
    lock = json.loads(lock_path.read_text(encoding="utf-8"))
    lock["book_sha256"] = digest(book_path)
    lock_path.write_text(json.dumps(lock, indent=2) + "\n", encoding="utf-8")


def main() -> int:
    temp, root, _, lock_path = stage()
    try:
        result = run(root, lock_path)
        if result.returncode != 0:
            raise AssertionError(f"baseline failed: {result.stderr}")
    finally:
        temp.cleanup()

    expect_failure("byte tamper", lambda book, _lock, _root: book.write_text(book.read_text() + "\ntamper\n"))

    def remove_heading(book: Path, lock: Path, _root: Path) -> None:
        text = book.read_text(encoding="utf-8").replace("## 44. 最终 Definition of Done", "## removed", 1)
        book.write_text(text, encoding="utf-8")
        update_book_hash(book, lock)

    expect_failure("required heading", remove_heading)

    def add_cycle(book: Path, lock: Path, _root: Path) -> None:
        text = book.read_text(encoding="utf-8").replace(
            '  S0["S0 Science P0 repair"] --> F0["F0 current baseline receipt"]',
            '  S0["S0 Science P0 repair"] --> F0["F0 current baseline receipt"]\n  F0 --> S0',
            1,
        )
        book.write_text(text, encoding="utf-8")
        update_book_hash(book, lock)

    expect_failure("dependency cycle", add_cycle)

    def remove_pointer(_book: Path, lock: Path, root: Path) -> None:
        data = json.loads(lock.read_text(encoding="utf-8"))
        pointer = root / data["pointer_files"][0]
        pointer.write_text(pointer.read_text(encoding="utf-8").replace(Path(data["book_path"]).name, "missing.md"), encoding="utf-8")

    expect_failure("supersession pointer", remove_pointer)

    def claim_completion(_book: Path, lock: Path, _root: Path) -> None:
        data = json.loads(lock.read_text(encoding="utf-8"))
        data["top_level_gate_pass_count"] = 1
        lock.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")

    expect_failure("false gate completion", claim_completion)
    print("PASS: canonical execution-book verifier negative corpus (6 passed)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
