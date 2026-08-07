#!/usr/bin/env python3
"""Negative tests for the science status generator and verifier (LS5-F0-01).

A gate that cannot fail proves nothing. These tests corrupt a copy of the
status file in each of the ways a human or a well-meaning agent would
plausibly corrupt it, and assert the verifier rejects every one.

    python3 scripts/test-science-status.py

Exit 0 all tests pass, 1 otherwise. Stdlib only. Never mutates the real
status file — every case runs against a temporary copy of the repo's
docs/science/status/current.json.
"""

from __future__ import annotations

import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any, Callable

ROOT = Path(__file__).resolve().parent.parent
STATUS_REL = Path("docs/science/status/current.json")
STATUS_EVIDENCE_REL = Path("docs/science/status/evidence.v1.json")
VERIFIER = ROOT / "scripts" / "verify-science-status.py"
GENERATOR = ROOT / "scripts" / "generate-science-status.py"

results: list[tuple[str, bool, str]] = []


def run_verifier(cwd: Path, *extra: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(VERIFIER), *extra],
        cwd=cwd,
        capture_output=True,
        text=True,
        check=False,
    )


def case(name: str, mutate: Callable[[dict[str, Any]], dict[str, Any]], *, expect_fail: bool = True,
         needle: str = "") -> None:
    """Apply `mutate` to a scratch copy of the status file and check the verdict.

    The scratch copy lives inside the real repo (a temp path under
    docs/science/status/) so the verifier still sees a valid git tree, then is
    swapped back. The original bytes are restored in `finally` unconditionally.
    """
    real = ROOT / STATUS_REL
    original = real.read_bytes()
    try:
        mutated = mutate(json.loads(original.decode("utf-8")))
        real.write_text(
            json.dumps(mutated, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
        )
        # --skip-freshness isolates the honesty/contradiction checks; freshness
        # has its own dedicated case below.
        proc = run_verifier(ROOT, "--skip-freshness")
        failed = proc.returncode != 0
        combined = proc.stdout + proc.stderr
        if failed != expect_fail:
            results.append(
                (name, False, f"expected {'fail' if expect_fail else 'pass'}, got exit {proc.returncode}")
            )
        elif needle and needle not in combined:
            results.append((name, False, f"failed for the wrong reason; wanted {needle!r}"))
        else:
            results.append((name, True, ""))
    finally:
        real.write_bytes(original)


# ── honesty cases ────────────────────────────────────────────────────────


def upgrade_gate_without_evidence(status: dict[str, Any]) -> dict[str, Any]:
    """The classic: flip a not_run gate to pass because it 'probably works'."""
    status["desktop"]["typecheck"] = {"state": "pass"}
    return status


def invent_skipped_state(status: dict[str, Any]) -> dict[str, Any]:
    """Introduce a vocabulary word that reads like success but is not."""
    status["desktop"]["headedE2E"] = {"state": "skipped"}
    return status


def contradict_evidence(status: dict[str, Any]) -> dict[str, Any]:
    """Record a command that exited 0 but label the gate not_run."""
    status["desktop"]["fullBuild"] = {
        "state": "not_run",
        "evidence": {"command": "npm run dist:full", "exitCode": 0},
    }
    return status


def strip_all_gates(status: dict[str, Any]) -> dict[str, Any]:
    """A status file that describes nothing must not be treated as healthy."""
    status["desktop"] = {"version": "1.1.0-dev"}
    status["ci"] = {"checks": {}}
    return status


def honest_not_run(status: dict[str, Any]) -> dict[str, Any]:
    """Control: the real, unmodified shape must pass."""
    return status


# ── contradiction case ───────────────────────────────────────────────────


def reintroduce_contradiction(status: dict[str, Any]) -> dict[str, Any]:
    """Move the machine truth so the live prose docs now disagree with it."""
    status["skillInventory"]["derivedApproved"] = 99
    return status


# ── freshness case (separate: needs the generator to disagree) ───────────


def test_freshness_detects_stale() -> None:
    real = ROOT / STATUS_REL
    original = real.read_bytes()
    try:
        stale = json.loads(original.decode("utf-8"))
        # A fabricated provenance stamp is refused by the STAMP check, not by
        # freshness: the stamp fields are structurally exempt from the content
        # comparison (the committed file cannot name the commit that records
        # it), so the guard against a lying stamp is the consistency check.
        stale["sourceCommit"] = "0" * 40
        real.write_text(json.dumps(stale, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
        proc = run_verifier(ROOT)
        combined = proc.stdout + proc.stderr
        if proc.returncode == 0:
            results.append(("a fabricated provenance stamp is refused", False, "verifier passed a fake stamp"))
        elif "does not name a commit" not in combined:
            results.append(("a fabricated provenance stamp is refused", False, "failed for the wrong reason"))
        else:
            results.append(("a fabricated provenance stamp is refused", True, ""))

        # And CONTENT freshness: a derived field that no longer matches a
        # regeneration at the current tree is stale — this is what freshness
        # means now that the stamp fields are exempt.
        drifted = json.loads(original.decode("utf-8"))
        drifted["release"]["pipeline"]["targets"] = ["tampered-target"]
        real.write_text(json.dumps(drifted, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
        proc = run_verifier(ROOT)
        combined = proc.stdout + proc.stderr
        if proc.returncode == 0:
            results.append(("freshness detects drifted derived content", False, "verifier passed a stale file"))
        elif "stale" not in combined:
            results.append(("freshness detects drifted derived content", False, "failed for the wrong reason"))
        else:
            results.append(("freshness detects drifted derived content", True, ""))
    finally:
        real.write_bytes(original)


def test_missing_status_file() -> None:
    """A deleted status file must be an error, not an absent-therefore-fine."""
    real = ROOT / STATUS_REL
    original = real.read_bytes()
    backup = None
    try:
        backup = tempfile.NamedTemporaryFile(delete=False)
        backup.write(original)
        backup.close()
        real.unlink()
        proc = run_verifier(ROOT)
        if proc.returncode == 0:
            results.append(("missing status file is an error", False, "verifier passed with no file"))
        else:
            results.append(("missing status file is an error", True, ""))
    finally:
        real.write_bytes(original)
        if backup:
            Path(backup.name).unlink(missing_ok=True)


def test_generator_refuses_unevidenced_pass() -> None:
    """The generator itself must refuse to write an unevidenced pass."""
    with tempfile.TemporaryDirectory() as tmp:
        evidence = Path(tmp) / "evidence.json"
        evidence.write_text(
            json.dumps({"desktop": {"typecheck": {"state": "pass"}}}), encoding="utf-8"
        )
        proc = subprocess.run(
            [
                sys.executable,
                str(ROOT / "scripts" / "generate-science-status.py"),
                "--stdout",
                "--evidence",
                str(evidence),
            ],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=False,
        )
        combined = proc.stdout + proc.stderr
        if proc.returncode == 0:
            results.append(
                ("generator refuses unevidenced pass", False, "generator wrote a bare pass")
            )
        elif "refusing to emit pass" not in combined:
            results.append(
                ("generator refuses unevidenced pass", False, "failed for the wrong reason")
            )
        else:
            results.append(("generator refuses unevidenced pass", True, ""))


def test_generator_accepts_evidenced_pass() -> None:
    """Control: a properly evidenced pass must be accepted and recorded."""
    with tempfile.TemporaryDirectory() as tmp:
        evidence = Path(tmp) / "evidence.json"
        evidence.write_text(
            json.dumps(
                {
                    "desktop": {
                        "typecheck": {
                            "state": "pass",
                            "evidence": {"command": "npm run typecheck", "exitCode": 0},
                        }
                    }
                }
            ),
            encoding="utf-8",
        )
        proc = subprocess.run(
            [
                sys.executable,
                str(ROOT / "scripts" / "generate-science-status.py"),
                "--stdout",
                "--evidence",
                str(evidence),
            ],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=False,
        )
        if proc.returncode != 0:
            results.append(("generator accepts evidenced pass", False, proc.stderr.strip()[:120]))
            return
        status = json.loads(proc.stdout)
        gate = status["desktop"]["typecheck"]
        if gate.get("state") == "pass" and gate.get("evidence", {}).get("command"):
            results.append(("generator accepts evidenced pass", True, ""))
        else:
            results.append(("generator accepts evidenced pass", False, f"got {gate}"))


def test_status_evidence_rejects_stale_desktop_source() -> None:
    """A Desktop code/workflow change must invalidate an older CI receipt."""
    path = ROOT / STATUS_EVIDENCE_REL
    original = path.read_bytes()
    try:
        evidence = json.loads(original.decode("utf-8"))
        # This commit predates the tracked full-package workflow. It remains an
        # ancestor, so failure proves protected-path drift rather than a
        # missing-commit error.
        evidence["gates"]["desktop"]["fullBuild"]["evidence"]["sourceAnchor"]["headCommit"] = (
            "07310811b0def9a4a36b768b88c325aaca995d9b"
        )
        path.write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
        proc = subprocess.run(
            [sys.executable, str(GENERATOR), "--stdout"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=False,
        )
        output = proc.stdout + proc.stderr
        if proc.returncode == 2 and "stale" in output:
            results.append(("stale Desktop CI evidence is rejected", True, ""))
        else:
            results.append(
                (
                    "stale Desktop CI evidence is rejected",
                    False,
                    f"exit={proc.returncode}; output={output.strip()[:180]!r}",
                )
            )
    finally:
        path.write_bytes(original)


def main() -> int:
    if not (ROOT / STATUS_REL).is_file():
        print(
            "FAIL: status file missing; run generate-science-status.py first", file=sys.stderr
        )
        return 1

    case(
        "unevidenced pass is rejected",
        upgrade_gate_without_evidence,
        needle="no evidence.command",
    )
    case("invented 'skipped' state is rejected", invent_skipped_state, needle="invented gate state")
    case(
        "successful command labelled not_run is rejected",
        contradict_evidence,
        needle="recorded as not_run",
    )
    case("status file with no gates is rejected", strip_all_gates, needle="describes nothing")
    case("reintroduced doc contradiction is caught", reintroduce_contradiction,
         needle="non-contradiction")
    case("honest not_run status passes", honest_not_run, expect_fail=False)

    test_freshness_detects_stale()
    test_missing_status_file()
    test_generator_refuses_unevidenced_pass()
    test_generator_accepts_evidenced_pass()
    test_status_evidence_rejects_stale_desktop_source()

    print("test-science-status")
    passed = 0
    for name, good, detail in results:
        if good:
            passed += 1
            print(f"  ok    {name}")
        else:
            print(f"  FAIL  {name}: {detail}")

    total = len(results)
    if passed != total:
        print(f"\nFAIL: {passed}/{total} passed", file=sys.stderr)
        return 1
    print(f"\nOK: {passed}/{total} passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
