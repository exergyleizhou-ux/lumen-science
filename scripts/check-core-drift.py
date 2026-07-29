#!/usr/bin/env python3
"""Machine-visible Core drift gate between Science and an audited Lumen head.

Compares agent/crates/**/*.rs (excluding the Science-only xai-grok-science crate)
between this repository and an exact upstream Lumen checkout.

Semantics (not strict-zero):
  - Known drift recorded in docs/science/5.0/core-v0.1.251-admission.lock.json
    is allowed (today: 129 shared_diverged + 5 missing_from_science = 134).
  - Silent growth/shrink/reclassification or equal-count replacement fails.
  - Does NOT claim single-Rust-base completion.

Exit codes:
  0  — comparison matches the admission lock (or fixture expectation)
  1  — drift mismatch, pin mismatch, path escape, or I/O/schema error
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path
from typing import Any


EXCLUDED_CRATE_PREFIXES = (
    "codegen/xai-grok-science/",
    "codegen/xai-grok-science\\",
)
DRIFT_MANIFEST_SCHEMA = "lumen-core-drift-v1"


class DriftError(RuntimeError):
    pass


def fail(message: str) -> None:
    raise DriftError(message)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def is_excluded(rel: str) -> bool:
    normalized = rel.replace("\\", "/")
    return any(normalized.startswith(prefix.replace("\\", "/")) for prefix in EXCLUDED_CRATE_PREFIXES)


def collect_rust_files(root: Path) -> dict[str, str]:
    """Map relative path under agent/crates → sha256 of file bytes."""
    crates = (root / "agent" / "crates").resolve()
    if not crates.is_dir():
        fail(f"missing agent/crates under {root}")

    out: dict[str, str] = {}
    for path in crates.rglob("*.rs"):
        # Reject symlink escape: resolved path must stay under crates root.
        try:
            resolved = path.resolve(strict=True)
        except OSError as exc:
            fail(f"cannot resolve {path}: {exc}")
        try:
            resolved.relative_to(crates)
        except ValueError:
            fail(f"path escape rejected: {path} resolves outside agent/crates")
        if path.is_symlink():
            # Symlink whose target is still inside crates is allowed only if
            # the link itself is not used to smuggle a different content view.
            # We hash the resolved file bytes after the containment check.
            pass
        rel = resolved.relative_to(crates).as_posix()
        if is_excluded(rel):
            continue
        out[rel] = sha256_file(resolved)
    return out


def classify(science: dict[str, str], upstream: dict[str, str]) -> dict[str, Any]:
    shared = set(science) & set(upstream)
    upstream_identical = sorted(rel for rel in shared if science[rel] == upstream[rel])
    shared_diverged = sorted(rel for rel in shared if science[rel] != upstream[rel])
    missing_from_science = sorted(set(upstream) - set(science))
    science_only = sorted(set(science) - set(upstream))
    total_drift = len(shared_diverged) + len(missing_from_science)
    # Counts alone cannot detect an equal-size substitution: one known
    # divergent file can become identical while a previously identical file
    # becomes divergent. Bind classification, path and both byte digests into a
    # stable manifest so every such replacement (and edits within an already
    # divergent file) requires an explicit lock review.
    manifest_entries: list[dict[str, str]] = []
    for rel in shared_diverged:
        manifest_entries.append(
            {
                "classification": "shared_diverged",
                "path": rel,
                "science_sha256": science[rel],
                "upstream_sha256": upstream[rel],
            }
        )
    for rel in missing_from_science:
        manifest_entries.append(
            {
                "classification": "missing_from_science",
                "path": rel,
                "upstream_sha256": upstream[rel],
            }
        )
    for rel in science_only:
        manifest_entries.append(
            {
                "classification": "science_only",
                "path": rel,
                "science_sha256": science[rel],
            }
        )
    manifest_payload = {
        "schema": DRIFT_MANIFEST_SCHEMA,
        "entries": manifest_entries,
    }
    manifest_sha256 = hashlib.sha256(
        json.dumps(
            manifest_payload,
            ensure_ascii=True,
            separators=(",", ":"),
            sort_keys=True,
        ).encode("utf-8")
    ).hexdigest()
    return {
        "upstream_identical": len(upstream_identical),
        "science_only": len(science_only),
        "shared_diverged": len(shared_diverged),
        "missing_from_science": len(missing_from_science),
        "total_drift": total_drift,
        "drift_manifest_schema": DRIFT_MANIFEST_SCHEMA,
        "drift_manifest_sha256": manifest_sha256,
        "shared_diverged_paths": shared_diverged,
        "missing_from_science_paths": missing_from_science,
        "science_only_paths": science_only,
    }


def load_lock(path: Path) -> dict[str, Any]:
    if not path.is_file():
        fail(f"missing admission lock: {path}")
    with path.open(encoding="utf-8") as handle:
        data = json.load(handle)
    if not isinstance(data, dict):
        fail("admission lock root must be an object")
    return data


def expected_from_lock(lock: dict[str, Any]) -> dict[str, Any]:
    comparison = lock.get("comparison")
    if not isinstance(comparison, dict):
        fail("admission lock missing comparison object")
    source = lock.get("lumen_source")
    if not isinstance(source, dict):
        fail("admission lock missing lumen_source object")
    baseline = lock.get("science_baseline")
    if not isinstance(baseline, dict):
        fail("admission lock missing science_baseline object")

    for key in (
        "basis",
        "diverged_rust_files",
        "missing_in_science_rust_files",
        "science_only_rust_files",
        "total_drift",
        "drift_manifest_schema",
        "drift_manifest_sha256",
        "lumen_head",
    ):
        if key not in comparison:
            fail(f"admission lock comparison missing {key}")
    audited_main_head = str(source.get("audited_main_head") or "")
    if comparison["basis"] != "audited_lumen_main_head":
        fail("admission lock comparison basis must be audited_lumen_main_head")
    if str(comparison["lumen_head"]) != audited_main_head:
        fail(
            "admission lock comparison.lumen_head must equal "
            "lumen_source.audited_main_head"
        )

    return {
        "shared_diverged": int(comparison["diverged_rust_files"]),
        "missing_from_science": int(comparison["missing_in_science_rust_files"]),
        "science_only": int(comparison["science_only_rust_files"]),
        "total_drift": int(comparison["total_drift"]),
        "drift_manifest_schema": str(comparison["drift_manifest_schema"]),
        "drift_manifest_sha256": str(comparison["drift_manifest_sha256"]),
        "upstream_commit": str(comparison["lumen_head"]),
        "comparison_basis": str(comparison["basis"]),
        "target_tag_commit": str(source.get("target_tag_commit") or ""),
        "pinned_lumen_version": str(baseline.get("pinned_lumen_version") or ""),
        "strict_zero_drift_gate": bool(comparison.get("strict_zero_drift_gate", False)),
    }


def resolve_upstream_commit(upstream_root: Path) -> str:
    head = upstream_root / ".git" / "HEAD"
    # Prefer git rev-parse via reading .git when available without spawning if detached file
    git_dir = upstream_root / ".git"
    if not git_dir.exists():
        fail(f"upstream root is not a git checkout: {upstream_root}")
    # Use git when available
    import subprocess

    result = subprocess.run(
        ["git", "-C", str(upstream_root), "rev-parse", "HEAD"],
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        fail(f"cannot resolve upstream HEAD: {result.stderr.strip()}")
    return result.stdout.strip()


def verify_against_lock(
    result: dict[str, Any],
    expected: dict[str, Any],
    upstream_commit: str,
) -> None:
    if upstream_commit != expected["upstream_commit"]:
        fail(
            "upstream commit pin mismatch: "
            f"checkout={upstream_commit} lock.comparison.lumen_head={expected['upstream_commit']}"
        )
    if result["shared_diverged"] != expected["shared_diverged"]:
        fail(
            "shared_diverged count mismatch: "
            f"actual={result['shared_diverged']} lock={expected['shared_diverged']}"
        )
    if result["missing_from_science"] != expected["missing_from_science"]:
        fail(
            "missing_from_science count mismatch: "
            f"actual={result['missing_from_science']} lock={expected['missing_from_science']}"
        )
    if result["science_only"] != expected["science_only"]:
        fail(
            "science_only count mismatch: "
            f"actual={result['science_only']} lock={expected['science_only']}"
        )
    if result["total_drift"] != expected["total_drift"]:
        fail(
            "total_drift mismatch: "
            f"actual={result['total_drift']} lock={expected['total_drift']}"
        )
    if result["drift_manifest_schema"] != expected["drift_manifest_schema"]:
        fail(
            "drift manifest schema mismatch: "
            f"actual={result['drift_manifest_schema']} "
            f"lock={expected['drift_manifest_schema']}"
        )
    if result["drift_manifest_sha256"] != expected["drift_manifest_sha256"]:
        fail(
            "drift manifest digest mismatch: "
            f"actual={result['drift_manifest_sha256']} "
            f"lock={expected['drift_manifest_sha256']}; "
            "a drift path, classification, or file digest changed"
        )
    if expected["strict_zero_drift_gate"] and result["total_drift"] != 0:
        fail(f"strict_zero_drift_gate set but total_drift={result['total_drift']}")


def run_comparison(
    science_root: Path,
    upstream_root: Path,
    lock_path: Path | None,
) -> dict[str, Any]:
    science_files = collect_rust_files(science_root)
    upstream_files = collect_rust_files(upstream_root)
    result = classify(science_files, upstream_files)
    upstream_commit = resolve_upstream_commit(upstream_root)
    result["upstream_commit"] = upstream_commit
    result["science_root"] = str(science_root)
    result["upstream_root"] = str(upstream_root)

    if lock_path is not None:
        lock = load_lock(lock_path)
        expected = expected_from_lock(lock)
        result["lock"] = {
            "path": str(lock_path),
            "expected_shared_diverged": expected["shared_diverged"],
            "expected_missing_from_science": expected["missing_from_science"],
            "expected_science_only": expected["science_only"],
            "expected_total_drift": expected["total_drift"],
            "expected_drift_manifest_schema": expected["drift_manifest_schema"],
            "expected_drift_manifest_sha256": expected["drift_manifest_sha256"],
            "expected_upstream_commit": expected["upstream_commit"],
            "pinned_lumen_version": expected["pinned_lumen_version"],
        }
        verify_against_lock(result, expected, upstream_commit)
        result["lock_match"] = True
    return result


def run_self_test(tmp: Path) -> None:
    """Offline fixture tests — no real Lumen checkout required."""
    science = tmp / "science"
    upstream = tmp / "upstream"
    for root in (science, upstream):
        (root / "agent" / "crates" / "codegen" / "demo" / "src").mkdir(parents=True)
        (root / ".git").mkdir(parents=True)
        # Minimal git repo for rev-parse
        import subprocess

        subprocess.run(["git", "init"], cwd=root, check=True, capture_output=True)
        subprocess.run(
            ["git", "config", "user.email", "gate@example.com"],
            cwd=root,
            check=True,
            capture_output=True,
        )
        subprocess.run(
            ["git", "config", "user.name", "gate"],
            cwd=root,
            check=True,
            capture_output=True,
        )

    def write(root: Path, rel: str, body: str) -> None:
        path = root / "agent" / "crates" / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(body, encoding="utf-8")

    # identical
    write(science, "codegen/demo/src/lib.rs", "fn a() {}\n")
    write(upstream, "codegen/demo/src/lib.rs", "fn a() {}\n")
    # modified
    write(science, "codegen/demo/src/mod.rs", "fn b() { 1 }\n")
    write(upstream, "codegen/demo/src/mod.rs", "fn b() { 2 }\n")
    # missing from science
    write(upstream, "codegen/demo/src/only_up.rs", "fn up() {}\n")
    # science only
    write(science, "codegen/demo/src/only_sci.rs", "fn sci() {}\n")
    # excluded science crate (must not count)
    write(science, "codegen/xai-grok-science/src/lib.rs", "fn sci_lane() {}\n")
    write(upstream, "codegen/xai-grok-science/src/lib.rs", "fn other() {}\n")

    for root in (science, upstream):
        import subprocess

        subprocess.run(["git", "add", "-A"], cwd=root, check=True, capture_output=True)
        subprocess.run(
            ["git", "commit", "-m", "fixture"],
            cwd=root,
            check=True,
            capture_output=True,
        )

    commit = resolve_upstream_commit(upstream)
    result = classify(collect_rust_files(science), collect_rust_files(upstream))
    assert result["upstream_identical"] == 1, result
    assert result["shared_diverged"] == 1, result
    assert result["missing_from_science"] == 1, result
    assert result["science_only"] == 1, result
    assert result["total_drift"] == 2, result
    assert len(result["drift_manifest_sha256"]) == 64, result
    assert "codegen/xai-grok-science/src/lib.rs" not in result["shared_diverged_paths"]
    assert "codegen/xai-grok-science/src/lib.rs" not in result["science_only_paths"]

    # lock match
    lock = {
        "schema_version": 1,
        "science_baseline": {"pinned_lumen_version": "0.1.222"},
        "lumen_source": {
            "target_tag_commit": "f" * 40,
            "audited_main_head": commit,
        },
        "comparison": {
            "basis": "audited_lumen_main_head",
            "lumen_head": commit,
            "diverged_rust_files": 1,
            "missing_in_science_rust_files": 1,
            "science_only_rust_files": 1,
            "total_drift": 2,
            "drift_manifest_schema": result["drift_manifest_schema"],
            "drift_manifest_sha256": result["drift_manifest_sha256"],
            "strict_zero_drift_gate": False,
        },
    }
    lock_path = tmp / "lock.json"
    lock_path.write_text(json.dumps(lock), encoding="utf-8")
    ok = run_comparison(science, upstream, lock_path)
    assert ok["lock_match"] is True

    # lock count mismatch fails
    lock["comparison"]["total_drift"] = 99
    lock_path.write_text(json.dumps(lock), encoding="utf-8")
    try:
        run_comparison(science, upstream, lock_path)
        raise AssertionError("expected lock count mismatch")
    except DriftError as exc:
        assert "total_drift" in str(exc)

    # pin mismatch fails
    lock["comparison"]["total_drift"] = 2
    lock["comparison"]["lumen_head"] = "0" * 40
    lock["lumen_source"]["audited_main_head"] = "0" * 40
    lock_path.write_text(json.dumps(lock), encoding="utf-8")
    try:
        run_comparison(science, upstream, lock_path)
        raise AssertionError("expected pin mismatch")
    except DriftError as exc:
        assert "pin mismatch" in str(exc)

    # Equal-count substitution must fail. Revert the known divergent file and
    # diverge the formerly-identical file: shared_diverged remains 1 and all
    # aggregate counts still match, but the manifest identity changes.
    lock["comparison"]["lumen_head"] = commit
    lock["lumen_source"]["audited_main_head"] = commit
    lock_path.write_text(json.dumps(lock), encoding="utf-8")
    write(science, "codegen/demo/src/mod.rs", "fn b() { 2 }\n")
    write(science, "codegen/demo/src/lib.rs", "fn a() { 99 }\n")
    substituted = classify(collect_rust_files(science), collect_rust_files(upstream))
    assert substituted["shared_diverged"] == result["shared_diverged"], substituted
    assert substituted["total_drift"] == result["total_drift"], substituted
    try:
        run_comparison(science, upstream, lock_path)
        raise AssertionError("expected equal-count drift substitution failure")
    except DriftError as exc:
        assert "manifest digest mismatch" in str(exc)

    # symlink escape fails
    escape_root = tmp / "escape_sci"
    (escape_root / "agent" / "crates" / "codegen" / "demo" / "src").mkdir(parents=True)
    outside = tmp / "outside.rs"
    outside.write_text("fn evil() {}\n", encoding="utf-8")
    link = escape_root / "agent" / "crates" / "codegen" / "demo" / "src" / "evil.rs"
    try:
        link.symlink_to(outside)
    except OSError:
        print("self-test: symlink skipped on this platform")
    else:
        try:
            collect_rust_files(escape_root)
            raise AssertionError("expected symlink escape failure")
        except DriftError as exc:
            assert "escape" in str(exc).lower() or "outside" in str(exc).lower()

    print("check-core-drift self-test: ALL PASSED")


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument(
        "--science-root",
        type=Path,
        default=Path(__file__).resolve().parent.parent,
        help="Lumen Science repository root",
    )
    result.add_argument(
        "--upstream-root",
        type=Path,
        default=None,
        help="Exact audited Lumen checkout (required unless --self-test)",
    )
    result.add_argument(
        "--lock",
        type=Path,
        default=None,
        help="Admission lock JSON (default: docs/science/5.0/core-v0.1.251-admission.lock.json under science-root)",
    )
    result.add_argument(
        "--json-output",
        type=Path,
        default=None,
        help="Optional path to write full JSON report",
    )
    result.add_argument(
        "--self-test",
        action="store_true",
        help="Run offline fixture tests and exit",
    )
    result.add_argument(
        "--no-lock",
        action="store_true",
        help="Compare without enforcing admission lock counts",
    )
    return result


def main() -> int:
    args = parser().parse_args()
    try:
        if args.self_test:
            import tempfile

            with tempfile.TemporaryDirectory(prefix="core-drift-") as tmp:
                run_self_test(Path(tmp))
            return 0

        if args.upstream_root is None:
            fail("--upstream-root is required (or pass --self-test)")
        science_root = args.science_root.resolve()
        upstream_root = args.upstream_root.resolve()
        lock_path: Path | None
        if args.no_lock:
            lock_path = None
        elif args.lock is not None:
            lock_path = args.lock.resolve()
        else:
            lock_path = (
                science_root
                / "docs"
                / "science"
                / "5.0"
                / "core-v0.1.251-admission.lock.json"
            ).resolve()

        report = run_comparison(science_root, upstream_root, lock_path)
        summary = {
            "upstream_identical": report["upstream_identical"],
            "science_only": report["science_only"],
            "shared_diverged": report["shared_diverged"],
            "missing_from_science": report["missing_from_science"],
            "total_drift": report["total_drift"],
            "drift_manifest_schema": report["drift_manifest_schema"],
            "drift_manifest_sha256": report["drift_manifest_sha256"],
            "upstream_commit": report["upstream_commit"],
            "lock_match": report.get("lock_match"),
        }
        print(json.dumps(summary, indent=2, sort_keys=True))
        if args.json_output is not None:
            args.json_output.parent.mkdir(parents=True, exist_ok=True)
            args.json_output.write_text(
                json.dumps(report, indent=2, sort_keys=True) + "\n",
                encoding="utf-8",
            )
        return 0
    except DriftError as exc:
        print(f"FAIL: {exc}", file=sys.stderr)
        return 1
    except OSError as exc:
        print(f"FAIL: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
