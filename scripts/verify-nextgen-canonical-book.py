#!/usr/bin/env python3
"""Verify the canonical Lumen Science execution book without claiming features."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parent.parent
DEFAULT_LOCK = ROOT / "docs/science/5.0/NEXTGEN_CANONICAL_EXECUTION_BOOK.lock.json"
SHA_RE = re.compile(r"^[0-9a-f]{40}$")
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def load_json(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ValueError(f"cannot read canonical book lock: {error}") from error
    require(isinstance(value, dict), "canonical book lock root must be an object")
    return value


def relative_path(value: Any, label: str) -> Path:
    require(isinstance(value, str) and value, f"{label} must be a non-empty string")
    path = Path(value)
    require(not path.is_absolute(), f"{label} must be repository-relative")
    require(".." not in path.parts, f"{label} escapes the repository")
    return path


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def verify_dag(book: str) -> None:
    blocks = re.findall(r"~~~mermaid\s*\nflowchart TD\n(.*?)\n~~~", book, re.DOTALL)
    require(blocks, "canonical dependency Mermaid graph is missing")

    def edges_for(block: str) -> list[tuple[str, str]]:
        edges: list[tuple[str, str]] = []
        for line in block.splitlines():
            edge = re.match(
                r"\s*([A-Za-z][A-Za-z0-9_]*)(?:\[[^]]*\])?\s*-->\s*"
                r"([A-Za-z][A-Za-z0-9_]*)",
                line,
            )
            if edge:
                edges.append((edge.group(1), edge.group(2)))
        return edges

    edges = max((edges_for(block) for block in blocks), key=len)
    require(len(edges) >= 30, "canonical dependency graph is unexpectedly small")
    require(len(edges) == len(set(edges)), "canonical dependency graph repeats an edge")

    graph: dict[str, list[str]] = {}
    for source, target in edges:
        graph.setdefault(source, []).append(target)
        graph.setdefault(target, [])
    visiting: set[str] = set()
    visited: set[str] = set()

    def visit(node: str) -> None:
        require(node not in visiting, f"canonical dependency graph contains a cycle at {node}")
        if node in visited:
            return
        visiting.add(node)
        for target in graph[node]:
            visit(target)
        visiting.remove(node)
        visited.add(node)

    for node in graph:
        visit(node)


def verify_local_links(book_path: Path, book: str) -> None:
    for target in re.findall(r"\[[^]]+\]\(([^)]+)\)", book):
        if "://" in target or target.startswith("#"):
            continue
        local = (book_path.parent / target.split("#", 1)[0]).resolve()
        require(local.exists(), f"canonical book has a missing local link: {target}")


def verify_contract(lock_path: Path, root: Path) -> None:
    lock = load_json(lock_path)
    require(lock.get("schema_version") == 1, "canonical book schema_version must be 1")
    require(lock.get("status") == "execution_contract", "canonical book status is invalid")
    require(lock.get("feature_completion_claimed") is False, "book lock claims feature completion")
    require(lock.get("top_level_gate_pass_count") == 0, "book lock claims a passed top-level gate")

    book_path = root / relative_path(lock.get("book_path"), "book_path")
    require(book_path.is_file(), "canonical execution book is missing")
    expected_hash = lock.get("book_sha256")
    require(
        isinstance(expected_hash, str) and SHA256_RE.fullmatch(expected_hash) is not None,
        "canonical book sha256 is malformed",
    )
    require(sha256(book_path) == expected_hash, "canonical execution book hash drifted")
    book = book_path.read_text(encoding="utf-8")
    require(book.count("~~~") % 2 == 0, "canonical book has an unbalanced fenced block")

    for heading in lock.get("required_headings", []):
        require(isinstance(heading, str) and book.count(heading) == 1, f"required heading drifted: {heading}")
    for term in lock.get("required_terms", []):
        require(isinstance(term, str) and term in book, f"required contract term is missing: {term}")

    verify_dag(book)
    verify_local_links(book_path, book)

    for pointer in lock.get("pointer_files", []):
        pointer_path = root / relative_path(pointer, "pointer_files[]")
        require(pointer_path.is_file(), f"canonical pointer file is missing: {pointer}")
        require(book_path.name in pointer_path.read_text(encoding="utf-8"), f"pointer omits canonical book: {pointer}")

    science = lock.get("science_observation")
    require(isinstance(science, dict), "science_observation is missing")
    source_commit = science.get("source_commit")
    require(isinstance(source_commit, str) and SHA_RE.fullmatch(source_commit), "Science source commit is malformed")
    ancestor = subprocess.run(
        ["git", "-C", str(root), "merge-base", "--is-ancestor", source_commit, "HEAD"],
        check=False,
        capture_output=True,
        text=True,
    )
    require(ancestor.returncode == 0, "Science observation is not an ancestor of HEAD")

    lumen = lock.get("canonical_lumen_observation")
    require(isinstance(lumen, dict), "canonical_lumen_observation is missing")
    require(lumen.get("observation_only") is True, "Lumen observation must remain read-only")
    pin_eligible = lumen.get("pin_eligible")
    require(isinstance(pin_eligible, bool), "pin_eligible must be a boolean observation")
    if pin_eligible is True:
        receipt = lumen.get("r0_receipt")
        require(isinstance(receipt, dict), "pin requires an r0 receipt object")
        require(
            isinstance(receipt.get("release_tags"), list)
            and receipt["release_tags"]
            and all(isinstance(t, str) and t for t in receipt["release_tags"]),
            "pin r0 receipt must list release tags",
        )
        for field in ("source_commit_a", "evidence_commit_b"):
            value = receipt.get(field)
            require(
                isinstance(value, str) and SHA_RE.fullmatch(value) is not None,
                f"pin r0 receipt {field} must be a full SHA",
            )
        require(
            isinstance(receipt.get("ci_green"), str) and receipt["ci_green"],
            "pin r0 receipt must record exact CI green evidence",
        )
        require(
            isinstance(receipt.get("observed_at"), str) and receipt["observed_at"],
            "pin r0 receipt must record an observation timestamp",
        )
    else:
        require(
            lumen.get("r0_receipt") is None,
            "a non-eligible observation must not carry an r0 receipt",
        )
    lumen_head = lumen.get("local_head")
    require(isinstance(lumen_head, str) and SHA_RE.fullmatch(lumen_head), "Lumen observed head is malformed")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--lock", type=Path, default=DEFAULT_LOCK)
    args = parser.parse_args()
    try:
        verify_contract(args.lock, args.root.resolve())
    except ValueError as error:
        print(f"FAIL: {error}", file=sys.stderr)
        return 1
    print("PASS: canonical Lumen Science execution book verified (plan only; 0 gates claimed)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
