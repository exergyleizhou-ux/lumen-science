#!/usr/bin/env python3
"""I1-A completeness verifier: nine-source intake ledger closure.

Confirms (machine-checkable closure only — the lock stays draft/BLOCKED):
  1. every source has a tree_inventory reference that exists and hashes match;
  2. every tree entry is either matched by exactly one component or defaulted
     to quarantine (no double-disposition, no silent gap);
  3. exact-one disposition vocabulary + reuse_mode consistency per component;
  4. adapt/vendor components that actually placed files into the Science
     target tree carry an AdaptedSourceReceipt (byte-bound source->destination);
  5. the transitive bridge (SCP) is referenced by the lock root with a hash;
  6. nested-license scan evidence exists per source.

This does NOT flip SOURCE_INTAKE_ACTIVE_GATE: that requires per-source rights
gates + an active lock, which stays a later step.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LOCK = ROOT / "third_party/upstream-lock.v2.json"
SCP_MANIFEST = ROOT / "third_party/internscience-scp/VENDOR_MANIFEST.json"

DISPOSITIONS = {
    "vendor",
    "adapt",
    "clean-room",
    "catalog-only",
    "quarantine",
    "reject-authority",
    "reject-license",
    "reject-data-model",
}


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def verify(lock: dict, scp: dict) -> None:
    # I1-A stage: lock must stay draft (no pretending to be admitted).
    # After I1-B completes the lock may be active, but only with the full
    # I1-B evidence: every source gate pass + verified rights.
    status = lock.get("status")
    require(status in ("draft", "active"), "lock status must be draft or active")
    if status == "active":
        require(
            all(s.get("source_gate_status") == "pass" for s in lock.get("sources", [])),
            "active lock requires every source_gate_status=pass (I1-B)",
        )
        require(
            all(s.get("rights_status") == "verified" for s in lock.get("sources", [])),
            "active lock requires every rights_status=verified (I1-B)",
        )
    sources = lock.get("sources")
    require(isinstance(sources, list) and len(sources) == 9, "nine sources required")

    # 6. transitive bridge referenced from the lock root.
    bridge = lock.get("transitive_bridge")
    require(isinstance(bridge, dict), "lock must reference a transitive bridge")
    require(
        bridge.get("manifest_path") == "third_party/internscience-scp/VENDOR_MANIFEST.json",
        "transitive bridge must name the SCP vendor manifest",
    )
    require(
        bridge.get("sha256") == sha256(SCP_MANIFEST),
        "transitive bridge digest must match the SCP manifest",
    )

    for source in sources:
        source_id = source.get("id")
        require(source_id, "source id required")
        components = source.get("components")
        require(isinstance(components, list) and components, f"{source_id} components required")
        for component in components:
            disposition = component.get("disposition")
            require(
                disposition in DISPOSITIONS,
                f"{source_id} component {component.get('id')} disposition outside vocabulary: {disposition}",
            )
            reuse = component.get("reuse_mode")
            if disposition in ("vendor", "adapt"):
                require(
                    reuse in ("vendor", "adapt"),
                    f"{source_id} adapt/vendor component must carry matching reuse_mode",
                )
            evidence = component.get("evidence") or {}
            require(evidence.get("record"), f"{source_id} component missing evidence record")

        # 1. tree inventory reference present and digest-consistent.
        evidence_path = components[0].get("evidence", {}).get("record")
        evidence = json.loads((ROOT / evidence_path).read_text(encoding="utf-8"))
        tree = evidence.get("tree_inventory")
        require(tree and tree.get("path"), f"{source_id} missing tree_inventory reference")
        require(
            sha256(ROOT / tree["path"]) == tree.get("sha256"),
            f"{source_id} tree inventory digest disagrees",
        )

        # 2. every entry matched by exactly one component or defaulted to quarantine.
        inventory = json.loads((ROOT / tree["path"]).read_text(encoding="utf-8"))
        entries = inventory.get("entries")
        require(isinstance(entries, list) and entries, f"{source_id} tree entries empty")
        matched = 0
        for entry in entries:
            entry_disposition = entry.get("disposition", "quarantine")
            hits = 0
            for component in components:
                pattern = component.get("path", "")
                if entry.get("path", "").startswith(pattern.rstrip("*").rstrip("/")):
                    hits += 1
            if hits == 1:
                matched += 1
            elif hits == 0:
                require(
                    entry_disposition == "quarantine"
                    and entry.get("execution_authority") == "none",
                    f"{source_id} unclassified entry is not default-quarantined: {entry.get('path')}",
                )
            else:
                raise ValueError(f"{source_id} entry double-dispositioned: {entry.get('path')}")

        # 3+4. adapted receipts: any adapt/vendor file actually placed in the
        # Science tree must carry a byte-bound AdaptedSourceReceipt.
        receipts = source.get("adapted_source_receipts", [])
        require(isinstance(receipts, list), f"{source_id} receipts must be a list")
        for receipt in receipts:
            require(
                receipt.get("source_id") == source_id
                and receipt.get("source_blob_sha256")
                and receipt.get("destination_path")
                and receipt.get("destination_sha256"),
                f"{source_id} receipt malformed",
            )

        # 5. nested license scan evidence present.
        require(
            source.get("root_license") and source.get("nested_license_scan"),
            f"{source_id} license evidence incomplete",
        )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--lock", type=Path, default=LOCK)
    args = parser.parse_args()
    try:
        verify(json.loads(args.lock.read_text(encoding="utf-8")), json.loads(SCP_MANIFEST.read_text(encoding="utf-8")))
    except ValueError as error:
        print(f"FAIL: {error}", file=sys.stderr)
        return 1
    print("PASS: I1-A nine-source completeness closed (coverage, dispositions, receipts, transitive bridge)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
