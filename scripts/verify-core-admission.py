#!/usr/bin/env python3
"""Verify the Lumen core admission lock without network access."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Any


SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
GIT_SHA_RE = re.compile(r"^[0-9a-f]{40}$")


def fail(message: str) -> None:
    raise ValueError(message)


def require(condition: bool, message: str) -> None:
    if not condition:
        fail(message)


def run_git(repo: Path, *args: str) -> str:
    result = subprocess.run(
        ["git", "-C", str(repo), *args],
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        fail(
            f"git {' '.join(args)} failed in {repo}: "
            f"{result.stderr.strip() or result.stdout.strip()}"
        )
    return result.stdout.strip()


def require_commit(repo: Path, commit: str, label: str) -> None:
    require(GIT_SHA_RE.fullmatch(commit) is not None, f"{label} is not a full Git SHA")
    resolved = run_git(repo, "rev-parse", f"{commit}^{{commit}}")
    require(resolved == commit, f"{label} resolves to {resolved}, expected {commit}")


def require_ancestor(repo: Path, commit: str, label: str) -> None:
    result = subprocess.run(
        ["git", "-C", str(repo), "merge-base", "--is-ancestor", commit, "HEAD"],
        check=False,
        capture_output=True,
        text=True,
    )
    require(result.returncode == 0, f"{label} is not an ancestor of Science HEAD")


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def load_lock(path: Path) -> dict[str, Any]:
    with path.open(encoding="utf-8") as handle:
        value = json.load(handle)
    require(isinstance(value, dict), "lock root must be an object")
    return value


def verify_lock(
    lock: dict[str, Any],
    science_repo: Path,
    lumen_repo: Path | None,
    binary: Path | None,
) -> None:
    require(lock.get("schema_version") == 1, "unsupported schema_version")

    policy = lock["policy"]
    require(policy["source_of_truth"] == "lumen", "source_of_truth must be lumen")
    require(policy["full_tree_overlay"] is False, "full_tree_overlay must remain false")
    require(
        policy["version_equivalence_claimed"] is False,
        "selective admission must not claim version equivalence",
    )
    require(
        policy["science_session_actor_remains_execution_authority"] is True,
        "Science SessionActor authority invariant is missing",
    )

    baseline = lock["science_baseline"]
    version = (science_repo / "agent" / "VERSION").read_text(encoding="utf-8").strip()
    require(
        version == baseline["pinned_lumen_version"],
        f"agent/VERSION is {version}, but selective admission is pinned to "
        f"{baseline['pinned_lumen_version']}",
    )

    admissions = lock["admissions"]
    require(admissions, "at least one admission is required")
    for index, admission in enumerate(admissions):
        label = f"admissions[{index}].science_commit"
        commit = admission["science_commit"]
        require_commit(science_repo, commit, label)
        require_ancestor(science_repo, commit, label)
        require(admission["status"] == "admitted", f"admissions[{index}] is not admitted")
        for source_index, source_commit in enumerate(admission.get("lumen_commits", [])):
            require(
                GIT_SHA_RE.fullmatch(source_commit) is not None,
                f"admissions[{index}].lumen_commits[{source_index}] is not a full Git SHA",
            )

    comparison = lock["comparison"]
    require(
        comparison["total_drift"]
        == comparison["diverged_rust_files"] + comparison["missing_in_science_rust_files"],
        "comparison total_drift does not add up",
    )
    require(
        comparison["tracked_security_markers_present"]
        == comparison["tracked_security_markers_total"],
        "not all tracked security markers are present",
    )
    require(
        comparison["strict_zero_drift_gate"] is False,
        "record cannot claim a zero-drift gate while drift remains",
    )

    verification = lock["verification"]
    for result in verification["source_and_focused_tests"]:
        require(result["exit_code"] == 0, f"{result['command']} did not exit zero")
        require(result["failed"] == 0, f"{result['command']} records failures")
    built_binary = verification["built_binary"]
    require(
        SHA256_RE.fullmatch(built_binary["sha256"]) is not None,
        "built binary SHA-256 is malformed",
    )
    require(built_binary["failures"] == 0, "built-binary evidence records failures")
    require_commit(
        science_repo,
        built_binary["source_commit"],
        "verification.built_binary.source_commit",
    )
    require_ancestor(
        science_repo,
        built_binary["source_commit"],
        "verification.built_binary.source_commit",
    )

    deferred_items = {entry["item"] for entry in lock["deferred"]}
    require("full v0.1.251 source parity" in deferred_items, "full parity deferral is missing")
    require(
        "version metadata bump from 0.1.222 to 0.1.251" in deferred_items,
        "version metadata deferral is missing",
    )

    if lumen_repo is not None:
        source = lock["lumen_source"]
        require_commit(lumen_repo, source["target_tag_commit"], "lumen target_tag_commit")
        require_commit(lumen_repo, source["audited_main_head"], "lumen audited_main_head")
        tag_commit = run_git(lumen_repo, "rev-parse", f"{source['target_tag']}^{{commit}}")
        require(
            tag_commit == source["target_tag_commit"],
            f"{source['target_tag']} resolves to {tag_commit}, "
            f"expected {source['target_tag_commit']}",
        )
        for index, admission in enumerate(admissions):
            for source_index, source_commit in enumerate(admission.get("lumen_commits", [])):
                require_commit(
                    lumen_repo,
                    source_commit,
                    f"admissions[{index}].lumen_commits[{source_index}]",
                )

    if binary is not None:
        require(binary.is_file(), f"binary does not exist: {binary}")
        actual = sha256(binary)
        require(
            actual == built_binary["sha256"],
            f"binary SHA-256 is {actual}, expected {built_binary['sha256']}",
        )


def main() -> int:
    script_path = Path(__file__).resolve()
    default_science_repo = script_path.parent.parent
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--lock",
        type=Path,
        default=default_science_repo
        / "docs"
        / "science"
        / "5.0"
        / "core-v0.1.251-admission.lock.json",
    )
    parser.add_argument("--science-repo", type=Path, default=default_science_repo)
    parser.add_argument("--lumen-repo", type=Path)
    parser.add_argument("--binary", type=Path)
    args = parser.parse_args()

    try:
        lock = load_lock(args.lock.resolve())
        verify_lock(
            lock,
            args.science_repo.resolve(),
            args.lumen_repo.resolve() if args.lumen_repo else None,
            args.binary.resolve() if args.binary else None,
        )
    except (KeyError, OSError, ValueError, json.JSONDecodeError) as error:
        print(f"FAIL: {error}", file=sys.stderr)
        return 1

    checked = ["lock", "Science history", "version boundary"]
    if args.lumen_repo:
        checked.append("Lumen source refs")
    if args.binary:
        checked.append("binary hash")
    print(f"PASS: core admission verified ({', '.join(checked)})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
