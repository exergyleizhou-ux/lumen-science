#!/usr/bin/env python3
"""Fail-closed contract for Lumen release tags and release assets."""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path


VERSION_SOURCE = Path("VERSION")
CARGO_VERSION_SOURCE = Path("agent/crates/codegen/xai-grok-pager-bin/Cargo.toml")
SIGNING_IDENTITY = "lumen-release-v1"
PUBLIC_KEY_ASSET = "lumen-release.pub"
MANIFEST_SIGNATURE = "lumen-release-manifest.json.minisig"
TARGETS = {
    "lumen-macos-arm64": "aarch64-apple-darwin",
    "lumen-macos-amd64": "x86_64-apple-darwin",
    "lumen-linux-arm64": "aarch64-unknown-linux-gnu",
    "lumen-linux-amd64": "x86_64-unknown-linux-gnu",
}
FORBIDDEN_IDENTITY = re.compile(r"(?:^|[-_.])(grok|xai)(?:[-_.]|$)", re.IGNORECASE)


class ContractError(RuntimeError):
    pass


def fail(message: str) -> None:
    raise ContractError(message)


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def release_version(root: Path) -> str:
    version_path = root / VERSION_SOURCE
    if not version_path.is_file():
        fail(f"version source missing: {version_path}")
    raw_version = version_path.read_text(encoding="utf-8")
    if not raw_version.endswith("\n") or raw_version.count("\n") != 1:
        fail("VERSION must contain exactly one newline-terminated SemVer")
    version = raw_version.rstrip("\n")
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?", version):
        fail(f"VERSION is not SemVer: {version}")

    cargo = root / CARGO_VERSION_SOURCE
    if not cargo.is_file():
        fail(f"Cargo version mirror missing: {cargo}")
    in_package = False
    for raw_line in cargo.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if line.startswith("["):
            in_package = line == "[package]"
            continue
        if in_package:
            match = re.fullmatch(r'version\s*=\s*"([^"]+)"(?:\s*#.*)?', line)
            if match:
                cargo_version = match.group(1)
                if cargo_version != version:
                    fail(
                        f"VERSION/Cargo mismatch: VERSION has {version}, "
                        f"{CARGO_VERSION_SOURCE} has {cargo_version}"
                    )
                return version
    fail(f"[package].version missing from {cargo}")


def validate_tag(tag: str, version: str) -> None:
    expected = f"v{version}"
    if tag != expected:
        fail(f"tag/version mismatch: expected {expected}, got {tag}")


def git(root: Path, *args: str) -> str:
    result = subprocess.run(
        ["git", "-C", str(root), *args],
        check=False,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if result.returncode != 0:
        fail(f"git {' '.join(args)} failed: {result.stderr.strip()}")
    return result.stdout.strip()


def preflight(args: argparse.Namespace) -> None:
    root = args.root.resolve()
    version = release_version(root)
    validate_tag(args.tag, version)
    if args.require_clean:
        dirty = git(root, "status", "--porcelain", "--untracked-files=all")
        if dirty:
            fail(f"formal release requires a clean tree:\n{dirty}")
    if args.require_head_tag:
        head = git(root, "rev-parse", "HEAD")
        tagged = git(root, "rev-list", "-n", "1", args.tag)
        if head != tagged:
            fail(f"tag {args.tag} points to {tagged}, not checked-out HEAD {head}")
    if args.allowed_ref:
        tagged = git(root, "rev-list", "-n", "1", args.tag)
        result = subprocess.run(
            ["git", "-C", str(root), "merge-base", "--is-ancestor", tagged, args.allowed_ref],
            check=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        if result.returncode != 0:
            fail(f"tag commit {tagged} is not an ancestor of authorized ref {args.allowed_ref}")
    print(f"OK: release preflight tag={args.tag} version={version}")


def read_public_key(path: Path) -> tuple[str, str]:
    if not path.is_file():
        fail(f"minisign public key missing: {path}")
    lines = [line.strip() for line in path.read_text(encoding="utf-8").splitlines() if line.strip()]
    if len(lines) != 2 or not lines[0].startswith("untrusted comment:"):
        fail("minisign public key must contain one comment line and one base64 key line")
    try:
        raw = base64.b64decode(lines[1], validate=True)
    except ValueError as exc:
        fail(f"invalid minisign public key base64: {exc}")
    if len(raw) != 42 or raw[:2] != b"Ed":
        fail("invalid minisign Ed25519 public key payload")
    return lines[1], f"sha256:{hashlib.sha256(raw).hexdigest()}"


def expected_payload_names() -> set[str]:
    binaries = set(TARGETS)
    sboms = {f"{name}.spdx.json" for name in TARGETS}
    return binaries | sboms | {"lumen-release.pub", "lumen-release-manifest.json"}


def expected_unsigned_names() -> set[str]:
    return expected_payload_names()


def reject_legacy_name(name: str) -> None:
    if not name.startswith("lumen-"):
        fail(f"release file is not lumen-namespaced: {name}")
    if FORBIDDEN_IDENTITY.search(name):
        fail(f"legacy product identity forbidden in release file: {name}")


def validate_sbom(
    path: Path,
    binary: Path,
    asset_name: str,
    version: str,
    target: str,
    commit: str,
    tag: str,
) -> None:
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        fail(f"invalid SBOM {path.name}: {exc}")
    required_scalars = {
        "spdxVersion": "SPDX-2.3",
        "dataLicense": "CC0-1.0",
        "SPDXID": "SPDXRef-DOCUMENT",
        "name": f"{asset_name}-{version}",
        "documentNamespace": f"https://lumen.local/spdx/{tag}/{target}",
    }
    for field, expected in required_scalars.items():
        if document.get(field) != expected:
            fail(f"SBOM {path.name} {field} mismatch: expected {expected!r}")
    creation = document.get("creationInfo")
    if not isinstance(creation, dict) or not creation.get("created") or not creation.get("creators"):
        fail(f"SBOM {path.name} has incomplete creationInfo")
    expected_creators = {
        "Tool: scripts/generate-release-sbom.sh",
        "Organization: Lumen",
    }
    if set(creation.get("creators", [])) != expected_creators:
        fail(f"SBOM {path.name} creator identity mismatch")
    packages = document.get("packages")
    files = document.get("files")
    relationships = document.get("relationships")
    if not isinstance(packages, list) or not packages:
        fail(f"SBOM {path.name} has no packages")
    if not isinstance(files, list) or not files:
        fail(f"SBOM {path.name} has no files")
    if not isinstance(relationships, list) or not relationships:
        fail(f"SBOM {path.name} has no relationships")

    asset_package = next(
        (package for package in packages if package.get("SPDXID") == "SPDXRef-Package-lumen-release-asset"),
        None,
    )
    source_package = next(
        (package for package in packages if package.get("SPDXID") == "SPDXRef-Package-lumen-source"),
        None,
    )
    if (
        not asset_package
        or asset_package.get("name") != asset_name
        or asset_package.get("versionInfo") != version
        or asset_package.get("supplier") != "Organization: Lumen authors"
    ):
        fail(f"SBOM {path.name} does not describe the release asset package")
    if (
        not source_package
        or source_package.get("name") != "lumen-source"
        or source_package.get("versionInfo") != commit
        or source_package.get("supplier") != "Organization: Lumen authors"
    ):
        fail(f"SBOM {path.name} source package commit mismatch")

    cargo_components = [
        package for package in packages
        if package.get("SPDXID") not in {
            "SPDXRef-Package-lumen-release-asset",
            "SPDXRef-Package-lumen-source",
        }
    ]
    if not cargo_components or not any(package.get("name") == "xai-grok-pager-bin" for package in cargo_components):
        fail(f"SBOM {path.name} is not rooted at xai-grok-pager-bin")
    for package in cargo_components:
        for license_field in ("licenseConcluded", "licenseDeclared"):
            if "/" in str(package.get(license_field, "")):
                fail(f"SBOM {path.name} contains an invalid slash license expression")
        location = package.get("downloadLocation")
        if location != "NOASSERTION" and not str(location).startswith("https://"):
            fail(f"SBOM {path.name} contains invalid component downloadLocation")
        refs = package.get("externalRefs", [])
        if not any(
            ref.get("referenceCategory") == "PACKAGE-MANAGER"
            and ref.get("referenceType") == "purl"
            and str(ref.get("referenceLocator", "")).startswith("pkg:cargo/")
            for ref in refs
        ):
            fail(f"SBOM {path.name} cargo component lacks purl externalRef")

    binary_hash = sha256(binary)
    binary_sha1 = hashlib.sha1(binary.read_bytes()).hexdigest()
    binary_file = next(
        (item for item in files if item.get("SPDXID") == "SPDXRef-File-lumen-release-binary"),
        None,
    )
    if not binary_file or binary_file.get("fileName") != f"./{asset_name}":
        fail(f"SBOM {path.name} binary file binding is missing")
    checksums = binary_file.get("checksums", [])
    for checksum in (
        {"algorithm": "SHA1", "checksumValue": binary_sha1},
        {"algorithm": "SHA256", "checksumValue": binary_hash},
    ):
        if checksum not in checksums:
            fail(f"SBOM {path.name} does not contain final binary checksum {checksum['algorithm']}")

    required_relationships = [
        ("SPDXRef-DOCUMENT", "DESCRIBES", "SPDXRef-Package-lumen-release-asset"),
        ("SPDXRef-Package-lumen-release-asset", "CONTAINS", "SPDXRef-File-lumen-release-binary"),
        ("SPDXRef-Package-lumen-release-asset", "GENERATED_FROM", "SPDXRef-Package-lumen-source"),
    ]
    actual_relationships = {
        (item.get("spdxElementId"), item.get("relationshipType"), item.get("relatedSpdxElement"))
        for item in relationships
    }
    for relationship in required_relationships:
        if relationship not in actual_relationships:
            fail(f"SBOM {path.name} missing relationship {relationship}")

    release_annotation = None
    for annotation in document.get("annotations", []):
        try:
            comment = json.loads(annotation.get("comment", ""))
        except (TypeError, json.JSONDecodeError):
            continue
        if comment.get("identity") == SIGNING_IDENTITY:
            release_annotation = comment
            break
    expected_annotation = {
        "asset": asset_name,
        "target": target,
        "version": version,
        "tag": tag,
        "commit": commit,
        "binary_sha1": binary_sha1,
        "binary_sha256": binary_hash,
        "binary_size": binary.stat().st_size,
        "identity": SIGNING_IDENTITY,
        "cargo_root": "xai-grok-pager-bin",
    }
    if release_annotation is None or any(release_annotation.get(key) != value for key, value in expected_annotation.items()):
        fail(f"SBOM {path.name} release annotation does not bind the final binary")

    document_identity = f"{document.get('name', '')} {document.get('documentNamespace', '')}".casefold()
    for forbidden in ("x.ai/cli", "xai-org-shared/grok-build"):
        if forbidden.casefold() in document_identity:
            fail(f"legacy product identity forbidden at SBOM document level: {forbidden}")


def assemble(args: argparse.Namespace) -> None:
    root = args.root.resolve()
    dist = args.dist.resolve()
    version = release_version(root)
    validate_tag(args.tag, version)
    if not re.fullmatch(r"[0-9a-f]{40}", args.commit):
        fail("release commit must be a full 40-character lowercase git SHA")

    public_key_line, fingerprint = read_public_key(args.public_key.resolve())
    allowed_before_signing = (
        set(TARGETS)
        | {f"{name}.spdx.json" for name in TARGETS}
        | {"lumen-release.pub"}
    )
    actual = {path.name for path in dist.iterdir() if path.is_file()}
    # Re-assembly is allowed in the no-secret verification job; the manifest is
    # deterministically replaced after substituting a temporary test key.
    actual.discard("lumen-release-manifest.json")
    if actual != allowed_before_signing:
        fail(
            "release payload mismatch before signing: "
            f"missing={sorted(allowed_before_signing - actual)} extra={sorted(actual - allowed_before_signing)}"
        )
    for name in actual:
        reject_legacy_name(name)

    assets = []
    for name, target in TARGETS.items():
        binary = dist / name
        sbom_name = f"{name}.spdx.json"
        sbom = dist / sbom_name
        validate_sbom(sbom, binary, name, version, target, args.commit, args.tag)
        assets.append(
            {
                "name": name,
                "target": target,
                "sha256": sha256(binary),
                "size": binary.stat().st_size,
                "signature": f"{name}.minisig",
                "sbom": {
                    "name": sbom_name,
                    "target": target,
                    "sha256": sha256(sbom),
                    "size": sbom.stat().st_size,
                    "signature": f"{sbom_name}.minisig",
                },
            }
        )

    manifest = {
        "schemaVersion": 1,
        "product": "lumen",
        "version": version,
        "tag": args.tag,
        "commit": args.commit,
        "versionSource": str(VERSION_SOURCE),
        "signing": {
            "scheme": "minisign-ed25519-prehashed",
            "identity": SIGNING_IDENTITY,
            "publicKey": public_key_line,
            "publicKeyFingerprint": fingerprint,
            "publicKeyAsset": PUBLIC_KEY_ASSET,
            "publicKeyAssetSha256": sha256(args.public_key.resolve()),
            "manifestSignature": MANIFEST_SIGNATURE,
        },
        "assets": assets,
    }
    output = dist / "lumen-release-manifest.json"
    output.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"OK: assembled {output} with {len(assets)} platform assets")


def run_minisign(
    executable: str,
    public_key: Path,
    message: Path,
    signature: Path,
    tag: str,
) -> None:
    result = subprocess.run(
        [executable, "-V", "-p", str(public_key), "-m", str(message), "-x", str(signature)],
        check=False,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if result.returncode != 0:
        detail = (result.stderr or result.stdout).strip()
        fail(f"signature verification failed for {message.name}: {detail}")
    expected_comment = f"Trusted comment: {SIGNING_IDENTITY} {tag} {message.name}"
    output_lines = (result.stdout + "\n" + result.stderr).splitlines()
    if expected_comment not in output_lines:
        fail(
            f"signature trusted comment mismatch for {message.name}: "
            f"expected {expected_comment!r}"
        )


def validate_unsigned_contract(
    root: Path,
    dist: Path,
    tag: str,
    expected_commit: str,
    public_key: Path,
    *,
    require_unsigned_boundary: bool,
) -> tuple[Path, list[Path]]:
    version = release_version(root)
    validate_tag(tag, version)
    if not re.fullmatch(r"[0-9a-f]{40}", expected_commit):
        fail("expected commit must be a full 40-character lowercase git SHA")
    public_key_line, fingerprint = read_public_key(public_key)
    manifest_path = dist / "lumen-release-manifest.json"
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        fail(f"invalid release manifest: {exc}")

    serialized = json.dumps(manifest, sort_keys=True)
    for forbidden in ("x.ai/cli", "xai-org-shared/grok-build"):
        if forbidden in serialized:
            fail(f"legacy product identity forbidden in manifest: {forbidden}")
    expected_manifest_fields = {
        "schemaVersion", "product", "version", "tag", "commit",
        "versionSource", "signing", "assets",
    }
    if set(manifest) != expected_manifest_fields:
        fail(
            "release manifest field set mismatch: "
            f"missing={sorted(expected_manifest_fields - set(manifest))} "
            f"extra={sorted(set(manifest) - expected_manifest_fields)}"
        )
    if (
        type(manifest.get("schemaVersion")) is not int
        or manifest.get("schemaVersion") != 1
        or manifest.get("product") != "lumen"
    ):
        fail("unsupported or non-Lumen release manifest")
    if manifest.get("version") != version or manifest.get("tag") != tag:
        fail("manifest tag/version does not match Cargo version source")
    if manifest.get("versionSource") != str(VERSION_SOURCE):
        fail(f"manifest version source must be {VERSION_SOURCE}")
    if manifest.get("commit") != expected_commit:
        fail(
            "manifest commit does not match expected tag commit: "
            f"expected {expected_commit}, got {manifest.get('commit')}"
        )
    signing = manifest.get("signing")
    expected_signing_fields = {
        "scheme", "identity", "publicKey", "publicKeyFingerprint",
        "publicKeyAsset", "publicKeyAssetSha256", "manifestSignature",
    }
    if not isinstance(signing, dict) or set(signing) != expected_signing_fields:
        actual_signing_fields = set(signing) if isinstance(signing, dict) else set()
        fail(
            "manifest signing field set mismatch: "
            f"missing={sorted(expected_signing_fields - actual_signing_fields)} "
            f"extra={sorted(actual_signing_fields - expected_signing_fields)}"
        )
    if signing.get("scheme") != "minisign-ed25519-prehashed" or signing.get("identity") != SIGNING_IDENTITY:
        fail("manifest signing scheme or identity mismatch")
    if signing.get("publicKey") != public_key_line or signing.get("publicKeyFingerprint") != fingerprint:
        fail("manifest public key does not match pinned verification key")
    if signing.get("publicKeyAsset") != PUBLIC_KEY_ASSET:
        fail(f"manifest public key asset must be {PUBLIC_KEY_ASSET}")
    if signing.get("manifestSignature") != MANIFEST_SIGNATURE:
        fail(f"manifest signature asset must be {MANIFEST_SIGNATURE}")
    if signing.get("publicKeyAssetSha256") != sha256(public_key):
        fail("public key asset checksum mismatch")

    assets = manifest.get("assets")
    if not isinstance(assets, list) or len(assets) != len(TARGETS):
        fail("manifest must contain exactly four platform assets")
    if not all(isinstance(asset, dict) for asset in assets):
        fail("manifest platform assets must be objects")
    names = [asset.get("name") for asset in assets]
    if len(set(names)) != len(names):
        fail("manifest contains duplicate platform asset names")
    by_name = {asset.get("name"): asset for asset in assets}
    if set(by_name) != set(TARGETS):
        fail(f"manifest platform set mismatch: {sorted(by_name)}")

    signed_payloads = [manifest_path]
    for name, target in TARGETS.items():
        reject_legacy_name(name)
        asset = by_name[name]
        expected_asset_fields = {"name", "target", "sha256", "size", "signature", "sbom"}
        if set(asset) != expected_asset_fields:
            fail(f"manifest binary field set mismatch for {name}")
        if asset.get("target") != target:
            fail(f"target mismatch for {name}")
        binary = dist / name
        if (
            not binary.is_file()
            or asset.get("sha256") != sha256(binary)
            or type(asset.get("size")) is not int
            or asset.get("size") != binary.stat().st_size
        ):
            fail(f"binary hash/size mismatch for {name}")
        if asset.get("signature") != f"{name}.minisig":
            fail(f"signature name mismatch for {name}")
        sbom_meta = asset.get("sbom")
        expected_sbom_fields = {"name", "target", "sha256", "size", "signature"}
        if not isinstance(sbom_meta, dict) or set(sbom_meta) != expected_sbom_fields:
            fail(f"manifest SBOM field set mismatch for {name}")
        sbom_name = f"{name}.spdx.json"
        sbom = dist / sbom_name
        if (
            sbom_meta.get("name") != sbom_name
            or sbom_meta.get("target") != target
            or not sbom.is_file()
            or sbom_meta.get("sha256") != sha256(sbom)
            or type(sbom_meta.get("size")) is not int
            or sbom_meta.get("size") != sbom.stat().st_size
        ):
            fail(f"SBOM target/hash/size/name mismatch for {name}")
        if sbom_meta.get("signature") != f"{sbom_name}.minisig":
            fail(f"SBOM signature name mismatch for {name}")
        validate_sbom(sbom, binary, name, version, target, expected_commit, tag)
        signed_payloads.extend([binary, sbom])

    if require_unsigned_boundary:
        actual_files = {path.name for path in dist.iterdir() if path.is_file()}
        expected_files = expected_unsigned_names()
        if actual_files != expected_files:
            fail(
                "unsigned release file set mismatch: "
                f"missing={sorted(expected_files - actual_files)} extra={sorted(actual_files - expected_files)}"
            )
        for name in actual_files:
            reject_legacy_name(name)
    return manifest_path, signed_payloads


def validate_unsigned(args: argparse.Namespace) -> None:
    validate_unsigned_contract(
        args.root.resolve(),
        args.dist.resolve(),
        args.tag,
        args.expected_commit,
        args.public_key.resolve(),
        require_unsigned_boundary=True,
    )
    print(f"OK: validated unsigned release contract for {args.tag} ({len(TARGETS)} targets)")


def verify(args: argparse.Namespace) -> None:
    root = args.root.resolve()
    dist = args.dist.resolve()
    public_key = args.public_key.resolve()
    manifest_path, signed_payloads = validate_unsigned_contract(
        root,
        dist,
        args.tag,
        args.expected_commit,
        public_key,
        require_unsigned_boundary=False,
    )

    expected_files = expected_payload_names() | {f"{path.name}.minisig" for path in signed_payloads}
    actual_files = {path.name for path in dist.iterdir() if path.is_file()}
    if actual_files != expected_files:
        fail(
            "signed release file set mismatch: "
            f"missing={sorted(expected_files - actual_files)} extra={sorted(actual_files - expected_files)}"
        )
    for name in actual_files:
        reject_legacy_name(name)
    for payload in signed_payloads:
        signature = dist / f"{payload.name}.minisig"
        if not signature.is_file() or signature.stat().st_size == 0:
            fail(f"signature missing or empty: {signature.name}")
        run_minisign(args.minisign, public_key, payload, signature, args.tag)
    print(f"OK: verified signed release contract for {args.tag} ({len(TARGETS)} targets)")


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__)
    subparsers = result.add_subparsers(dest="command", required=True)

    preflight_parser = subparsers.add_parser("preflight")
    preflight_parser.add_argument("--root", type=Path, default=Path.cwd())
    preflight_parser.add_argument("--tag", required=True)
    preflight_parser.add_argument("--require-clean", action="store_true")
    preflight_parser.add_argument("--require-head-tag", action="store_true")
    preflight_parser.add_argument("--allowed-ref")
    preflight_parser.set_defaults(func=preflight)

    assemble_parser = subparsers.add_parser("assemble")
    assemble_parser.add_argument("--root", type=Path, default=Path.cwd())
    assemble_parser.add_argument("--dist", type=Path, required=True)
    assemble_parser.add_argument("--tag", required=True)
    assemble_parser.add_argument("--commit", required=True)
    assemble_parser.add_argument("--public-key", type=Path, required=True)
    assemble_parser.set_defaults(func=assemble)

    unsigned_parser = subparsers.add_parser("validate-unsigned")
    unsigned_parser.add_argument("--root", type=Path, default=Path.cwd())
    unsigned_parser.add_argument("--dist", type=Path, required=True)
    unsigned_parser.add_argument("--tag", required=True)
    unsigned_parser.add_argument("--expected-commit", required=True)
    unsigned_parser.add_argument("--public-key", type=Path, required=True)
    unsigned_parser.set_defaults(func=validate_unsigned)

    verify_parser = subparsers.add_parser("verify")
    verify_parser.add_argument("--root", type=Path, default=Path.cwd())
    verify_parser.add_argument("--dist", type=Path, required=True)
    verify_parser.add_argument("--tag", required=True)
    verify_parser.add_argument("--expected-commit", required=True)
    verify_parser.add_argument("--public-key", type=Path, required=True)
    verify_parser.add_argument("--minisign", default="minisign")
    verify_parser.set_defaults(func=verify)
    return result


def main() -> int:
    args = parser().parse_args()
    try:
        args.func(args)
    except (ContractError, FileNotFoundError) as exc:
        print(f"FAIL: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
