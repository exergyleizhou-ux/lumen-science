#!/usr/bin/env python3
"""Verify the committed Lumen Science status file (LS5-F0-01).

Three independent checks:

1. FRESHNESS — regenerate from the current tree and diff against the committed
   docs/science/status/current.json. Any drift fails, so a status file cannot
   quietly rot behind the source it describes.

2. HONESTY — re-assert the invariants the generator promises:
   a gate may only be ``pass`` when it carries evidence naming a command, and
   ``not_run`` may never be presented as success. This runs against the
   committed bytes, so hand-editing the file to upgrade a gate is caught here
   even though the generator would have refused to write it.

3. NON-CONTRADICTION — prose documents must not restate machine facts with
   different values. Documents may cite a number if it agrees with the
   generated status; disagreeing is an error and the fix is to point at
   current.json rather than to re-copy the number.

Exit codes: 0 all checks pass, 1 a check failed, 2 the tree could not be read.

    python3 scripts/verify-science-status.py
    python3 scripts/verify-science-status.py --skip-freshness   # local WIP

Stdlib only.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parent.parent
STATUS_PATH = ROOT / "docs" / "science" / "status" / "current.json"
GENERATOR = ROOT / "scripts" / "generate-science-status.py"

failures: list[str] = []
checks_run = 0


def fail(check: str, detail: str) -> None:
    failures.append(f"{check}: {detail}")


def ok(check: str, detail: str = "") -> None:
    suffix = f" — {detail}" if detail else ""
    print(f"  ok    {check}{suffix}")


# ── 1. freshness ─────────────────────────────────────────────────────────


def check_freshness(committed: dict[str, Any]) -> None:
    global checks_run
    checks_run += 1
    result = subprocess.run(
        [sys.executable, str(GENERATOR), "--stdout"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        fail("freshness", f"generator failed: {result.stderr.strip()}")
        return
    try:
        regenerated = json.loads(result.stdout)
    except json.JSONDecodeError as exc:
        fail("freshness", f"generator emitted invalid JSON: {exc}")
        return

    # Gate results come from CI evidence that is not present locally, so compare
    # only the source-derived surface. Those gates are covered by check 2.
    #
    # The commit-identity stamps are volatile too, and structurally so: the
    # committed file necessarily names the HEAD at generation time, and the
    # commit that records it creates a NEW head. Comparing them meant no commit
    # that changed anything else could ever be fresh — this gate was red on
    # every CI run of the branch while passing locally before each commit,
    # which is the exact split a freshness check exists to prevent. What
    # freshness MEANS is that the derived content matches a regeneration at
    # the current tree; the stamp records provenance and is checked for
    # internal consistency below (the named commit must exist and its
    # committer time must match the recorded epoch), not for equality with a
    # head it cannot know.
    volatile = {"ci", "desktop", "sourceCommit", "sourceCommitEpoch", "sourceCommitIso"}
    drifted = [
        key
        for key in set(committed) | set(regenerated)
        if key not in volatile and committed.get(key) != regenerated.get(key)
    ]
    if drifted:
        fail(
            "freshness",
            "current.json is stale for: "
            + ", ".join(sorted(drifted))
            + " — regenerate with: python3 scripts/generate-science-status.py",
        )
        return
    ok("freshness", f"matches tree at {committed.get('sourceCommit', '?')[:12]}")


def check_stamp_consistency(committed: dict[str, Any]) -> None:
    """The provenance stamp must describe a real commit, truthfully."""
    global checks_run
    checks_run += 1
    stamp = str(committed.get("sourceCommit", ""))
    if not stamp or len(stamp) < 12:
        fail("stamp", "sourceCommit is missing or too short to identify a commit")
        return
    result = subprocess.run(
        ["git", "show", "-s", "--format=%ct", stamp],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        fail("stamp", f"sourceCommit {stamp[:12]} does not name a commit in this repository")
        return
    actual_epoch = int(result.stdout.strip())
    claimed = committed.get("sourceCommitEpoch")
    if claimed != actual_epoch:
        fail(
            "stamp",
            f"sourceCommitEpoch {claimed} does not match commit {stamp[:12]}'s "
            f"committer time {actual_epoch} — the stamp lies about when it was derived",
        )
        return
    ok("stamp", f"provenance stamp names real commit {stamp[:12]}")


# ── 2. honesty ───────────────────────────────────────────────────────────


def walk_gates(node: Any, path: str = "") -> list[tuple[str, dict[str, Any]]]:
    """Find every gate record (a dict with a "state" in the gate vocabulary)."""
    found: list[tuple[str, dict[str, Any]]] = []
    if isinstance(node, dict):
        if isinstance(node.get("state"), str) and node["state"] in ("pass", "fail", "not_run"):
            found.append((path, node))
        for key, value in node.items():
            found.extend(walk_gates(value, f"{path}.{key}" if path else key))
    elif isinstance(node, list):
        for index, value in enumerate(node):
            found.extend(walk_gates(value, f"{path}[{index}]"))
    return found


def check_honesty(committed: dict[str, Any]) -> None:
    global checks_run
    checks_run += 1
    gates = walk_gates(committed)
    if not gates:
        fail("honesty", "no gate records found — the status file describes nothing")
        return

    bad: list[str] = []
    for path, gate in gates:
        state = gate["state"]
        evidence = gate.get("evidence") or {}
        if state == "pass" and not evidence.get("command"):
            bad.append(f"{path} claims pass with no evidence.command")
        if state != "pass" and evidence.get("command") and evidence.get("exitCode") == 0:
            bad.append(f"{path} has a successful command but is recorded as {state}")

    # An unknown state anywhere means someone invented new vocabulary, e.g.
    # "skipped" being treated as success.
    raw = STATUS_PATH.read_text(encoding="utf-8")
    for invented in ("skipped", "partial", "probably", "assumed"):
        if re.search(rf'"state"\s*:\s*"{invented}"', raw):
            bad.append(f'invented gate state "{invented}" — use pass/fail/not_run')

    if bad:
        for item in bad:
            fail("honesty", item)
        return
    passing = sum(1 for _, g in gates if g["state"] == "pass")
    ok("honesty", f"{len(gates)} gates, {passing} pass (all evidenced)")


# ── 3. non-contradiction ─────────────────────────────────────────────────

# Documents that historically restated machine facts. Each rule names a value
# from the generated status and a regex whose captured group must equal it.
def build_rules(status: dict[str, Any]) -> list[dict[str, Any]]:
    skills = status["skillInventory"]
    connectors = status["connectorInventory"]
    versions = status["versions"]
    return [
        {
            "name": "skills approved count",
            "expected": str(skills["derivedApproved"]),
            # Both orderings appear in the docs: `approved=5` and `5 approved / 22 pending`.
            "pattern": r"approved\s*=\s*(\d+)|(?:\*\*)?(\d+)(?:\*\*)?\s+approved\s*/\s*\d+\s+pending",
            "docs": ["docs/science/**/*.md", "docs/science/*.md"],
        },
        {
            "name": "connector rejected count",
            "expected": str(connectors["rejected"]),
            "pattern": r"(?:\*\*)?(\d+)(?:\*\*)?\s+rejected",
            "docs": ["docs/science/**/*.md", "docs/science/*.md"],
        },
        {
            "name": "science CLI version",
            "expected": versions["cliVersion"]["value"],
            "pattern": r"Lumen Science CLI/MCP[^\n|]*\|[^|]*\|\s*`?v?(\d+\.\d+\.\d+)`?",
            "docs": ["docs/VERSIONING.md"],
        },
    ]


# A document that records what was true at an earlier milestone is legitimate,
# but it must say so in its own bytes. Without this marker a stale number is
# indistinguishable from a wrong one, which is how the repo got here.
HISTORICAL_MARKER = "status-claim: historical"


def check_contradictions(status: dict[str, Any]) -> None:
    global checks_run
    checks_run += 1
    conflicts: list[str] = []
    exempt = 0
    seen: set[Path] = set()
    for rule in build_rules(status):
        expected = rule["expected"]
        if expected is None:
            continue
        for glob in rule["docs"]:
            for path in sorted(ROOT.glob(glob)):
                if not path.is_file():
                    continue
                text = path.read_text(encoding="utf-8", errors="replace")
                if HISTORICAL_MARKER in text:
                    if path not in seen:
                        seen.add(path)
                        exempt += 1
                    continue
                for match in re.finditer(rule["pattern"], text, re.I):
                    # Alternation: exactly one group matches per hit.
                    found = next((g for g in match.groups() if g is not None), None)
                    if found is not None and found != expected:
                        rel = path.relative_to(ROOT)
                        line = text[: match.start()].count("\n") + 1
                        conflicts.append(
                            f'{rel}:{line} states {rule["name"]}={found}, '
                            f"generated status says {expected} "
                            f"(fix the number, or mark the doc `{HISTORICAL_MARKER}`)"
                        )
    if conflicts:
        for item in sorted(set(conflicts)):
            fail("non-contradiction", item)
        return
    ok(
        "non-contradiction",
        f"no live document contradicts the generated status ({exempt} marked historical)",
    )


# ── main ─────────────────────────────────────────────────────────────────


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--skip-freshness",
        action="store_true",
        help="skip regeneration diff (local work-in-progress only; CI must not use this)",
    )
    args = parser.parse_args()

    if not STATUS_PATH.is_file():
        print(
            f"FAIL: {STATUS_PATH.relative_to(ROOT)} missing — "
            "run: python3 scripts/generate-science-status.py",
            file=sys.stderr,
        )
        return 2

    try:
        committed = json.loads(STATUS_PATH.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        print(f"FAIL: status file is not valid JSON: {exc}", file=sys.stderr)
        return 2

    print("verify-science-status")
    if args.skip_freshness:
        print("  skip  freshness (--skip-freshness)")
    else:
        check_freshness(committed)
        check_stamp_consistency(committed)
    check_honesty(committed)
    check_contradictions(committed)

    if failures:
        print(f"\nFAIL ({len(failures)} problem(s)):", file=sys.stderr)
        for item in failures:
            print(f"  - {item}", file=sys.stderr)
        return 1
    print(f"\nOK: {checks_run} check(s) passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
