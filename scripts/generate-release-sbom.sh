#!/usr/bin/env bash
# Generate a deterministic, target-filtered SPDX 2.3 SBOM for one Lumen binary.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ASSET_NAME="${1:?usage: generate-release-sbom.sh ASSET TARGET OUTPUT TAG}"
TARGET="${2:?usage: generate-release-sbom.sh ASSET TARGET OUTPUT TAG}"
OUTPUT="${3:?usage: generate-release-sbom.sh ASSET TARGET OUTPUT TAG}"
TAG="${4:?usage: generate-release-sbom.sh ASSET TARGET OUTPUT TAG}"

case "$ASSET_NAME:$TARGET" in
  lumen-macos-arm64:aarch64-apple-darwin|\
  lumen-macos-amd64:x86_64-apple-darwin|\
  lumen-linux-arm64:aarch64-unknown-linux-gnu|\
  lumen-linux-amd64:x86_64-unknown-linux-gnu) ;;
  *) echo "FAIL: unsupported release asset/target tuple: $ASSET_NAME:$TARGET" >&2; exit 1 ;;
esac

BINARY="$(dirname "$OUTPUT")/$ASSET_NAME"
[[ -f "$BINARY" ]] || { echo "FAIL: release binary missing for SBOM: $BINARY" >&2; exit 1; }
COMMIT="$(git -C "$ROOT" rev-parse HEAD)"
VERSION="$(python3 "$ROOT/scripts/release_contract.py" preflight --root "$ROOT" --tag "$TAG" | sed -n 's/.* version=//p')"
SOURCE_EPOCH="${SOURCE_DATE_EPOCH:-$(git -C "$ROOT" show -s --format=%ct "$COMMIT")}"
[[ "$SOURCE_EPOCH" =~ ^[0-9]+$ ]] || { echo "FAIL: SOURCE_DATE_EPOCH must be an integer" >&2; exit 1; }

TMP="$(mktemp -d "${TMPDIR:-/tmp}/lumen-release-sbom-XXXXXX")"
trap 'rm -rf "$TMP"' EXIT
META="$TMP/cargo-metadata.json"

# No --no-deps fallback: release SBOMs require the resolved, target-filtered
# dependency graph rooted at xai-grok-pager-bin.
(cd "$ROOT/agent" && cargo metadata --locked --format-version 1 \
  --filter-platform "$TARGET" \
  --features xai-grok-pager-bin/release-dist >"$META")

mkdir -p "$(dirname "$OUTPUT")"
python3 - "$META" "$OUTPUT" "$BINARY" "$ASSET_NAME" "$TARGET" "$VERSION" "$TAG" "$COMMIT" "$SOURCE_EPOCH" <<'PY'
import hashlib
import json
import re
import sys
from collections import deque
from datetime import datetime, timezone
from pathlib import Path
from urllib.parse import quote

metadata_path, output, binary_path, asset, target, version, tag, commit, source_epoch = sys.argv[1:]
metadata = json.loads(Path(metadata_path).read_text(encoding="utf-8"))
binary = Path(binary_path)
binary_bytes = binary.read_bytes()
binary_sha1 = hashlib.sha1(binary_bytes).hexdigest()
binary_sha256 = hashlib.sha256(binary_bytes).hexdigest()
created = datetime.fromtimestamp(int(source_epoch), timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")

packages_by_id = {package["id"]: package for package in metadata.get("packages", [])}
resolve = metadata.get("resolve")
if not isinstance(resolve, dict) or not isinstance(resolve.get("nodes"), list):
    raise SystemExit("FAIL: cargo metadata did not return a resolved dependency graph")
nodes_by_id = {node["id"]: node for node in resolve["nodes"]}
root_candidates = [
    package for package in packages_by_id.values()
    if package.get("name") == "xai-grok-pager-bin"
]
if len(root_candidates) != 1:
    raise SystemExit(f"FAIL: expected one xai-grok-pager-bin package, got {len(root_candidates)}")
root_id = root_candidates[0]["id"]

reachable = set()
queue = deque([root_id])
def is_release_dependency(dependency):
    kinds = dependency.get("dep_kinds", [])
    return not kinds or any(kind.get("kind") != "dev" for kind in kinds)

while queue:
    package_id = queue.popleft()
    if package_id in reachable:
        continue
    if package_id not in packages_by_id or package_id not in nodes_by_id:
        raise SystemExit(f"FAIL: incomplete cargo resolve node: {package_id}")
    reachable.add(package_id)
    for dependency in nodes_by_id[package_id].get("deps", []):
        if not is_release_dependency(dependency):
            continue
        dependency_id = dependency.get("pkg")
        if dependency_id and dependency_id not in reachable:
            queue.append(dependency_id)

def spdx_id(package_id: str) -> str:
    return "SPDXRef-Cargo-" + hashlib.sha256(package_id.encode()).hexdigest()[:24]

def normalize_license(value):
    if not isinstance(value, str) or not value.strip():
        return "NOASSERTION"
    value = value.strip()
    # Cargo metadata historically contains slash-separated alternatives. Only
    # rewrite a sequence of SPDX-like identifiers; ambiguous values fail closed.
    if "/" in value:
        parts = [part.strip() for part in value.split("/")]
        if all(re.fullmatch(r"[A-Za-z0-9.+-]+", part) for part in parts):
            value = " OR ".join(parts)
        else:
            return "NOASSERTION"
    return value

def download_location(package):
    source = package.get("source") or ""
    if source.startswith("registry+"):
        return f"https://crates.io/api/v1/crates/{quote(package['name'], safe='')}/{quote(package['version'], safe='')}/download"
    return "NOASSERTION"

def component(package_id):
    package = packages_by_id[package_id]
    license_expression = normalize_license(package.get("license"))
    return {
        "SPDXID": spdx_id(package_id),
        "name": package["name"],
        "versionInfo": package["version"],
        "downloadLocation": download_location(package),
        "filesAnalyzed": False,
        "licenseConcluded": license_expression,
        "licenseDeclared": license_expression,
        "copyrightText": "NOASSERTION",
        "supplier": "NOASSERTION",
        "externalRefs": [{
            "referenceCategory": "PACKAGE-MANAGER",
            "referenceType": "purl",
            "referenceLocator": f"pkg:cargo/{quote(package['name'], safe='')}@{quote(package['version'], safe='')}",
        }],
    }

asset_package_id = "SPDXRef-Package-lumen-release-asset"
source_package_id = "SPDXRef-Package-lumen-source"
binary_file_id = "SPDXRef-File-lumen-release-binary"
packages = [{
    "SPDXID": asset_package_id,
    "name": asset,
    "versionInfo": version,
    "filesAnalyzed": True,
    "downloadLocation": "NOASSERTION",
    "licenseConcluded": "Apache-2.0",
    "licenseDeclared": "Apache-2.0",
    "copyrightText": "See NOTICE and LEGAL.md",
    "supplier": "Organization: Lumen authors",
}, {
    "SPDXID": source_package_id,
    "name": "lumen-source",
    "versionInfo": commit,
    "downloadLocation": "NOASSERTION",
    "filesAnalyzed": False,
    "licenseConcluded": "Apache-2.0",
    "licenseDeclared": "Apache-2.0",
    "copyrightText": "See NOTICE and LEGAL.md",
    "supplier": "Organization: Lumen authors",
}] + [component(package_id) for package_id in sorted(reachable)]

relationships = [{
    "spdxElementId": "SPDXRef-DOCUMENT",
    "relationshipType": "DESCRIBES",
    "relatedSpdxElement": asset_package_id,
}, {
    "spdxElementId": asset_package_id,
    "relationshipType": "CONTAINS",
    "relatedSpdxElement": binary_file_id,
}, {
    "spdxElementId": asset_package_id,
    "relationshipType": "GENERATED_FROM",
    "relatedSpdxElement": source_package_id,
}, {
    "spdxElementId": asset_package_id,
    "relationshipType": "DEPENDS_ON",
    "relatedSpdxElement": spdx_id(root_id),
}]
for package_id in sorted(reachable):
    for dependency in nodes_by_id[package_id].get("deps", []):
        if not is_release_dependency(dependency):
            continue
        dependency_id = dependency.get("pkg")
        if dependency_id in reachable:
            relationships.append({
                "spdxElementId": spdx_id(package_id),
                "relationshipType": "DEPENDS_ON",
                "relatedSpdxElement": spdx_id(dependency_id),
            })

document = {
    "spdxVersion": "SPDX-2.3",
    "dataLicense": "CC0-1.0",
    "SPDXID": "SPDXRef-DOCUMENT",
    "name": f"{asset}-{version}",
    "documentNamespace": f"https://lumen.local/spdx/{tag}/{target}",
    "creationInfo": {
        "created": created,
        "creators": [
            "Tool: scripts/generate-release-sbom.sh",
            "Organization: Lumen",
        ],
        "licenseListVersion": "3.21",
    },
    "packages": packages,
    "files": [{
        "SPDXID": binary_file_id,
        "fileName": f"./{asset}",
        "checksums": [
            {"algorithm": "SHA1", "checksumValue": binary_sha1},
            {"algorithm": "SHA256", "checksumValue": binary_sha256},
        ],
        "licenseConcluded": "NOASSERTION",
        "copyrightText": "NOASSERTION",
    }],
    "relationships": relationships,
    "annotations": [{
        "annotationType": "OTHER",
        "annotator": "Tool: scripts/generate-release-sbom.sh",
        "annotationDate": created,
        "comment": json.dumps({
            "asset": asset,
            "target": target,
            "version": version,
            "tag": tag,
            "commit": commit,
            "binary_sha1": binary_sha1,
            "binary_sha256": binary_sha256,
            "binary_size": len(binary_bytes),
            "identity": "lumen-release-v1",
            "cargo_root": "xai-grok-pager-bin",
            "cargo_reachable_packages": len(reachable),
        }, sort_keys=True),
    }],
}
Path(output).write_text(json.dumps(document, indent=2, sort_keys=True) + "\n", encoding="utf-8")
print(f"OK: wrote target-scoped release SBOM {output} packages={len(reachable)} asset={asset}")
PY
