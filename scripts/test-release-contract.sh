#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP="$(mktemp -d "${TMPDIR:-/tmp}/lumen-release-contract-XXXXXX")"
trap 'rm -rf "$TMP"' EXIT
FIXTURE="$TMP/repo"
DIST="$FIXTURE/dist"
HOME="$TMP/home"
export HOME
mkdir -p "$FIXTURE/agent/crates/codegen/xai-grok-pager-bin" "$DIST" "$HOME"

cp "$ROOT/scripts/release_contract.py" "$FIXTURE/release_contract.py"
printf '1.2.3-alpha.4\n' >"$FIXTURE/VERSION"
cat >"$FIXTURE/agent/crates/codegen/xai-grok-pager-bin/Cargo.toml" <<'EOF'
[package]
name = "fixture"
version = "1.2.3-alpha.4"
EOF

git -C "$FIXTURE" init -q
git -C "$FIXTURE" config user.email fixture@example.invalid
git -C "$FIXTURE" config user.name Fixture
git -C "$FIXTURE" add .
git -C "$FIXTURE" commit -qm fixture
git -C "$FIXTURE" tag v1.2.3-alpha.4
FIXTURE_COMMIT="$(git -C "$FIXTURE" rev-parse HEAD)"
git -C "$FIXTURE" branch authorized

expect_fail() {
  local label="$1"
  local expected_stderr="$2"
  shift 2
  if "$@" >"$TMP/$label.out" 2>&1; then
    echo "FAIL: expected failure: $label" >&2
    exit 1
  fi
  if ! grep -Fq -- "$expected_stderr" "$TMP/$label.out"; then
    echo "FAIL: $label failed for the wrong reason; expected output containing: $expected_stderr" >&2
    sed -n '1,80p' "$TMP/$label.out" >&2
    exit 1
  fi
  echo "OK: rejected $label"
}

python3 "$FIXTURE/release_contract.py" preflight \
  --root "$FIXTURE" --tag v1.2.3-alpha.4 --require-clean --require-head-tag --allowed-ref authorized
cp "$FIXTURE/VERSION" "$TMP/VERSION.valid"
printf '1.2.4\n' >"$FIXTURE/VERSION"
expect_fail version-cargo-mismatch "VERSION/Cargo mismatch" python3 "$FIXTURE/release_contract.py" preflight \
  --root "$FIXTURE" --tag v1.2.4
cp "$TMP/VERSION.valid" "$FIXTURE/VERSION"
expect_fail tag-version-mismatch "tag/version mismatch" python3 "$FIXTURE/release_contract.py" preflight \
  --root "$FIXTURE" --tag v1.2.4
echo dirty >"$FIXTURE/dirty"
expect_fail dirty-tree "formal release requires a clean tree" python3 "$FIXTURE/release_contract.py" preflight \
  --root "$FIXTURE" --tag v1.2.3-alpha.4 --require-clean
rm "$FIXTURE/dirty"

EMPTY_TREE="$(git -C "$FIXTURE" mktree </dev/null)"
UNRELATED_COMMIT="$(printf 'unrelated\n' | git -C "$FIXTURE" commit-tree "$EMPTY_TREE")"
git -C "$FIXTURE" update-ref refs/heads/not-containing "$UNRELATED_COMMIT"
expect_fail unauthorized-tag-commit "is not an ancestor of authorized ref" python3 "$FIXTURE/release_contract.py" preflight \
  --root "$FIXTURE" --tag v1.2.3-alpha.4 --allowed-ref not-containing --require-head-tag

python3 - "$DIST" "$FIXTURE_COMMIT" <<'PY'
import hashlib
import json
import sys
from pathlib import Path

dist = Path(sys.argv[1])
commit = sys.argv[2]
targets = {
    "lumen-macos-arm64": "aarch64-apple-darwin",
    "lumen-macos-amd64": "x86_64-apple-darwin",
    "lumen-linux-arm64": "aarch64-unknown-linux-gnu",
    "lumen-linux-amd64": "x86_64-unknown-linux-gnu",
}
for asset, target in targets.items():
    binary = dist / asset
    binary.write_bytes(f"fixture binary for {target}\n".encode())
    digest_sha1 = hashlib.sha1(binary.read_bytes()).hexdigest()
    digest = hashlib.sha256(binary.read_bytes()).hexdigest()
    sbom = {
        "spdxVersion": "SPDX-2.3",
        "dataLicense": "CC0-1.0",
        "SPDXID": "SPDXRef-DOCUMENT",
        "name": f"{asset}-1.2.3-alpha.4",
        "documentNamespace": f"https://lumen.local/spdx/v1.2.3-alpha.4/{target}",
        "creationInfo": {
            "created": "2026-07-18T00:00:00Z",
            "creators": ["Tool: scripts/generate-release-sbom.sh", "Organization: Lumen"],
        },
        "packages": [
            {
                "SPDXID": "SPDXRef-Package-lumen-release-asset",
                "name": asset,
                "versionInfo": "1.2.3-alpha.4",
                "supplier": "Organization: Lumen authors",
            },
            {
                "SPDXID": "SPDXRef-Package-lumen-source",
                "name": "lumen-source",
                "versionInfo": commit,
                "supplier": "Organization: Lumen authors",
            },
            # Dependency names may retain upstream crate identity.
            {
                "SPDXID": "SPDXRef-Cargo-root",
                "name": "xai-grok-pager-bin",
                "versionInfo": "1.2.3-alpha.4",
                "downloadLocation": "NOASSERTION",
                "licenseDeclared": "Apache-2.0",
                "licenseConcluded": "Apache-2.0",
                "externalRefs": [{"referenceCategory": "PACKAGE-MANAGER", "referenceType": "purl", "referenceLocator": "pkg:cargo/xai-grok-pager-bin@1.2.3-alpha.4"}],
            },
            {
                "SPDXID": "SPDXRef-Cargo-dependency",
                "name": "xai-grok-config",
                "versionInfo": "1",
                "downloadLocation": "NOASSERTION",
                "licenseDeclared": "MIT OR Apache-2.0",
                "licenseConcluded": "MIT OR Apache-2.0",
                "externalRefs": [{"referenceCategory": "PACKAGE-MANAGER", "referenceType": "purl", "referenceLocator": "pkg:cargo/xai-grok-config@1"}],
            },
        ],
        "files": [{
            "SPDXID": "SPDXRef-File-lumen-release-binary",
            "fileName": f"./{asset}",
            "checksums": [
                {"algorithm": "SHA1", "checksumValue": digest_sha1},
                {"algorithm": "SHA256", "checksumValue": digest},
            ],
        }],
        "relationships": [
            {"spdxElementId": "SPDXRef-DOCUMENT", "relationshipType": "DESCRIBES", "relatedSpdxElement": "SPDXRef-Package-lumen-release-asset"},
            {"spdxElementId": "SPDXRef-Package-lumen-release-asset", "relationshipType": "CONTAINS", "relatedSpdxElement": "SPDXRef-File-lumen-release-binary"},
            {"spdxElementId": "SPDXRef-Package-lumen-release-asset", "relationshipType": "GENERATED_FROM", "relatedSpdxElement": "SPDXRef-Package-lumen-source"},
        ],
        "annotations": [{
            "comment": json.dumps({
                "asset": asset,
                "target": target,
                "version": "1.2.3-alpha.4",
                "tag": "v1.2.3-alpha.4",
                "commit": commit,
                "binary_sha1": digest_sha1,
                "binary_sha256": digest,
                "binary_size": binary.stat().st_size,
                "identity": "lumen-release-v1",
                "cargo_root": "xai-grok-pager-bin",
            })
        }],
    }
    (dist / f"{asset}.spdx.json").write_text(json.dumps(sbom) + "\n")

# Valid-format synthetic minisign public key. Cryptography is mocked below.
import base64
raw = b"Ed" + bytes(range(40))
(dist / "lumen-release.pub").write_text(
    "untrusted comment: lumen release public key\n" + base64.b64encode(raw).decode() + "\n"
)
PY

python3 "$FIXTURE/release_contract.py" assemble --root "$FIXTURE" --dist "$DIST" \
  --tag v1.2.3-alpha.4 --commit "$FIXTURE_COMMIT" \
  --public-key "$DIST/lumen-release.pub"
python3 "$FIXTURE/release_contract.py" validate-unsigned --root "$FIXTURE" --dist "$DIST" \
  --tag v1.2.3-alpha.4 --expected-commit "$FIXTURE_COMMIT" \
  --public-key "$DIST/lumen-release.pub"
cp -R "$DIST" "$TMP/unsigned-template"

cat >"$TMP/minisign-mock" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
while (($#)); do
  case "$1" in
    -m) message="$2"; shift 2 ;;
    -x) signature="$2"; shift 2 ;;
    *) shift ;;
  esac
done
expected="$(shasum -a 256 "$message" | awk '{print $1}')"
IFS='|' read -r actual identity tag filename <"$signature"
if [[ "$actual" != "$expected" ]]; then
  echo "signature digest mismatch" >&2
  exit 1
fi
echo "Trusted comment: $identity $tag $filename"
EOF
chmod +x "$TMP/minisign-mock"

sign_fixtures() {
  for payload in "$DIST"/lumen-macos-* "$DIST"/lumen-linux-* "$DIST/lumen-release-manifest.json"; do
    [[ "$payload" == *.minisig ]] && continue
    [[ "$payload" == "$DIST/lumen-release.pub" ]] && continue
    printf '%s|lumen-release-v1|v1.2.3-alpha.4|%s\n' \
      "$(shasum -a 256 "$payload" | awk '{print $1}')" "$(basename "$payload")" >"$payload.minisig"
  done
}
refresh_manifest_sbom_hash() {
  python3 - "$DIST/lumen-release-manifest.json" "$DIST/lumen-linux-amd64.spdx.json" <<'PY'
import hashlib
import json
import sys
from pathlib import Path

manifest_path, sbom_path = map(Path, sys.argv[1:])
manifest = json.loads(manifest_path.read_text())
digest = hashlib.sha256(sbom_path.read_bytes()).hexdigest()
metadata = next(asset for asset in manifest["assets"] if asset["name"] == "lumen-linux-amd64")["sbom"]
metadata["sha256"] = digest
metadata["size"] = sbom_path.stat().st_size
manifest_path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
PY
  sign_fixtures
}
sign_fixtures

verify=(python3 "$FIXTURE/release_contract.py" verify --root "$FIXTURE" --dist "$DIST"
  --tag v1.2.3-alpha.4 --expected-commit "$FIXTURE_COMMIT"
  --public-key "$DIST/lumen-release.pub" --minisign "$TMP/minisign-mock")
"${verify[@]}"

cp "$DIST/lumen-release-manifest.json" "$TMP/manifest.valid"
for field_and_value in \
  'versionSource|Other|manifest version source must be' \
  'signing.publicKeyAsset|renamed-release.pub|manifest public key asset must be' \
  'signing.manifestSignature|renamed-manifest.minisig|manifest signature asset must be'
do
  IFS='|' read -r field value expected_error <<<"$field_and_value"
  cp "$TMP/manifest.valid" "$DIST/lumen-release-manifest.json"
  python3 - "$DIST/lumen-release-manifest.json" "$field" "$value" <<'PY'
import json
import sys
from pathlib import Path

path = Path(sys.argv[1])
document = json.loads(path.read_text())
target = document
parts = sys.argv[2].split(".")
for part in parts[:-1]:
    target = target[part]
target[parts[-1]] = sys.argv[3]
path.write_text(json.dumps(document, indent=2, sort_keys=True) + "\n")
PY
  sign_fixtures
  expect_fail "resigned-${field//./-}-mismatch" "$expected_error" "${verify[@]}"
done

for field_and_value in \
  'schemaVersion|true|unsupported or non-Lumen release manifest' \
  'schemaVersion|1.0|unsupported or non-Lumen release manifest' \
  'assets.0.size|"__FLOAT_CURRENT__"|binary hash/size mismatch' \
  'assets.0.sbom.size|"__FLOAT_CURRENT__"|SBOM target/hash/size/name mismatch'
do
  IFS='|' read -r field value expected_error <<<"$field_and_value"
  cp "$TMP/manifest.valid" "$DIST/lumen-release-manifest.json"
  python3 - "$DIST/lumen-release-manifest.json" "$field" "$value" <<'PY'
import json
import sys
from pathlib import Path

path = Path(sys.argv[1])
document = json.loads(path.read_text())
value = json.loads(sys.argv[3])
target = document
parts = sys.argv[2].split(".")
for part in parts[:-1]:
    target = target[int(part)] if part.isdigit() else target[part]
last = parts[-1]
current = target[int(last)] if last.isdigit() else target[last]
if value == "__FLOAT_CURRENT__":
    value = float(current)
if last.isdigit():
    target[int(last)] = value
else:
    target[last] = value
path.write_text(json.dumps(document, indent=2, sort_keys=True) + "\n")
PY
  sign_fixtures
  expect_fail "resigned-exact-type-${field//./-}" "$expected_error" "${verify[@]}"
done
cp "$TMP/manifest.valid" "$DIST/lumen-release-manifest.json"
sign_fixtures

missing="$DIST/lumen-linux-arm64.minisig"
mv "$missing" "$missing.saved"
expect_fail missing-signature "signed release file set mismatch" "${verify[@]}"
mv "$missing.saved" "$missing"

printf 'wrong\n' >"$DIST/lumen-linux-arm64.minisig"
expect_fail invalid-signature "signature verification failed" "${verify[@]}"
sign_fixtures

signature="$DIST/lumen-linux-arm64.minisig"
sed 's/lumen-release-v1/WRONG-RELEASE-IDENTITY/' "$signature" >"$signature.wrong"
mv "$signature.wrong" "$signature"
expect_fail wrong-release-identity "signature trusted comment mismatch" "${verify[@]}"
sign_fixtures

expect_fail wrong-manifest-commit "manifest commit does not match expected tag commit" python3 "$FIXTURE/release_contract.py" verify \
  --root "$FIXTURE" --dist "$DIST" --tag v1.2.3-alpha.4 \
  --expected-commit 0123456789abcdef0123456789abcdef01234567 \
  --public-key "$DIST/lumen-release.pub" --minisign "$TMP/minisign-mock"

printf 'tampered\n' >>"$DIST/lumen-macos-amd64"
expect_fail checksum-mismatch "binary hash/size mismatch" "${verify[@]}"
sed -i.bak '$d' "$DIST/lumen-macos-amd64" && rm "$DIST/lumen-macos-amd64.bak"

printf 'legacy\n' >"$DIST/grok-linux-amd64"
expect_fail legacy-asset-identity "signed release file set mismatch" "${verify[@]}"
rm "$DIST/grok-linux-amd64"

sbom="$DIST/lumen-linux-amd64.spdx.json"
cp "$sbom" "$sbom.valid"
python3 - "$sbom" <<'PY'
import json, sys
from pathlib import Path
path = Path(sys.argv[1]); doc = json.loads(path.read_text()); doc.pop("dataLicense"); path.write_text(json.dumps(doc))
PY
refresh_manifest_sbom_hash
expect_fail incomplete-spdx-document "dataLicense mismatch" "${verify[@]}"
mv "$sbom.valid" "$sbom"
refresh_manifest_sbom_hash

cp "$sbom" "$sbom.valid"
python3 - "$sbom" <<'PY'
import json, sys
from pathlib import Path
path = Path(sys.argv[1]); doc = json.loads(path.read_text()); doc["files"][0]["checksums"][0]["checksumValue"] = "0" * 64; path.write_text(json.dumps(doc))
PY
refresh_manifest_sbom_hash
expect_fail sbom-binary-hash-mismatch "does not contain final binary checksum SHA1" "${verify[@]}"
mv "$sbom.valid" "$sbom"
refresh_manifest_sbom_hash

cp "$sbom" "$sbom.valid"
python3 - "$sbom" <<'PY'
import json, sys
from pathlib import Path
path = Path(sys.argv[1]); doc = json.loads(path.read_text()); comment = json.loads(doc["annotations"][0]["comment"]); comment["binary_size"] += 1; doc["annotations"][0]["comment"] = json.dumps(comment); path.write_text(json.dumps(doc))
PY
refresh_manifest_sbom_hash
expect_fail sbom-binary-size-mismatch "release annotation does not bind" "${verify[@]}"
mv "$sbom.valid" "$sbom"
refresh_manifest_sbom_hash

cp "$sbom" "$sbom.valid"
python3 - "$sbom" <<'PY'
import json, sys
from pathlib import Path
path = Path(sys.argv[1]); doc = json.loads(path.read_text()); doc["relationships"] = [item for item in doc["relationships"] if item["relationshipType"] != "GENERATED_FROM"]; path.write_text(json.dumps(doc))
PY
refresh_manifest_sbom_hash
expect_fail sbom-source-relationship-missing "missing relationship" "${verify[@]}"
mv "$sbom.valid" "$sbom"
refresh_manifest_sbom_hash

cp "$sbom" "$sbom.valid"
python3 - "$sbom" <<'PY'
import json, sys
from pathlib import Path
path = Path(sys.argv[1]); doc = json.loads(path.read_text()); doc["documentNamespace"] = "https://XAI-ORG-SHARED/GROK-BUILD/releases/1"; path.write_text(json.dumps(doc))
PY
refresh_manifest_sbom_hash
expect_fail legacy-sbom-document-identity "documentNamespace mismatch" "${verify[@]}"
mv "$sbom.valid" "$sbom"
refresh_manifest_sbom_hash

cp "$sbom" "$sbom.valid"
python3 - "$sbom" <<'PY'
import json, sys
from pathlib import Path
path = Path(sys.argv[1]); doc = json.loads(path.read_text()); doc["documentNamespace"] = "https://lumen.local/spdx/wrong/target"; path.write_text(json.dumps(doc))
PY
refresh_manifest_sbom_hash
expect_fail sbom-namespace-mismatch "documentNamespace mismatch" "${verify[@]}"
mv "$sbom.valid" "$sbom"
refresh_manifest_sbom_hash

cp "$sbom" "$sbom.valid"
python3 - "$sbom" <<'PY'
import json, sys
from pathlib import Path
path = Path(sys.argv[1]); doc = json.loads(path.read_text()); comment = json.loads(doc["annotations"][0]["comment"]); comment["tag"] = "v9.9.9"; doc["annotations"][0]["comment"] = json.dumps(comment); path.write_text(json.dumps(doc))
PY
refresh_manifest_sbom_hash
expect_fail sbom-tag-annotation-mismatch "release annotation does not bind" "${verify[@]}"
mv "$sbom.valid" "$sbom"
refresh_manifest_sbom_hash

rm "$DIST/lumen-macos-arm64.spdx.json" "$DIST/lumen-macos-arm64.spdx.json.minisig"
expect_fail asset-sbom-not-one-to-one "SBOM target/hash/size/name mismatch" "${verify[@]}"

# SBOM generator preflights against the live repo VERSION + tag; keep them matched.
LIVE_VERSION="$(tr -d '[:space:]' <"$ROOT/VERSION")"
LIVE_TAG="v${LIVE_VERSION}"
printf 'release sbom fixture binary\n' >"$TMP/lumen-linux-amd64"
SOURCE_DATE_EPOCH=1721260800 \
  "$ROOT/scripts/generate-release-sbom.sh" \
  lumen-linux-amd64 x86_64-unknown-linux-gnu \
  "$TMP/lumen-linux-amd64.spdx.json" "$LIVE_TAG"
python3 - "$TMP/lumen-linux-amd64.spdx.json" "$LIVE_VERSION" "$LIVE_TAG" <<'PY'
import json
import sys
from pathlib import Path

document = json.loads(Path(sys.argv[1]).read_text())
version = sys.argv[2]
tag = sys.argv[3]
assert document["dataLicense"] == "CC0-1.0"
assert document["SPDXID"] == "SPDXRef-DOCUMENT"
assert document["documentNamespace"] == f"https://lumen.local/spdx/{tag}/x86_64-unknown-linux-gnu"
assert document["files"][0]["fileName"] == "./lumen-linux-amd64"
assert document["packages"][0]["versionInfo"] == version
assert {item["algorithm"] for item in document["files"][0]["checksums"]} == {"SHA1", "SHA256"}
components = document["packages"][2:]
assert any(package["name"] == "xai-grok-pager-bin" for package in components)
assert all("/" not in package["licenseDeclared"] for package in components)
assert all(any(ref["referenceLocator"].startswith("pkg:cargo/") for ref in package["externalRefs"]) for package in components)
assert all(package["name"] != "lumen-science" for package in components)
assert all(package["name"] != "serial_test" for package in components)
annotation = json.loads(document["annotations"][0]["comment"])
assert annotation["cargo_root"] == "xai-grok-pager-bin"
assert annotation["cargo_reachable_packages"] == len(components)
print("OK: target-scoped Cargo closure, normalized licenses, purls, and dual binary hashes present")
PY

if [[ -n "${SPDX_TOOLS_PYTHON:-}" ]]; then
  "$SPDX_TOOLS_PYTHON" "$ROOT/scripts/validate-spdx.py" "$TMP/lumen-linux-amd64.spdx.json"
  cp "$TMP/lumen-linux-amd64.spdx.json" "$TMP/invalid-license.spdx.json"
  python3 - "$TMP/invalid-license.spdx.json" <<'PY'
import json, sys
from pathlib import Path
path = Path(sys.argv[1]); document = json.loads(path.read_text()); document["packages"][2]["licenseDeclared"] = "MIT/Apache-2.0"; path.write_text(json.dumps(document))
PY
  expect_fail official-spdx-license-rejection "official SPDX" \
    "$SPDX_TOOLS_PYTHON" "$ROOT/scripts/validate-spdx.py" "$TMP/invalid-license.spdx.json"
fi

grep -q -- '--filter-platform "$TARGET"' "$ROOT/scripts/generate-release-sbom.sh"
grep -q 'xai-grok-pager-bin' "$ROOT/scripts/generate-release-sbom.sh"
grep -q -- '--profile release-dist' "$ROOT/.github/workflows/release.yml"
grep -q 'release-dist/lumen' "$ROOT/.github/workflows/release.yml"
grep -q 'environment: lumen-release' "$ROOT/.github/workflows/release.yml"
grep -q 'group: lumen-release' "$ROOT/.github/workflows/release.yml"
grep -q 'minisign-0.12-linux.tar.gz' "$ROOT/.github/workflows/release.yml"
grep -q '9a599b48ba6eb7b1e80f12f36b94ceca7c00b7a5173c95c3efc88d9822957e73' "$ROOT/.github/workflows/release.yml"
grep -q -- '--require-hashes' "$ROOT/.github/workflows/release.yml"
grep -q 'spdx-tools==0.8.5 --hash=sha256:7c2d5865941be9d2e898f5b084e8d5422dd298dc5a29320ddb198fec304f59c4' \
  "$ROOT/.github/workflows/release.yml"
grep -q -- '--latest=false' "$ROOT/.github/workflows/release.yml"
grep -q -- '--repo "$GH_REPO"' "$ROOT/.github/workflows/release.yml"
grep -q 'GH_REPO:' "$ROOT/.github/workflows/release.yml"
if grep -q 'origin/${{' "$ROOT/.github/workflows/release.yml"; then
  echo "FAIL: default branch expression is interpolated directly into shell" >&2
  exit 1
fi
extract_workflow_step() {
  local step_name="$1"
  local output="$2"
  ruby -ryaml -e '
    workflow = YAML.safe_load(File.read(ARGV.fetch(0)), aliases: true)
    step = workflow.fetch("jobs").fetch("publish").fetch("steps").find {
      |item| item["name"] == ARGV.fetch(1)
    }
    abort "workflow step missing: #{ARGV.fetch(1)}" unless step
    puts step.fetch("run")
  ' "$ROOT/.github/workflows/release.yml" "$step_name" >"$output"
}

UNSIGNED_VALIDATOR_SCRIPT="$TMP/validate-unsigned-workflow.sh"
extract_workflow_step "Validate downloaded unsigned release contract" "$UNSIGNED_VALIDATOR_SCRIPT"
bash -n "$UNSIGNED_VALIDATOR_SCRIPT"

run_unsigned_validator_fixture() {
  local scenario="$1"
  local field="${2:-}"
  local value="${3:-}"
  local work="$TMP/unsigned-$scenario"
  rm -rf "$work"
  mkdir -p "$work"
  cp -R "$TMP/unsigned-template" "$work/dist"
  if [[ -n "$field" ]]; then
    python3 - "$work/dist/lumen-release-manifest.json" "$field" "$value" <<'PY'
import json
import sys
from pathlib import Path

path = Path(sys.argv[1])
document = json.loads(path.read_text())
value = json.loads(sys.argv[3])
if value == "__DUPLICATE_ASSET__":
    document["assets"][1] = document["assets"][0].copy()
    path.write_text(json.dumps(document, indent=2, sort_keys=True) + "\n")
    raise SystemExit(0)
target = document
parts = sys.argv[2].split(".")
for part in parts[:-1]:
    target = target[int(part)] if part.isdigit() else target[part]
last = parts[-1]
current = target[int(last)] if last.isdigit() else target[last]
if value == "__FLOAT_CURRENT__":
    value = float(current)
if last.isdigit():
    target[int(last)] = value
else:
    target[last] = value
path.write_text(json.dumps(document, indent=2, sort_keys=True) + "\n")
PY
  fi
  (
    cd "$work"
    export GITHUB_REF_NAME=v1.2.3-alpha.4
    export GITHUB_SHA="$FIXTURE_COMMIT"
    bash "$UNSIGNED_VALIDATOR_SCRIPT"
  )
}

run_unsigned_validator_fixture valid
for fixture in \
  'schema-version|schemaVersion|999|unsigned manifest schemaVersion mismatch' \
  'schema-version-bool|schemaVersion|true|unsigned manifest schemaVersion mismatch' \
  'schema-version-float|schemaVersion|1.0|unsigned manifest schemaVersion mismatch' \
  'product|product|"other"|unsigned manifest product mismatch' \
  'version|version|"9.9.9"|unsigned manifest version mismatch' \
  'version-source|versionSource|"Other.toml"|unsigned manifest versionSource mismatch' \
  'tag|tag|"v9.9.9"|unsigned manifest tag mismatch' \
  'commit|commit|"0000000000000000000000000000000000000000"|unsigned manifest commit mismatch' \
  'signing-scheme|signing.scheme|"wrong"|unsigned manifest signing.scheme mismatch' \
  'signing-identity|signing.identity|"wrong"|unsigned manifest signing.identity mismatch' \
  'signing-key|signing.publicKey|"wrong"|unsigned manifest signing.publicKey mismatch' \
  'signing-fingerprint|signing.publicKeyFingerprint|"wrong"|unsigned manifest signing.publicKeyFingerprint mismatch' \
  'signing-key-asset|signing.publicKeyAsset|"wrong.pub"|unsigned manifest signing.publicKeyAsset mismatch' \
  'signing-key-sha|signing.publicKeyAssetSha256|"wrong"|unsigned manifest signing.publicKeyAssetSha256 mismatch' \
  'signing-manifest-sig|signing.manifestSignature|"wrong.sig"|unsigned manifest signing.manifestSignature mismatch' \
  'asset-name|assets.0.name|"wrong"|unsigned manifest platform set mismatch' \
  'asset-target|assets.0.target|"wrong"|asset lumen-macos-arm64.target mismatch' \
  'asset-hash|assets.0.sha256|"wrong"|asset lumen-macos-arm64.sha256 mismatch' \
  'asset-size|assets.0.size|999|asset lumen-macos-arm64.size mismatch' \
  'asset-size-float|assets.0.size|"__FLOAT_CURRENT__"|asset lumen-macos-arm64.size mismatch' \
  'asset-signature|assets.0.signature|"wrong"|asset lumen-macos-arm64.signature mismatch' \
  'sbom-name|assets.0.sbom.name|"wrong"|SBOM lumen-macos-arm64.name mismatch' \
  'sbom-target|assets.0.sbom.target|"wrong"|SBOM lumen-macos-arm64.target mismatch' \
  'sbom-hash|assets.0.sbom.sha256|"wrong"|SBOM lumen-macos-arm64.sha256 mismatch' \
  'sbom-size|assets.0.sbom.size|999|SBOM lumen-macos-arm64.size mismatch' \
  'sbom-size-float|assets.0.sbom.size|"__FLOAT_CURRENT__"|SBOM lumen-macos-arm64.size mismatch' \
  'sbom-signature|assets.0.sbom.signature|"wrong"|SBOM lumen-macos-arm64.signature mismatch'
do
  IFS='|' read -r label field value expected_error <<<"$fixture"
  expect_fail "workflow-unsigned-$label" "$expected_error" \
    run_unsigned_validator_fixture "$label" "$field" "$value"
done
expect_fail workflow-unsigned-duplicate-assets "duplicate asset names" \
  run_unsigned_validator_fixture duplicate-assets assets '"__DUPLICATE_ASSET__"'
echo "OK: extracted production unsigned validator rejects semantic manifest tampering"

PUBLISH_SCRIPT="$TMP/publish-release.sh"
extract_workflow_step "Publish existing tag and exact Lumen assets" "$PUBLISH_SCRIPT"
bash -n "$PUBLISH_SCRIPT"

mkdir -p "$TMP/mock-bin" "$TMP/publish-template/dist"
python3 - "$TMP/publish-template/dist" <<'PY'
from pathlib import Path
import sys

dist = Path(sys.argv[1])
payloads = {
    "lumen-macos-arm64", "lumen-macos-arm64.spdx.json",
    "lumen-macos-amd64", "lumen-macos-amd64.spdx.json",
    "lumen-linux-arm64", "lumen-linux-arm64.spdx.json",
    "lumen-linux-amd64", "lumen-linux-amd64.spdx.json",
    "lumen-release.pub", "lumen-release-manifest.json",
}
names = payloads | {f"{name}.minisig" for name in payloads if name != "lumen-release.pub"}
for name in names:
    (dist / name).write_bytes(f"fixture release asset {name}\n".encode())
PY

cat >"$TMP/mock-bin/gh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
[[ ! -e .git ]]
[[ "$GH_REPO" == example/lumen ]]
printf '%s\n' "$*" >>"$GH_LOG"

metadata() {
  local draft=true immutable=false
  if [[ -e "$GH_STATE/published" || "$GH_SCENARIO" == published-nonimmutable || "$GH_SCENARIO" == published-immutable-rerun ]]; then
    draft=false
  fi
  if [[ "$draft" == false && "$GH_SCENARIO" != published-nonimmutable ]]; then
    immutable=true
  fi
  printf '{"databaseId":123,"isDraft":%s,"isImmutable":%s,"tagName":"%s"}\n' \
    "$draft" "$immutable" "$GITHUB_REF_NAME"
}

assets() {
  python3 - "$GH_SCENARIO" <<'PY'
import hashlib
import json
import sys
from pathlib import Path

scenario = sys.argv[1]
items = []
for path in sorted(Path("dist").iterdir()):
    if path.is_file():
        items.append({
            "name": path.name,
            "digest": f"sha256:{hashlib.sha256(path.read_bytes()).hexdigest()}",
            "size": path.stat().st_size,
            "state": "uploaded",
        })
if scenario == "extra-asset":
    items.append({"name": "stale-extra", "digest": "sha256:" + "0" * 64, "size": 0, "state": "uploaded"})
elif scenario == "wrong-digest":
    items[0]["digest"] = "sha256:" + "0" * 64
elif scenario == "wrong-size":
    items[0]["size"] += 1
elif scenario == "wrong-state":
    items[0]["state"] = "new"
elif scenario == "truncated-assets":
    items.pop()
print(json.dumps([items]))
PY
}

if [[ "$1" == api && " $* " == *'/git/ref/tags/'* ]]; then
  tag_calls=0
  [[ ! -f "$GH_STATE/tag-ref-calls" ]] || read -r tag_calls <"$GH_STATE/tag-ref-calls"
  ((tag_calls += 1))
  printf '%s\n' "$tag_calls" >"$GH_STATE/tag-ref-calls"
  tag_sha="$GITHUB_SHA"
  case "$GH_SCENARIO:$tag_calls" in
    tag-move-before-create:1|tag-move-during-upload:2|tag-move-before-publish:3|tag-move-after-publish:4)
      tag_sha=0000000000000000000000000000000000000000
      ;;
  esac
  if [[ "$GH_SCENARIO" == annotated-tag-success ]]; then
    printf 'tag\t%s\n' 1111111111111111111111111111111111111111
  else
    printf 'commit\t%s\n' "$tag_sha"
  fi
elif [[ "$1" == api && " $* " == *'/git/tags/'* ]]; then
  [[ "$GH_SCENARIO" == annotated-tag-success ]]
  [[ " $* " == *'/git/tags/1111111111111111111111111111111111111111 '* ]]
  printf 'commit\t%s\n' "$GITHUB_SHA"
elif [[ "$1" == api && " $* " == *'/releases/123/assets?per_page=100'* ]]; then
  assets
elif [[ "$1 $2" == "release view" ]]; then
  [[ " $* " == *' --repo example/lumen '* ]]
  if [[ "$GH_SCENARIO" == first-success && ! -e "$GH_STATE/created" ]]; then
    exit 1
  fi
  metadata
elif [[ "$1 $2" == "release create" ]]; then
  [[ "$GH_SCENARIO" == first-success ]]
  touch "$GH_STATE/created"
elif [[ "$1 $2" == "release upload" ]]; then
  [[ " $* " == *' --repo example/lumen '* ]]
elif [[ "$1 $2" == "release edit" ]]; then
  [[ " $* " == *' --repo example/lumen '* ]]
  [[ " $* " == *' --draft=false '* ]]
  if [[ "$GH_SCENARIO" == edit-fail-draft ]]; then
    exit 1
  fi
  touch "$GH_STATE/published"
  if [[ "$GH_SCENARIO" == edit-timeout-published ]]; then
    exit 1
  fi
else
  echo "FAIL: unexpected gh invocation: $*" >&2
  exit 1
fi
EOF
chmod +x "$TMP/mock-bin/gh"

run_publish_fixture() {
  local scenario="$1"
  local state="$TMP/gh-$scenario"
  local work="$TMP/no-git-$scenario"
  rm -rf "$state" "$work"
  mkdir -p "$state" "$work"
  cp -R "$TMP/publish-template/dist" "$work/dist"
  : >"$state/calls.log"
  (
    cd "$work"
    export GH_REPO=example/lumen
    export GH_SCENARIO="$scenario"
    export GH_STATE="$state"
    export GH_LOG="$state/calls.log"
    export GITHUB_REF_NAME=v1.2.3-alpha.4
    export GITHUB_SHA=0123456789abcdef0123456789abcdef01234567
    export RUNNER_TEMP="$state"
    export PATH="$TMP/mock-bin:$PATH"
    bash "$PUBLISH_SCRIPT"
  )
}

run_publish_fixture first-success
grep -Fq 'release create' "$TMP/gh-first-success/calls.log"
grep -Fq -- '--draft=false' "$TMP/gh-first-success/calls.log"
echo "OK: first release create, upload, and immutable publish succeeds"

run_publish_fixture existing-draft-success
if grep -Fq 'release create' "$TMP/gh-existing-draft-success/calls.log"; then
  echo "FAIL: existing draft fixture unexpectedly created a second release" >&2
  exit 1
fi
grep -Fq -- '--draft=false' "$TMP/gh-existing-draft-success/calls.log"
echo "OK: existing draft upload and immutable publish succeeds"

run_publish_fixture annotated-tag-success
grep -Fq '/git/tags/1111111111111111111111111111111111111111' \
  "$TMP/gh-annotated-tag-success/calls.log"
echo "OK: annotated release tag peels to the exact workflow commit"

for fixture in \
  'extra-asset|remote release asset set mismatch' \
  'wrong-digest|digest mismatch' \
  'wrong-size|size mismatch' \
  'wrong-state|state is not uploaded' \
  'truncated-assets|remote release asset set mismatch'
do
  IFS='|' read -r scenario expected_error <<<"$fixture"
  expect_fail "publish-$scenario" "$expected_error" run_publish_fixture "$scenario"
  if grep -Fq -- '--draft=false' "$TMP/gh-$scenario/calls.log"; then
    echo "FAIL: $scenario was published before exact remote asset validation" >&2
    exit 1
  fi
done
echo "OK: extra, truncated, wrong-digest, wrong-size, and wrong-state assets all fail before publish"

for fixture in \
  'tag-move-before-create|tag moved before draft creation' \
  'tag-move-during-upload|tag moved during asset upload' \
  'tag-move-before-publish|tag moved immediately before publish' \
  'tag-move-after-publish|immutable tag/assets contract cannot be proven'
do
  IFS='|' read -r scenario expected_error <<<"$fixture"
  expect_fail "publish-$scenario" "$expected_error" run_publish_fixture "$scenario"
done
if grep -Eq 'release (upload|edit)' "$TMP/gh-tag-move-before-create/calls.log"; then
  echo "FAIL: release mutated after tag moved before initial reconciliation" >&2
  exit 1
fi
for scenario in tag-move-during-upload tag-move-before-publish; do
  if grep -Fq 'release edit' "$TMP/gh-$scenario/calls.log"; then
    echo "FAIL: $scenario reached publish mutation" >&2
    exit 1
  fi
done
grep -Fq 'release edit' "$TMP/gh-tag-move-after-publish/calls.log"
echo "OK: tag movement is rejected before create, during upload, before publish, and after publish"

run_publish_fixture edit-timeout-published
grep -Fq 'reconciliation proves the exact immutable release was published' \
  "$TMP/gh-edit-timeout-published/calls.log" || true
[[ -e "$TMP/gh-edit-timeout-published/published" ]]
echo "OK: ambiguous edit failure reconciles an exact immutable published release"

expect_fail publish-edit-fail-draft "release remains draft" run_publish_fixture edit-fail-draft
[[ ! -e "$TMP/gh-edit-fail-draft/published" ]]
echo "OK: edit failure that remains draft fails safely"

expect_fail publish-nonimmutable "not an exact immutable match" run_publish_fixture published-nonimmutable
if grep -Eq 'release (upload|edit)' "$TMP/gh-published-nonimmutable/calls.log"; then
  echo "FAIL: published nonimmutable release was mutated" >&2
  exit 1
fi
echo "OK: published nonimmutable release is rejected without mutation"

run_publish_fixture published-immutable-rerun
if grep -Eq 'release (create|upload|edit)' "$TMP/gh-published-immutable-rerun/calls.log"; then
  echo "FAIL: exact immutable rerun mutated the published release" >&2
  exit 1
fi
echo "OK: exact immutable published release rerun is idempotent"

if rg -q -- '--draft=true|rollback_to_draft' "$PUBLISH_SCRIPT"; then
  echo "FAIL: immutable release state machine must never attempt post-publish rollback" >&2
  exit 1
fi
echo "OK: explicit repository routing works from a no-.git directory"
echo "OK: workflow release profile, environment gate, and pinned signer contract present"

echo "OK: release contract fixture suite passed"
