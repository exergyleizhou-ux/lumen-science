#!/usr/bin/env python3
"""Independently verify a Lumen Science research dossier.

This script is the point of the product.

A dossier that can only be checked by the tool that produced it proves nothing:
the reader is trusting the claim and the claimant at once. So this verifier is
built to be run by someone who has no reason to trust us —

  - stdlib only, no install, no network, no Lumen
  - recomputes every digest from the bytes on disk; the package's own
    statements about itself are treated as claims to be tested, never as facts
  - ships inside the dossier, so a reader has it without asking us for it
  - ENUMERATES WHAT IT CANNOT CHECK, which is the part most verification tools
    leave out and the part that decides whether the rest can be believed

Usage:
    python3 verify-dossier.py <dossier-directory>
    python3 verify-dossier.py <dossier-directory> --json

Exit codes:
    0  every check performed passed
    1  at least one check failed
    2  the dossier could not be read at all

A pass means: the bytes are internally consistent and self-consistent with the
recorded claims. It does NOT mean the science is right. See "What this cannot
tell you" in the output.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path
from typing import Any

CANONICAL_DIGEST = re.compile(r"^[0-9a-f]{64}$")

# Values that look like an answer but are not one. A dossier carrying these has
# recorded a placeholder where a fact belongs, and reporting it as verified
# would launder the placeholder into evidence.
PLACEHOLDERS = {"", "unknown", "n/a", "none", "null", "todo", "tbd", "-"}

REQUIRED_FILES = (
    "dossier.md",
    "evidence-graph.json",
    "provenance.json",
    "artifacts/manifest.json",
)


class Report:
    def __init__(self) -> None:
        self.passed: list[str] = []
        self.failed: list[tuple[str, str]] = []
        self.unverifiable: list[str] = []

    def ok(self, label: str) -> None:
        self.passed.append(label)

    def fail(self, label: str, detail: str) -> None:
        self.failed.append((label, detail))

    def cannot(self, label: str) -> None:
        self.unverifiable.append(label)


def load_json(root: Path, rel: str, report: Report) -> Any | None:
    path = root / rel
    if not path.is_file():
        report.fail(f"{rel} present", "file missing")
        return None
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        report.fail(f"{rel} parses", str(exc))
        return None


def check_artifacts(root: Path, report: Report) -> dict[str, str]:
    """Recompute every artifact digest from bytes. Returns digest -> artifact id."""
    manifest = load_json(root, "artifacts/manifest.json", report)
    if manifest is None:
        return {}

    entries = manifest if isinstance(manifest, list) else manifest.get("artifacts", [])
    if not isinstance(entries, list) or not entries:
        report.fail("manifest lists artifacts", "no artifact entries found")
        return {}

    known: dict[str, str] = {}
    bad_digest: list[str] = []
    mismatched: list[str] = []
    absent: list[str] = []

    for entry in entries:
        if not isinstance(entry, dict):
            continue
        artifact_id = str(entry.get("artifactId") or entry.get("artifact_id") or "?")
        claimed = str(entry.get("sha256") or entry.get("digest") or "")

        if not CANONICAL_DIGEST.match(claimed):
            # A short or uppercase digest is not merely untidy: truncation is
            # how two distinct artifacts collapse onto one identity.
            bad_digest.append(f"{artifact_id}: {claimed!r}")
            continue

        # The bytes may be stored by digest or by id; try both before concluding
        # the artifact was not shipped.
        candidates = [
            root / "artifacts" / claimed,
            root / "artifacts" / f"{claimed}.bin",
            root / "artifacts" / artifact_id,
        ]
        blob = next((p for p in candidates if p.is_file()), None)
        if blob is None:
            absent.append(artifact_id)
            continue

        actual = hashlib.sha256(blob.read_bytes()).hexdigest()
        if actual != claimed:
            mismatched.append(f"{artifact_id}: claims {claimed[:16]}…, bytes are {actual[:16]}…")
            continue
        known[claimed] = artifact_id

    if bad_digest:
        report.fail("artifact digests are canonical 64-hex", "; ".join(bad_digest[:5]))
    else:
        report.ok(f"artifact digests are canonical 64-hex ({len(entries)} entries)")

    if mismatched:
        report.fail("artifact bytes match their recorded digest", "; ".join(mismatched[:5]))
    elif known:
        report.ok(f"artifact bytes match their recorded digest ({len(known)} re-hashed)")

    if absent:
        # Not a failure: a dossier may reference artifacts too large to ship.
        # But it bounds what was actually checked, so it must be said.
        report.cannot(
            f"{len(absent)} artifact(s) are referenced but their bytes are not in the "
            f"package, so their digests could not be recomputed: {', '.join(absent[:5])}"
        )
    return known


def check_evidence_graph(root: Path, known: dict[str, str], report: Report) -> None:
    graph = load_json(root, "evidence-graph.json", report)
    if graph is None:
        return

    nodes = graph.get("nodes", []) if isinstance(graph, dict) else []
    edges = graph.get("edges", []) if isinstance(graph, dict) else []
    node_ids = {str(n.get("id")) for n in nodes if isinstance(n, dict)}

    dangling = [
        f"{e.get('from')} -> {e.get('to')}"
        for e in edges
        if isinstance(e, dict)
        and (str(e.get("from")) not in node_ids or str(e.get("to")) not in node_ids)
    ]
    if dangling:
        report.fail("every edge resolves to a node", "; ".join(dangling[:5]))
    else:
        report.ok(f"every edge resolves to a node ({len(edges)} edges, {len(nodes)} nodes)")

    self_edges = [
        str(e.get("from"))
        for e in edges
        if isinstance(e, dict) and e.get("from") == e.get("to")
    ]
    if self_edges:
        report.fail("no self-edges", "; ".join(self_edges[:5]))
    else:
        report.ok("no self-edges")

    # A derivation cycle means the graph claims a result is its own ancestor.
    adjacency: dict[str, list[str]] = {}
    for e in edges:
        if isinstance(e, dict):
            adjacency.setdefault(str(e.get("from")), []).append(str(e.get("to")))

    colour: dict[str, int] = {}
    cycle: list[str] = []

    def visit(node: str, path: list[str]) -> bool:
        colour[node] = 1
        for nxt in adjacency.get(node, []):
            if colour.get(nxt) == 1:
                cycle.extend(path + [node, nxt])
                return True
            if colour.get(nxt, 0) == 0 and visit(nxt, path + [node]):
                return True
        colour[node] = 2
        return False

    has_cycle = any(visit(n, []) for n in list(adjacency) if colour.get(n, 0) == 0)
    if has_cycle:
        report.fail("no derivation cycle", " -> ".join(cycle[:8]))
    else:
        report.ok("no derivation cycle")

    # Every artifact the graph cites must be one whose bytes we re-hashed.
    cited = {
        str(n.get("sha256") or n.get("digest"))
        for n in nodes
        if isinstance(n, dict) and (n.get("sha256") or n.get("digest"))
    }
    uncited = sorted(d for d in cited if d not in known and CANONICAL_DIGEST.match(d or ""))
    if uncited:
        report.cannot(
            f"{len(uncited)} digest(s) cited by the evidence graph were not verifiable "
            f"against shipped bytes: {', '.join(d[:16] + '…' for d in uncited[:3])}"
        )
    elif cited:
        report.ok(f"every cited digest was re-hashed from shipped bytes ({len(cited)})")


def check_provenance(root: Path, report: Report) -> None:
    prov = load_json(root, "provenance.json", report)
    if prov is None:
        return

    # Environment identity is what makes a reproducibility claim mean anything.
    # A recorded "unknown" is worse than an absent field: it looks answered.
    env = prov.get("environment") if isinstance(prov, dict) else None
    if not isinstance(env, dict):
        report.cannot("no environment block recorded, so the run cannot be reproduced from it")
    else:
        placeholder = [
            f"{k}={v!r}"
            for k, v in env.items()
            if isinstance(v, str) and v.strip().lower() in PLACEHOLDERS
        ]
        if placeholder:
            report.fail(
                "environment identity is recorded, not placeheld",
                "; ".join(placeholder[:5]),
            )
        else:
            report.ok(f"environment identity is recorded ({len(env)} fields)")

    if isinstance(prov, dict):
        for field in ("policyHash", "policy_hash"):
            value = prov.get(field)
            if isinstance(value, str) and CANONICAL_DIGEST.match(value):
                report.ok("policy hash is a canonical digest")
                break
        else:
            report.cannot("no canonical policy hash recorded; the governing policy is unpinned")


def check_replay(root: Path, report: Report) -> None:
    path = root / "replay-report.json"
    if not path.is_file():
        report.cannot("no replay report shipped, so replay was not attempted or not recorded")
        return
    try:
        replay = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        report.fail("replay-report.json parses", str(exc))
        return

    if not isinstance(replay, dict):
        report.fail("replay report is an object", f"got {type(replay).__name__}")
        return

    # The report states an outcome; we can check it is stated unambiguously,
    # but a claim of "identical" made by the producer is not evidence of
    # identity — only re-running is, and that needs the environment.
    outcome = str(replay.get("outcome") or replay.get("result") or "").strip().lower()
    if outcome in PLACEHOLDERS:
        report.fail("replay report states an outcome", f"outcome={outcome!r}")
    else:
        report.ok(f"replay report states an outcome ({outcome})")
    report.cannot(
        "the replay claim itself was not independently re-executed; verifying it "
        "requires reconstructing the recorded environment and re-running"
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("dossier", type=Path)
    parser.add_argument("--json", action="store_true", help="machine-readable output")
    args = parser.parse_args()

    root = args.dossier
    if not root.is_dir():
        print(f"FAIL: not a directory: {root}", file=sys.stderr)
        return 2

    report = Report()

    missing = [f for f in REQUIRED_FILES if not (root / f).is_file()]
    if missing:
        print(f"FAIL: not a dossier — missing {', '.join(missing)}", file=sys.stderr)
        return 2

    known = check_artifacts(root, report)
    check_evidence_graph(root, known, report)
    check_provenance(root, report)
    check_replay(root, report)

    # Always stated, never conditional: these are the limits of ANY static
    # check of a package, and a reader who does not know them will over-trust
    # a pass.
    report.cannot("whether the analysis is scientifically sound — no tool can check that")
    report.cannot(
        "whether the recorded environment is the one that actually ran; the package "
        "records identity, it cannot prove the identity was honest"
    )
    report.cannot(
        "who produced this package — that requires verifying a signature against a "
        "public key obtained separately, not from inside the package"
    )

    if args.json:
        print(json.dumps({
            "passed": report.passed,
            "failed": [{"check": c, "detail": d} for c, d in report.failed],
            "unverifiable": report.unverifiable,
            "verdict": "fail" if report.failed else "pass",
        }, indent=2))
        return 1 if report.failed else 0

    print(f"verify-dossier {root}\n")
    for label in report.passed:
        print(f"  ok        {label}")
    for label, detail in report.failed:
        print(f"  FAIL      {label} — {detail}")
    print()
    print("What this cannot tell you:")
    for label in report.unverifiable:
        print(f"  unchecked {label}")

    print()
    if report.failed:
        print(f"VERDICT: FAIL — {len(report.failed)} of "
              f"{len(report.passed) + len(report.failed)} checks failed")
        return 1
    print(f"VERDICT: PASS — {len(report.passed)} checks passed, "
          f"{len(report.unverifiable)} things left unchecked and listed above")
    print("\nA pass means the bytes are internally consistent and match their recorded")
    print("claims. It does not mean the science is right.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
