#!/usr/bin/env python3
"""Keep the runnable Motif seqbench provenance aligned with the intake lock.

The v2 intake ledger is discovery evidence, while seqbench is an already
actor-gated product path.  This test only proves their source identities agree;
it deliberately does not convert any other intake component into a product.
"""

from __future__ import annotations

import json
import re
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
LOCK = ROOT / "third_party/upstream-lock.v2.json"
VENDOR = ROOT / "third_party/motif/VENDOR_MANIFEST.json"
PROVENANCE = ROOT / "third_party/provenance/motif-876a-seqbench.md"
SEQBENCH = ROOT / "agent/crates/codegen/xai-grok-science/src/seqbench.rs"
ALGORITHMS = {
    "src/bio/fasta-parser.ts",
    "src/bio/gc-content.ts",
    "src/bio/reverse-complement.ts",
    "src/bio/translate.ts",
    "src/bio/codon-tables.ts",
    "src/bio/orf-detection.ts",
    "src/bio/restriction-sites.ts",
    "src/bio/restriction-digest.ts",
}


def rust_constant(source: str, name: str) -> str:
    match = re.search(rf'pub const {re.escape(name)}: &str = "([^"]+)";', source)
    if match is None:
        raise ValueError(f"seqbench is missing {name}")
    return match.group(1)


def main() -> int:
    lock = json.loads(LOCK.read_text(encoding="utf-8"))
    motif = next(source for source in lock["sources"] if source["id"] == "jvogan-motif")
    vendor = json.loads(VENDOR.read_text(encoding="utf-8"))
    seqbench = SEQBENCH.read_text(encoding="utf-8")
    provenance = PROVENANCE.read_text(encoding="utf-8")
    receipt = json.loads((ROOT / motif["components"][0]["evidence"]["record"]).read_text(encoding="utf-8"))
    inventory = json.loads((ROOT / receipt["tree_inventory"]["path"]).read_text(encoding="utf-8"))
    tree_paths = {entry["path"] for entry in inventory["entries"]}
    deterministic = next(component for component in motif["components"] if component["id"] == "deterministic-sequence")
    constants_match = (
        rust_constant(seqbench, "MOTIF_REPOSITORY") == motif["repository"]
        and rust_constant(seqbench, "MOTIF_COMMIT") == motif["exact_commit"]
        and rust_constant(seqbench, "MOTIF_LICENSE") == motif["root_license"]["spdx"]
    )
    vendor_match = vendor["commit"] == motif["exact_commit"] and vendor["upstream"] + ".git" == motif["repository"]
    selected_paths = {match.group(1) for match in re.finditer(r'"(src/bio/[^" ]+\.ts)"\.into\(\)', seqbench)}
    results = [
        ("seqbench constants agree with the exact v2 Motif receipt", constants_match),
        ("vendor manifest agrees with the exact v2 Motif receipt", vendor_match),
        (
            "all eight adapted algorithm sources exist in the quarantined exact tree inventory",
            ALGORITHMS.issubset(tree_paths),
        ),
        (
            "seqbench declares exactly the eight provenance algorithm sources",
            selected_paths == ALGORITHMS,
        ),
        (
            "the selected Motif tree component is rights-verified adapt with no external execution authority",
            deterministic["path"] == "src/bio/**"
            and deterministic["disposition"] == "adapt"
            and deterministic["rights_status"] == "verified"
            and deterministic["execution_authority"] == "none",
        ),
        (
            "the human provenance record names every exact adapted source and its source-tree digest",
            all(path in provenance and next(entry["sha256"] for entry in inventory["entries"] if entry["path"] == path) in provenance for path in ALGORITHMS),
        ),
    ]
    for name, passed in results:
        print(f"  {'ok' if passed else 'FAIL':<4}  {name}")
    print(f"\n{'OK' if all(passed for _, passed in results) else 'FAIL'}: {sum(passed for _, passed in results)}/{len(results)} passed")
    return 0 if all(passed for _, passed in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
