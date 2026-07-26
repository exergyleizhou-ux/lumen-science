#!/usr/bin/env python3
"""Tamper tests for the dossier verifier.

A verifier that cannot detect tampering is worse than none: it converts "nobody
checked" into "it was checked and passed". So every check in verify-dossier.py
is tested by breaking exactly the thing it claims to detect, and asserting it
fails FOR THAT REASON — not merely that it failed, which any unrelated error
would also satisfy.

Each case restores the fixture afterwards and re-asserts a clean pass, so a
tamper that permanently corrupted the fixture cannot make later cases pass
vacuously.

    python3 scripts/test-verify-dossier.py

Exit 0 all pass, 1 otherwise. Stdlib only.
"""

from __future__ import annotations

import hashlib
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Callable

VERIFIER = Path(__file__).resolve().parent / "verify-dossier.py"

passed = 0
failures: list[str] = []


def check(label: str, condition: bool, detail: str = "") -> None:
    global passed
    if condition:
        passed += 1
        print(f"  ok    {label}")
    else:
        failures.append(label)
        print(f"  FAIL  {label}{f' — {detail}' if detail else ''}")


def build_fixture(root: Path) -> None:
    """A minimal but genuinely valid dossier: real bytes, real digests."""
    artifacts = root / "artifacts"
    artifacts.mkdir(parents=True, exist_ok=True)
    digests = {}
    for name, content in (
        ("input", b"col_a,col_b\n1,2\n"),
        ("result", b'{"mean": 1.5}\n'),
    ):
        digest = hashlib.sha256(content).hexdigest()
        (artifacts / digest).write_bytes(content)
        digests[name] = digest

    (artifacts / "manifest.json").write_text(
        json.dumps(
            {
                "artifacts": [
                    {"artifactId": "art-input", "sha256": digests["input"]},
                    {"artifactId": "art-result", "sha256": digests["result"]},
                ]
            },
            indent=2,
        )
    )
    (root / "evidence-graph.json").write_text(
        json.dumps(
            {
                "nodes": [
                    {"id": "n1", "sha256": digests["input"]},
                    {"id": "n2", "sha256": digests["result"]},
                    {"id": "claim1"},
                ],
                "edges": [{"from": "n1", "to": "n2"}, {"from": "n2", "to": "claim1"}],
            },
            indent=2,
        )
    )
    (root / "provenance.json").write_text(
        json.dumps(
            {
                "environment": {
                    "interpreter": "/usr/bin/python3",
                    "version": "3.11.9",
                    "sha256": "a" * 64,
                    "os": "darwin-arm64",
                },
                "policyHash": "b" * 64,
            },
            indent=2,
        )
    )
    (root / "replay-report.json").write_text(json.dumps({"outcome": "identical"}))
    (root / "dossier.md").write_text("# Research Dossier\n")


def verify(root: Path) -> dict:
    proc = subprocess.run(
        [sys.executable, str(VERIFIER), str(root), "--json"],
        capture_output=True,
        text=True,
    )
    try:
        return json.loads(proc.stdout)
    except json.JSONDecodeError:
        return {"verdict": "unreadable", "failed": [], "stderr": proc.stderr}


def edit_json(path: Path, mutate: Callable[[dict], None]) -> None:
    data = json.loads(path.read_text())
    mutate(data)
    path.write_text(json.dumps(data, indent=2))


def main() -> int:
    print("test-verify-dossier")
    with tempfile.TemporaryDirectory() as tmp:
        pristine = Path(tmp) / "pristine"
        work = Path(tmp) / "work"
        build_fixture(pristine)

        def reset() -> None:
            if work.exists():
                shutil.rmtree(work)
            shutil.copytree(pristine, work)

        # Control: a valid dossier must pass, or every tamper case below is
        # meaningless.
        reset()
        result = verify(work)
        check("a valid dossier passes", result["verdict"] == "pass", str(result.get("failed")))
        check(
            "a valid dossier still lists what it could not check",
            len(result.get("unverifiable", [])) > 0,
            "a verifier claiming to check everything is the untrustworthy kind",
        )

        cases: list[tuple[str, Callable[[Path], None], str]] = [
            (
                "one flipped byte in an artifact",
                lambda r: _append_byte(r),
                "artifact bytes match their recorded digest",
            ),
            (
                "a digest truncated to 16 hex",
                lambda r: edit_json(
                    r / "artifacts" / "manifest.json",
                    lambda d: d["artifacts"][0].__setitem__(
                        "sha256", d["artifacts"][0]["sha256"][:16]
                    ),
                ),
                "artifact digests are canonical 64-hex",
            ),
            (
                "an edge pointing at a node that does not exist",
                lambda r: edit_json(
                    r / "evidence-graph.json",
                    lambda d: d["edges"].append({"from": "n2", "to": "ghost"}),
                ),
                "every edge resolves to a node",
            ),
            (
                "a derivation cycle",
                lambda r: edit_json(
                    r / "evidence-graph.json",
                    lambda d: d["edges"].append({"from": "claim1", "to": "n1"}),
                ),
                "no derivation cycle",
            ),
            (
                "a self-edge",
                lambda r: edit_json(
                    r / "evidence-graph.json",
                    lambda d: d["edges"].append({"from": "n1", "to": "n1"}),
                ),
                "no self-edges",
            ),
            (
                "environment version recorded as 'unknown'",
                lambda r: edit_json(
                    r / "provenance.json",
                    lambda d: d["environment"].__setitem__("version", "unknown"),
                ),
                "environment identity is recorded, not placeheld",
            ),
        ]

        for label, tamper, expected_check in cases:
            reset()
            tamper(work)
            result = verify(work)
            failed_checks = [f["check"] for f in result.get("failed", [])]
            check(
                f"detects: {label}",
                result["verdict"] == "fail" and expected_check in failed_checks,
                f"verdict={result['verdict']} failed={failed_checks}",
            )

        # The fixture must still be good, or the cases above proved nothing.
        reset()
        check("fixture is restorable to a passing state", verify(work)["verdict"] == "pass")

        # A directory that is not a dossier must be refused, not silently passed.
        empty = Path(tmp) / "empty"
        empty.mkdir()
        proc = subprocess.run(
            [sys.executable, str(VERIFIER), str(empty)], capture_output=True, text=True
        )
        check("refuses a directory that is not a dossier", proc.returncode == 2)

    if failures:
        print(f"\nFAILED: {len(failures)} of {passed + len(failures)}", file=sys.stderr)
        return 1
    print(f"\nALL TAMPER TESTS PASSED ({passed} checks)")
    return 0


def _append_byte(root: Path) -> None:
    manifest = json.loads((root / "artifacts" / "manifest.json").read_text())
    digest = manifest["artifacts"][1]["sha256"]
    blob = root / "artifacts" / digest
    blob.write_bytes(blob.read_bytes() + b"X")


if __name__ == "__main__":
    raise SystemExit(main())
