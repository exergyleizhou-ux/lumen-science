#!/usr/bin/env python3
"""Tamper tests for verify-ecosystem-admission.py.

Every case changes a temporary copy of the lock and asserts that the verifier
fails for the intended reason.  The real lock and source tree are read-only.
"""

from __future__ import annotations

import copy
import hashlib
import json
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any, Callable


ROOT = Path(__file__).resolve().parent.parent
LOCK = ROOT / "docs/science/5.0/ecosystem-admission.lock.json"
VERIFIER = ROOT / "scripts/verify-ecosystem-admission.py"


def run(lock: dict[str, Any], *extra: str) -> subprocess.CompletedProcess[str]:
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "lock.json"
        path.write_text(
            json.dumps(lock, ensure_ascii=False, indent=2) + "\n",
            encoding="utf-8",
        )
        return subprocess.run(
            [
                sys.executable,
                str(VERIFIER),
                "--science-repo",
                str(ROOT),
                "--lock",
                str(path),
                *extra,
            ],
            check=False,
            capture_output=True,
            text=True,
        )


def main() -> int:
    pristine = json.loads(LOCK.read_text(encoding="utf-8"))
    results: list[tuple[str, bool, str]] = []

    def check(
        name: str,
        mutate: Callable[[dict[str, Any]], None] | None,
        *,
        expected_exit: int,
        needle: str,
        extra: tuple[str, ...] = (),
    ) -> None:
        candidate = copy.deepcopy(pristine)
        if mutate is not None:
            mutate(candidate)
        proc = run(candidate, *extra)
        output = proc.stdout + proc.stderr
        good = proc.returncode == expected_exit and needle in output
        detail = (
            ""
            if good
            else f"exit={proc.returncode}; wanted exit={expected_exit}, text={needle!r}; "
            f"output={output.strip()[:240]!r}"
        )
        results.append((name, good, detail))

    check(
        "the real lock passes",
        None,
        expected_exit=0,
        needle="PASS: ecosystem admission verified",
    )

    check(
        "a requested repository cannot disappear",
        lambda lock: lock["sources"].pop(),
        expected_exit=1,
        needle="four requested source repositories",
    )

    check(
        "external runtime authority cannot be enabled",
        lambda lock: lock["policy"].__setitem__("external_runtime_authority", "allowed"),
        expected_exit=1,
        needle="external runtime authority is not denied",
    )

    check(
        "a root MIT license cannot override a nested proprietary license",
        lambda lock: lock["policy"].__setitem__("root_license_overrides_nested_license", True),
        expected_exit=1,
        needle="root license must not override nested licenses",
    )

    def admit_proprietary(lock: dict[str, Any]) -> None:
        source = next(
            item for item in lock["sources"] if item["id"] == "qzzqzzb-openclaudescience"
        )
        component = next(
            item for item in source["components"] if item["path"] == "skills/pdf/**"
        )
        component["disposition"] = "adapt"

    check(
        "a proprietary nested skill cannot be relabelled as adaptable",
        admit_proprietary,
        expected_exit=1,
        needle="four proprietary OpenClaudeScience skill trees",
    )

    def remove_clean_room(lock: dict[str, Any]) -> None:
        source = next(
            item for item in lock["sources"] if item["id"] == "qzzqzzb-openclaudescience"
        )
        source["components"] = [
            item
            for item in source["components"]
            if item["disposition"] != "clean-room-reimplement"
        ]

    check(
        "proprietary rejection retains a clean-room replacement route",
        remove_clean_room,
        expected_exit=1,
        needle="clean-room replacement commitment",
    )

    check(
        "a protected commit cannot be replaced with an unrelated SHA",
        lambda lock: lock["protected_foundation"]["commits"][0].__setitem__(
            "sha", "0" * 40
        ),
        expected_exit=1,
        needle="not an ancestor of HEAD",
    )

    check(
        "an actor route source marker cannot disappear",
        lambda lock: lock["protected_foundation"]["required_markers"][0][
            "contains"
        ].append("marker-that-does-not-exist"),
        expected_exit=1,
        needle="protected foundation marker missing",
    )

    check(
        "the carried Science skill count cannot regress",
        lambda lock: lock["protected_foundation"]["carried_ledgers"]["science_skills"].__setitem__(
            "minimum_total", 10_000
        ),
        expected_exit=1,
        needle="Science skill inventory regressed",
    )

    def grant_source_authority(lock: dict[str, Any]) -> None:
        source = next(item for item in lock["sources"] if item["id"] == "jvogan-motif")
        source["runtime_authority"] = "Motif MCP"

    check(
        "an upstream project cannot become a second authority",
        grant_source_authority,
        expected_exit=1,
        needle="attempts to become an execution authority",
    )

    check(
        "license evidence requires a canonical SHA-256",
        lambda lock: lock["sources"][0].__setitem__("license_sha256", "abc"),
        expected_exit=1,
        needle="license_sha256 is malformed",
    )

    check(
        "unknown local source roots are refused",
        None,
        expected_exit=1,
        needle="unknown --source-root ids",
        extra=("--source-root", "not-a-source=/tmp"),
    )

    def run_real_verifier() -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [
                sys.executable,
                str(VERIFIER),
                "--science-repo",
                str(ROOT),
            ],
            check=False,
            capture_output=True,
            text=True,
        )

    def check_file_tamper(
        name: str,
        paths: list[Path],
        mutate: Callable[[], None],
        needle: str,
    ) -> None:
        originals = {path: path.read_bytes() for path in paths}
        try:
            mutate()
            proc = run_real_verifier()
            output = proc.stdout + proc.stderr
            good = proc.returncode == 1 and needle in output
            detail = (
                ""
                if good
                else f"exit={proc.returncode}; wanted {needle!r}; output={output.strip()[:240]!r}"
            )
            results.append((name, good, detail))
        finally:
            for path, value in originals.items():
                path.write_bytes(value)

    catalog_path = ROOT / "packs/science/skills/ecosystem/scp-catalog.json"
    vendor_manifest_path = ROOT / "third_party/internscience-scp/VENDOR_MANIFEST.json"

    def approve_catalog_skill() -> None:
        catalog = json.loads(catalog_path.read_text(encoding="utf-8"))
        catalog["summary"]["approved"] = 1
        catalog["skills"][0]["final_disposition"] = "approved"
        catalog_path.write_text(
            json.dumps(catalog, ensure_ascii=False, indent=2) + "\n",
            encoding="utf-8",
        )

    check_file_tamper(
        "a generated SCP skill cannot silently become approved",
        [catalog_path],
        approve_catalog_skill,
        "unreviewed approved skill",
    )

    def inject_credential_with_matching_hashes() -> None:
        catalog = json.loads(catalog_path.read_text(encoding="utf-8"))
        manifest = json.loads(vendor_manifest_path.read_text(encoding="utf-8"))
        skill = catalog["skills"][0]
        relative = skill["source_path"]
        skill_path = ROOT / "third_party/internscience-scp" / relative
        value = skill_path.read_bytes() + b"\nexample_token = 'sk-" + b"x" * 32 + b"'\n"
        digest = hashlib.sha256(value).hexdigest()
        skill_path.write_bytes(value)
        manifest["vendored_files"][relative] = digest
        skill["vendored_sha256"] = digest
        vendor_manifest_path.write_text(
            json.dumps(manifest, ensure_ascii=False, indent=2) + "\n",
            encoding="utf-8",
        )
        catalog_path.write_text(
            json.dumps(catalog, ensure_ascii=False, indent=2) + "\n",
            encoding="utf-8",
        )

    first_skill_path = ROOT / "third_party/internscience-scp" / json.loads(
        catalog_path.read_text(encoding="utf-8")
    )["skills"][0]["source_path"]
    check_file_tamper(
        "credential-shaped values remain forbidden after matching manifest tampering",
        [catalog_path, vendor_manifest_path, first_skill_path],
        inject_credential_with_matching_hashes,
        "credential-shaped value",
    )

    biomni_catalog_path = (
        ROOT / "packs/science/skills/ecosystem/biomni-tool-catalog.json"
    )
    biomni_manifest_path = (
        ROOT / "third_party/biomni-tool-descriptions/VENDOR_MANIFEST.json"
    )

    def approve_biomni_tool() -> None:
        catalog = json.loads(biomni_catalog_path.read_text(encoding="utf-8"))
        catalog["summary"]["approved"] = 1
        catalog["skills"][0]["final_disposition"] = "approved"
        biomni_catalog_path.write_text(
            json.dumps(catalog, ensure_ascii=False, indent=2) + "\n",
            encoding="utf-8",
        )

    check_file_tamper(
        "a Biomni descriptor cannot silently become an approved Lumen tool",
        [biomni_catalog_path],
        approve_biomni_tool,
        "unreviewed approved tool",
    )

    biomni_catalog = json.loads(biomni_catalog_path.read_text(encoding="utf-8"))
    first_biomni = biomni_catalog["skills"][0]
    first_biomni_path = (
        ROOT
        / "third_party/biomni-tool-descriptions"
        / first_biomni["vendored_path"]
    )

    def make_biomni_descriptor_executable_with_matching_hashes() -> None:
        catalog = json.loads(biomni_catalog_path.read_text(encoding="utf-8"))
        manifest = json.loads(biomni_manifest_path.read_text(encoding="utf-8"))
        skill = catalog["skills"][0]
        relative = skill["vendored_path"]
        descriptor_path = ROOT / "third_party/biomni-tool-descriptions" / relative
        value = descriptor_path.read_bytes() + b"\nprint('must never execute')\n"
        digest = hashlib.sha256(value).hexdigest()
        descriptor_path.write_bytes(value)
        manifest["source_files"][relative] = digest
        manifest["vendored_files"][relative] = digest
        skill["source_sha256"] = digest
        biomni_manifest_path.write_text(
            json.dumps(manifest, ensure_ascii=False, indent=2) + "\n",
            encoding="utf-8",
        )
        biomni_catalog_path.write_text(
            json.dumps(catalog, ensure_ascii=False, indent=2) + "\n",
            encoding="utf-8",
        )

    check_file_tamper(
        "matching hashes cannot turn a Biomni literal descriptor into executable code",
        [biomni_catalog_path, biomni_manifest_path, first_biomni_path],
        make_biomni_descriptor_executable_with_matching_hashes,
        "no longer one inert literal assignment",
    )

    resource_catalog_path = (
        ROOT / "packs/science/skills/ecosystem/biomni-resource-catalog.json"
    )

    def approve_biomni_resource() -> None:
        catalog = json.loads(resource_catalog_path.read_text(encoding="utf-8"))
        catalog["summary"]["approved"] = 1
        catalog["skills"][0]["final_disposition"] = "approved"
        resource_catalog_path.write_text(
            json.dumps(catalog, ensure_ascii=False, indent=2) + "\n",
            encoding="utf-8",
        )

    check_file_tamper(
        "a Biomni data or knowledge record cannot silently become approved",
        [resource_catalog_path],
        approve_biomni_resource,
        "unreviewed approval",
    )

    preview_component_path = (
        ROOT
        / "packs/science-desktop/src/renderer/src/pages/settings/"
        / "SkillImportCandidatePreview.tsx"
    )

    def enable_untrusted_preview_media() -> None:
        text = preview_component_path.read_text(encoding="utf-8")
        preview_component_path.write_text(
            text.replace("allowMedia={false}", "allowMedia={true}"),
            encoding="utf-8",
        )

    check_file_tamper(
        "untrusted Skill previews cannot silently enable remote media",
        [preview_component_path],
        enable_untrusted_preview_media,
        "Open Science preview marker missing",
    )

    copied_protocol = (
        ROOT
        / "third_party/biomni-resource-catalog/protocols/addgene/copied.txt"
    )
    try:
        copied_protocol.parent.mkdir(parents=True, exist_ok=True)
        copied_protocol.write_text(
            "unreviewed publisher protocol body\n", encoding="utf-8"
        )
        proc = run_real_verifier()
        output = proc.stdout + proc.stderr
        good = (
            proc.returncode == 1
            and "protocol bodies were copied" in output
        )
        results.append(
            (
                "unreviewed Biomni protocol bodies cannot enter the vendor tree",
                good,
                ""
                if good
                else f"exit={proc.returncode}; output={output.strip()[:240]!r}",
            )
        )
    finally:
        if copied_protocol.exists():
            copied_protocol.unlink()
        for directory in [copied_protocol.parent, copied_protocol.parent.parent]:
            if directory.exists() and not any(directory.iterdir()):
                directory.rmdir()

    print("test-ecosystem-admission")
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
