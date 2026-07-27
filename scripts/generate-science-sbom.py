#!/usr/bin/env python3
"""Deterministic SPDX 2.3 SBOM for one Lumen Science (Go) release binary.

Why a separate generator
------------------------
`scripts/generate-release-sbom.sh` produces SBOMs for the Lumen Core assets by
reading `cargo metadata`. The science assets are Go binaries, so that script
cannot describe them — it is hard-coded to four Rust asset/target tuples and
exits 1 for anything else. Extending it would mean bolting a second toolchain
onto a script whose whole shape is cargo's.

This reads the module list that the Go linker EMBEDDED IN THE BINARY
(`go version -m`), which is the strongest available source: it reports what was
actually linked into the artifact being shipped, not what a lockfile said
should be linked at some other time on some other machine.

Determinism: no wall clock. Timestamps come from SOURCE_DATE_EPOCH (the source
commit), so rebuilding the same commit yields a byte-identical SBOM — which is
what makes an SBOM comparable across independent rebuilds.

Usage:
    SOURCE_DATE_EPOCH=<epoch> generate-science-sbom.py <binary> <output.spdx.json> <tag>

Stdlib only.
"""

from __future__ import annotations

import hashlib
import json
import os
import re
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

SPDX_NOASSERTION = "NOASSERTION"


def die(msg: str) -> None:
    print(f"FAIL: {msg}", file=sys.stderr)
    raise SystemExit(1)


def spdx_id(raw: str) -> str:
    """SPDX identifiers allow only letters, digits, '.' and '-'."""
    return "SPDXRef-" + re.sub(r"[^A-Za-z0-9.\-]", "-", raw)


def parse_go_buildinfo(binary: Path) -> dict:
    """Read the module graph the linker embedded in the binary."""
    proc = subprocess.run(
        ["go", "version", "-m", str(binary)], capture_output=True, text=True, check=False
    )
    if proc.returncode != 0:
        die(f"`go version -m` failed on {binary}: {proc.stderr.strip()}")

    info: dict = {"main": None, "deps": [], "build": {}, "goVersion": None}
    for line in proc.stdout.splitlines():
        if not line.startswith("\t"):
            # e.g. "path/to/bin: go1.23.4"
            m = re.search(r":\s*(go\S+)", line)
            if m:
                info["goVersion"] = m.group(1)
            continue
        fields = line.strip().split("\t")
        kind = fields[0]
        if kind == "mod" and len(fields) >= 3:
            info["main"] = {"path": fields[1], "version": fields[2]}
        elif kind == "dep" and len(fields) >= 3:
            entry = {"path": fields[1], "version": fields[2]}
            # A 4th field is the module's own h1: checksum.
            if len(fields) >= 4 and fields[3].startswith("h1:"):
                entry["h1"] = fields[3]
            info["deps"].append(entry)
        elif kind == "=>" and len(fields) >= 3 and info["deps"]:
            # A replace directive: the LAST dep is what actually shipped.
            info["deps"][-1] = {"path": fields[1], "version": fields[2], "replaced": True}
        elif kind == "build" and len(fields) >= 2:
            kv = fields[1].split("=", 1)
            if len(kv) == 2:
                info["build"][kv[0]] = kv[1]
    return info


def purl(path: str, version: str) -> str:
    ver = version.lstrip("v")
    return f"pkg:golang/{path}@{ver}"


def main() -> int:
    if len(sys.argv) != 4:
        die("usage: generate-science-sbom.py <binary> <output.spdx.json> <tag>")
    binary, output, tag = Path(sys.argv[1]), Path(sys.argv[2]), sys.argv[3]

    if not binary.is_file():
        die(f"binary not found: {binary}")

    epoch_raw = os.environ.get("SOURCE_DATE_EPOCH")
    if not epoch_raw or not epoch_raw.isdigit():
        die("SOURCE_DATE_EPOCH must be set to an integer (the source commit timestamp)")
    created = datetime.fromtimestamp(int(epoch_raw), tz=timezone.utc).strftime(
        "%Y-%m-%dT%H:%M:%SZ"
    )

    info = parse_go_buildinfo(binary)
    if info["main"] is None:
        die(f"{binary} has no embedded Go module info — is it a Go binary built with modules?")

    digest = hashlib.sha256(binary.read_bytes()).hexdigest()
    asset = binary.name
    commit = info["build"].get("vcs.revision", SPDX_NOASSERTION)

    root_id = spdx_id(f"Package-{asset}")
    packages = [
        {
            "SPDXID": root_id,
            "name": asset,
            "versionInfo": tag.lstrip("v"),
            "downloadLocation": f"https://github.com/exergyleizhou-ux/lumen-science/releases/tag/{tag}",
            "filesAnalyzed": False,
            "licenseConcluded": "Apache-2.0",
            "licenseDeclared": "Apache-2.0",
            "copyrightText": SPDX_NOASSERTION,
            "supplier": "Organization: Lumen Science",
            "checksums": [{"algorithm": "SHA256", "checksumValue": digest}],
            "comment": (
                f"Built from {info['main']['path']} at commit {commit} with "
                f"{info['goVersion']}; GOOS={info['build'].get('GOOS', '?')} "
                f"GOARCH={info['build'].get('GOARCH', '?')} "
                f"trimpath={info['build'].get('-trimpath', 'false')}"
            ),
        }
    ]
    relationships = [
        {"spdxElementId": "SPDXRef-DOCUMENT", "relatedSpdxElement": root_id,
         "relationshipType": "DESCRIBES"}
    ]

    # Sorted so the document is stable regardless of link order.
    for dep in sorted(info["deps"], key=lambda d: (d["path"], d["version"])):
        dep_id = spdx_id(f"Package-{dep['path']}-{dep['version']}")
        entry = {
            "SPDXID": dep_id,
            "name": dep["path"],
            "versionInfo": dep["version"],
            "downloadLocation": f"https://proxy.golang.org/{dep['path']}/@v/{dep['version']}.zip",
            "filesAnalyzed": False,
            # The Go module proxy does not publish a normalised licence field,
            # and guessing one would be worse than declaring it unknown.
            "licenseConcluded": SPDX_NOASSERTION,
            "licenseDeclared": SPDX_NOASSERTION,
            "copyrightText": SPDX_NOASSERTION,
            "externalRefs": [
                {
                    "referenceCategory": "PACKAGE-MANAGER",
                    "referenceType": "purl",
                    "referenceLocator": purl(dep["path"], dep["version"]),
                }
            ],
        }
        if dep.get("h1"):
            # Go's own module checksum, as recorded in go.sum.
            entry["comment"] = f"go module checksum {dep['h1']}"
        if dep.get("replaced"):
            entry["comment"] = (entry.get("comment", "") + " (replace directive applied)").strip()
        packages.append(entry)
        relationships.append(
            {"spdxElementId": root_id, "relatedSpdxElement": dep_id,
             "relationshipType": "DEPENDS_ON"}
        )

    document = {
        "spdxVersion": "SPDX-2.3",
        "dataLicense": "CC0-1.0",
        "SPDXID": "SPDXRef-DOCUMENT",
        "name": f"lumen-science-{tag}-{asset}",
        "documentNamespace": (
            f"https://github.com/exergyleizhou-ux/lumen-science/spdx/{tag}/{asset}/{digest}"
        ),
        "creationInfo": {
            "created": created,
            "creators": ["Tool: scripts/generate-science-sbom.py", "Organization: Lumen Science"],
            "comment": (
                "Dependency list read from the module graph embedded in the binary by the Go "
                "linker (`go version -m`), so it describes what was actually linked into this "
                "artifact rather than what a lockfile predicted. Deterministic: timestamps come "
                "from SOURCE_DATE_EPOCH, so the same commit yields a byte-identical document."
            ),
        },
        "packages": packages,
        "relationships": relationships,
    }

    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(document, indent=2, sort_keys=False) + "\n", encoding="utf-8")

    dep_count = len(info["deps"])
    note = " (pure standard library)" if dep_count == 0 else ""
    print(f"OK: {output.name} — {dep_count} external module(s){note}, sha256 {digest[:16]}…")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
