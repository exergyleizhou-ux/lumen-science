#!/usr/bin/env python3
"""Focused tamper tests for the Motif capability intake record."""

from __future__ import annotations

import copy
import importlib.util
import json
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
SCRIPT = ROOT / "scripts/verify-capability-intake.py"
RECORD = ROOT / "third_party/capability-intake/jvogan-motif/seq-analyze.v1.json"
BIOMNI_RECORD = ROOT / "third_party/capability-intake/snap-stanford-biomni/query-uniprot.v1.json"
AIPOCH_RECORD = ROOT / "third_party/capability-intake/aipoch-open-science/skill-archive-preview.v1.json"
AIPOCH_QUARANTINE_RECORD = ROOT / "third_party/capability-intake/aipoch-open-science/skill-quarantine-import.v1.json"
PRIMER_RECORD = ROOT / "third_party/capability-intake/jvogan-motif/primer-thermodynamics-domain.v1.json"
LOCK = ROOT / "third_party/upstream-lock.v2.json"
SPEC = importlib.util.spec_from_file_location("capability_intake", SCRIPT)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError("cannot load capability intake verifier")
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


def fails(record: dict[str, object], lock: dict[str, object]) -> bool:
    try:
        MODULE.validate(record, lock)
    except ValueError:
        return True
    return False


def main() -> int:
    record = json.loads(RECORD.read_text(encoding="utf-8"))
    biomni = json.loads(BIOMNI_RECORD.read_text(encoding="utf-8"))
    aipoch = json.loads(AIPOCH_RECORD.read_text(encoding="utf-8"))
    aipoch_quarantine = json.loads(AIPOCH_QUARANTINE_RECORD.read_text(encoding="utf-8"))
    primer = json.loads(PRIMER_RECORD.read_text(encoding="utf-8"))
    lock = json.loads(LOCK.read_text(encoding="utf-8"))
    wrong_hash = copy.deepcopy(record)
    wrong_hash["source_files"][0]["sha256"] = "0" * 64
    bad_authority = copy.deepcopy(record)
    bad_authority["implementation"]["authority"] = "upstream runtime"
    inflated = copy.deepcopy(primer)
    inflated["evidence"]["intake_level"] = "E4"
    fake_quarantine_proof = copy.deepcopy(aipoch_quarantine)
    fake_quarantine_proof["evidence"]["built_binary_proof"]["source_commit"] = "0" * 40
    results = [
        ("checked-in Motif record is exact-source E4 with a local built-binary proof", not fails(record, lock)),
        ("checked-in Biomni UniProt record is exact-source E4 with a local built-binary proof", not fails(biomni, lock)),
        ("checked-in AIPOCH archive preview is exact-source E2 with no execution authority", not fails(aipoch, lock)),
        ("checked-in AIPOCH quarantine import is exact-source E4 with a local built-binary proof", not fails(aipoch_quarantine, lock)),
        ("checked-in Motif primer helper is exact-source E2 with no execution authority", not fails(primer, lock)),
        ("tampered upstream source hash fails", fails(wrong_hash, lock)),
        ("second execution authority fails", fails(bad_authority, lock)),
        ("unproven E4 self-claim fails", fails(inflated, lock)),
        ("E4 proof for a non-ancestor source revision fails", fails(fake_quarantine_proof, lock)),
    ]
    for name, passed in results:
        print(f"  {'ok' if passed else 'FAIL':<4}  {name}")
    print(f"\n{'OK' if all(passed for _, passed in results) else 'FAIL'}: {sum(passed for _, passed in results)}/{len(results)} passed")
    return 0 if all(passed for _, passed in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
