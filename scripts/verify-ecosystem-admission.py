#!/usr/bin/env python3
"""Verify the Lumen Science ecosystem admission lock without network access.

The lock is intentionally stricter than a dependency list.  It proves:

* the pre-existing Science authority commits and source markers are still here;
* every external source is pinned to a full commit and has a component verdict;
* no external project is admitted as an execution authority;
* nested proprietary licenses stay rejected even when the repository root is MIT;
* existing Open Science, skill, connector, and Motif ledgers did not disappear.

Optional ``--source-root id=/path`` arguments additionally re-hash exact local
upstream checkouts.  They never fetch or mutate those repositories.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Any


GIT_SHA_RE = re.compile(r"^[0-9a-f]{40}$")
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
ALLOWED_ROOT_LICENSES = {"Apache-2.0", "MIT"}
ALLOWED_DISPOSITIONS = {
    "adopt",
    "adapt",
    "quarantine",
    "reject-authority",
    "reject-data-trust",
    "reject-license",
    "clean-room-reimplement",
}
EXPECTED_SOURCES = {
    "snap-stanford-biomni",
    "jvogan-motif",
    "aipoch-open-science",
    "qzzqzzb-openclaudescience",
}
EXPECTED_TRANSITIVE_SOURCES = {"internscience-scp-skills"}
REQUIRED_PROPRIETARY_REJECTIONS = {
    "skills/docx/**",
    "skills/pdf/**",
    "skills/pptx/**",
    "skills/xlsx/**",
}
FORBIDDEN_COPY_PREFIXES = (
    "third_party/openclaudescience/skills/docx/",
    "third_party/openclaudescience/skills/pdf/",
    "third_party/openclaudescience/skills/pptx/",
    "third_party/openclaudescience/skills/xlsx/",
    "packs/science/skills/openclaudescience/docx/",
    "packs/science/skills/openclaudescience/pdf/",
    "packs/science/skills/openclaudescience/pptx/",
    "packs/science/skills/openclaudescience/xlsx/",
)


def fail(message: str) -> None:
    raise ValueError(message)


def require(condition: bool, message: str) -> None:
    if not condition:
        fail(message)


def run_git(repo: Path, *args: str, allow_failure: bool = False) -> str:
    result = subprocess.run(
        ["git", "-C", str(repo), *args],
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0 and not allow_failure:
        fail(
            f"git {' '.join(args)} failed in {repo}: "
            f"{result.stderr.strip() or result.stdout.strip()}"
        )
    return result.stdout.strip()


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def load_json(path: Path, label: str) -> dict[str, Any]:
    with path.open(encoding="utf-8") as handle:
        value = json.load(handle)
    require(isinstance(value, dict), f"{label} root must be an object")
    return value


def require_relative_path(value: str, label: str) -> None:
    path = Path(value)
    require(value != "", f"{label} is empty")
    require(not path.is_absolute(), f"{label} must be relative")
    require(".." not in path.parts, f"{label} escapes its source root")


def verify_source_shape(source: dict[str, Any], label: str) -> None:
    source_id = source["id"]
    require(isinstance(source_id, str) and source_id, f"{label}.id is missing")
    repository = source["repository"]
    require(
        repository.startswith("https://github.com/") and repository.endswith(".git"),
        f"{label}.repository must be an exact HTTPS GitHub clone URL",
    )
    require(
        GIT_SHA_RE.fullmatch(source["exact_commit"]) is not None,
        f"{label}.exact_commit is not a full Git SHA",
    )
    require(
        source["root_license"] in ALLOWED_ROOT_LICENSES,
        f"{label}.root_license is not an admitted permissive license",
    )
    require_relative_path(source["license_file"], f"{label}.license_file")
    require(
        SHA256_RE.fullmatch(source["license_sha256"]) is not None,
        f"{label}.license_sha256 is malformed",
    )
    require(
        source["runtime_authority"] == "none",
        f"{label} attempts to become an execution authority",
    )
    require(
        isinstance(source["admission_status"], str) and source["admission_status"],
        f"{label}.admission_status is missing",
    )

    components = source["components"]
    require(isinstance(components, list) and components, f"{label}.components is empty")
    paths: set[str] = set()
    for component_index, component in enumerate(components):
        component_label = f"{label}.components[{component_index}]"
        component_path = component["path"]
        require(
            isinstance(component_path, str) and component_path,
            f"{component_label}.path is missing",
        )
        require(component_path not in paths, f"{label} repeats component path {component_path}")
        paths.add(component_path)
        disposition = component["disposition"]
        require(
            disposition in ALLOWED_DISPOSITIONS,
            f"{component_label}.disposition is unsupported: {disposition}",
        )
        require(
            isinstance(component["license"], str) and component["license"],
            f"{component_label}.license is missing",
        )
        require(
            isinstance(component["reason"], str) and len(component["reason"]) >= 20,
            f"{component_label}.reason is not substantive",
        )
        if disposition == "reject-license":
            require(
                "proprietary" in component["license"].lower(),
                f"{component_label} rejects a license without identifying it as proprietary",
            )
            require_relative_path(
                component["license_file"], f"{component_label}.license_file"
            )
            require(
                SHA256_RE.fullmatch(component["license_sha256"]) is not None,
                f"{component_label}.license_sha256 is malformed",
            )

    for proof_index, proof in enumerate(source.get("source_proofs", [])):
        proof_label = f"{label}.source_proofs[{proof_index}]"
        require_relative_path(proof["path"], f"{proof_label}.path")
        require(
            SHA256_RE.fullmatch(proof["sha256"]) is not None,
            f"{proof_label}.sha256 is malformed",
        )


def verify_protected_foundation(lock: dict[str, Any], science_repo: Path) -> None:
    protected = lock["protected_foundation"]
    commits = protected["commits"]
    require(len(commits) >= 12, "protected foundation commit set is incomplete")
    seen_commits: set[str] = set()
    for index, entry in enumerate(commits):
        sha = entry["sha"]
        require(
            GIT_SHA_RE.fullmatch(sha) is not None,
            f"protected_foundation.commits[{index}].sha is not a full Git SHA",
        )
        require(sha not in seen_commits, f"protected foundation repeats commit {sha}")
        seen_commits.add(sha)
        result = subprocess.run(
            ["git", "-C", str(science_repo), "merge-base", "--is-ancestor", sha, "HEAD"],
            check=False,
            capture_output=True,
            text=True,
        )
        require(
            result.returncode == 0,
            f"protected foundation commit is not an ancestor of HEAD: {sha}",
        )

    for index, marker in enumerate(protected["required_markers"]):
        label = f"protected_foundation.required_markers[{index}]"
        require_relative_path(marker["path"], f"{label}.path")
        path = science_repo / marker["path"]
        require(path.is_file(), f"protected foundation file is missing: {marker['path']}")
        text = path.read_text(encoding="utf-8")
        for expected in marker["contains"]:
            require(
                expected in text,
                f"protected foundation marker missing from {marker['path']}: {expected}",
            )


def verify_carried_ledgers(lock: dict[str, Any], science_repo: Path) -> None:
    carried = lock["protected_foundation"]["carried_ledgers"]

    open_spec = carried["open_science_adoption"]
    open_ledger = load_json(science_repo / open_spec["path"], "Open Science adoption ledger")
    require(
        open_ledger["upstream"]["commit"] == open_spec["commit"],
        "Open Science adopted baseline changed or disappeared",
    )
    require(
        open_ledger["summary"]["totalShipped"] >= open_spec["minimum_total_shipped"],
        "Open Science shipped-file count regressed",
    )
    require(
        open_ledger["summary"]["upstreamFilesNotAdopted"]
        == open_spec["upstream_files_not_adopted"],
        "Open Science carried adoption ledger now reports missing upstream files",
    )

    skill_spec = carried["science_skills"]
    skill_ledger = load_json(science_repo / skill_spec["path"], "Science skill registry")
    require(
        skill_ledger["summary"]["total"] >= skill_spec["minimum_total"],
        "Science skill inventory regressed",
    )
    require(
        skill_ledger["summary"]["approved"] >= skill_spec["minimum_approved"],
        "approved Science skill count regressed",
    )
    require(
        skill_ledger["summary"]["total"] == len(skill_ledger["skills"]),
        "Science skill summary does not match its entries",
    )
    for skill in skill_ledger["skills"]:
        permissions = skill["runtime_permissions"]
        require(
            permissions["independent_execution_authority"] is False,
            f"skill has independent execution authority: {skill['skill_id']}",
        )
        require(
            permissions["session_actor_required"] is True,
            f"skill bypasses SessionActor: {skill['skill_id']}",
        )
        if skill["final_disposition"] == "approved":
            require(
                skill["prompt_injection_audit"]["status"] == "pass",
                f"approved skill lacks prompt-injection pass: {skill['skill_id']}",
            )
            require(
                permissions["may_call_lumen_tools_only"] is True,
                f"approved skill may call non-Lumen tools: {skill['skill_id']}",
            )

    connector_spec = carried["connector_inventory"]
    connector_ledger = load_json(
        science_repo / connector_spec["path"], "Science connector lock"
    )
    require(
        len(connector_ledger["items"]) == connector_spec["required_connectors"],
        "Science connector inventory regressed",
    )

    motif_spec = carried["motif_vendor"]
    motif_ledger = load_json(science_repo / motif_spec["path"], "Motif vendor manifest")
    require(
        motif_ledger["commit"] == motif_spec["commit"],
        "Motif vendored commit changed unexpectedly",
    )
    require(
        motif_ledger["runtime_authority"] == motif_spec["runtime_authority"] == "none",
        "Motif vendor manifest grants runtime authority",
    )

    scp_spec = carried["scp_quarantine_catalog"]
    scp_manifest = load_json(
        science_repo / scp_spec["vendor_manifest_path"], "SCP vendor manifest"
    )
    require(
        scp_manifest["commit"] == scp_spec["commit"],
        "SCP vendored source commit changed unexpectedly",
    )
    require(
        scp_manifest["runtime_authority"] == "none",
        "SCP vendor manifest grants runtime authority",
    )
    require(
        scp_manifest["skill_documents"] == scp_spec["required_total"],
        "SCP vendored skill count regressed",
    )
    require(
        len(scp_manifest["redactions"]) >= scp_spec["minimum_redacted_source_files"],
        "SCP source credential redaction ledger regressed",
    )
    source_files = scp_manifest["source_files"]
    vendored_files = scp_manifest["vendored_files"]
    require(
        set(source_files) == set(vendored_files),
        "SCP source and vendored file manifests have different paths",
    )
    for relative, expected in vendored_files.items():
        require_relative_path(relative, f"SCP vendored file {relative}")
        require(
            SHA256_RE.fullmatch(source_files[relative]) is not None,
            f"SCP source SHA-256 is malformed: {relative}",
        )
        require(
            SHA256_RE.fullmatch(expected) is not None,
            f"SCP vendored SHA-256 is malformed: {relative}",
        )
        path = science_repo / "third_party/internscience-scp" / relative
        require(path.is_file(), f"SCP vendored file is missing: {relative}")
        require(
            sha256(path) == expected,
            f"SCP vendored file hash drifted: {relative}",
        )

    scp_catalog = load_json(
        science_repo / scp_spec["catalog_path"], "SCP quarantine catalog"
    )
    authority = scp_catalog["authority"]
    require(
        authority["runtime_authority"] == "Rust SessionActor",
        "SCP catalog changed product authority",
    )
    require(
        authority["source_runtime_authority"] == "none"
        and authority["catalog_is_executable"] is False
        and authority["direct_scp_hub_calls_admitted"] is False
        and authority["bulk_auto_approval"] is False,
        "SCP catalog grants source execution, remote calls, or bulk approval",
    )
    summary = scp_catalog["summary"]
    skills = scp_catalog["skills"]
    require(
        summary["total"] == scp_spec["required_total"] == len(skills),
        "SCP catalog total regressed or disagrees with entries",
    )
    require(
        summary["approved"] == scp_spec["approved"] == 0,
        "SCP catalog contains an unreviewed approved skill",
    )
    require(
        summary["quarantined"] == len(skills),
        "SCP catalog quarantine count disagrees with entries",
    )

    seen_ids: set[str] = set()
    seen_paths: set[str] = set()
    credential_pattern = re.compile(r"(?<![A-Za-z0-9])sk-[A-Za-z0-9_-]{20,}")
    for skill in skills:
        skill_id = skill["skill_id"]
        relative = skill["source_path"]
        require(skill_id not in seen_ids, f"SCP catalog repeats skill id: {skill_id}")
        require(relative not in seen_paths, f"SCP catalog repeats source path: {relative}")
        seen_ids.add(skill_id)
        seen_paths.add(relative)
        require(
            skill["exact_commit"] == scp_spec["commit"],
            f"SCP catalog skill has the wrong source commit: {skill_id}",
        )
        require(relative in source_files, f"SCP catalog source is not vendored: {relative}")
        require(
            skill["source_sha256"] == source_files[relative],
            f"SCP catalog source hash disagrees with manifest: {skill_id}",
        )
        require(
            skill["vendored_sha256"] == vendored_files[relative],
            f"SCP catalog vendored hash disagrees with manifest: {skill_id}",
        )
        require(
            skill["source_redactions"] == scp_manifest["redactions"].get(relative, []),
            f"SCP catalog redactions disagree with manifest: {skill_id}",
        )
        require(
            skill["final_disposition"] == "quarantined",
            f"SCP skill bypassed quarantine: {skill_id}",
        )
        require(
            skill["prompt_injection_audit"]["status"] == "pending",
            f"SCP skill has an unevidenced prompt-injection pass: {skill_id}",
        )
        permissions = skill["runtime_permissions"]
        require(
            permissions["session_actor_required"] is True
            and permissions["may_call_lumen_tools_only"] is True
            and permissions["controlled_tools"] == []
            and permissions["independent_execution_authority"] is False
            and permissions["network"] == "denied-until-per-skill-admission"
            and permissions["shell"] == "denied"
            and permissions["filesystem"] == "denied",
            f"SCP skill has runtime capability before admission: {skill_id}",
        )
        vendored_text = (
            science_repo / "third_party/internscience-scp" / relative
        ).read_text(encoding="utf-8")
        require(
            credential_pattern.search(vendored_text) is None,
            f"SCP vendored skill contains a credential-shaped value: {relative}",
        )

    expected_skill_paths = {
        relative
        for relative in source_files
        if relative.startswith("skills/") and relative.endswith("/SKILL.md")
    }
    require(
        seen_paths == expected_skill_paths,
        "SCP catalog and vendored skill corpus are not a one-to-one set",
    )


def verify_no_proprietary_copy(science_repo: Path) -> None:
    tracked = run_git(science_repo, "ls-files").splitlines()
    violations = [
        path
        for path in tracked
        if any(path.startswith(prefix) for prefix in FORBIDDEN_COPY_PREFIXES)
    ]
    require(
        not violations,
        "proprietary OpenClaudeScience skill material appears in tracked destinations: "
        + ", ".join(violations),
    )


def verify_local_source(source: dict[str, Any], root: Path) -> None:
    label = f"source-root[{source['id']}]"
    require((root / ".git").exists(), f"{label} is not a Git checkout: {root}")
    head = run_git(root, "rev-parse", "HEAD")
    require(
        head == source["exact_commit"],
        f"{label} is at {head}, expected {source['exact_commit']}",
    )
    require(
        run_git(root, "status", "--porcelain") == "",
        f"{label} is dirty; source proof requires an unmodified checkout",
    )

    expected_files: list[tuple[str, str, str]] = [
        ("license", source["license_file"], source["license_sha256"])
    ]
    for index, proof in enumerate(source.get("source_proofs", [])):
        expected_files.append((f"proof[{index}]", proof["path"], proof["sha256"]))
    for index, component in enumerate(source["components"]):
        if component["disposition"] == "reject-license":
            expected_files.append(
                (
                    f"proprietary-license[{index}]",
                    component["license_file"],
                    component["license_sha256"],
                )
            )

    for proof_label, relative, expected in expected_files:
        path = root / relative
        require(path.is_file(), f"{label} {proof_label} file is missing: {relative}")
        actual = sha256(path)
        require(
            actual == expected,
            f"{label} {proof_label} SHA-256 is {actual}, expected {expected}",
        )

    inventory = source.get("inventory", {})
    if source["id"] == "internscience-scp-skills":
        skill_count = len(list((root / "skills").glob("*/SKILL.md")))
        require(
            skill_count == inventory["skill_documents"],
            f"{label} has {skill_count} skill documents, expected "
            f"{inventory['skill_documents']}",
        )
    if source["id"] == "qzzqzzb-openclaudescience":
        catalog = (root / "ui/src/app/skills/science-skill-catalog.ts").read_text(
            encoding="utf-8"
        )
        catalog_paths = set(
            re.findall(r'sourcePath:\s*"skills/([^"]+)"', catalog)
        )
        require(
            len(catalog_paths) == 207,
            f"{label} science catalog has {len(catalog_paths)} unique entries, expected 207",
        )


def parse_source_roots(values: list[str]) -> dict[str, Path]:
    roots: dict[str, Path] = {}
    for value in values:
        source_id, separator, path = value.partition("=")
        require(separator == "=" and source_id and path, f"invalid --source-root: {value}")
        require(source_id not in roots, f"duplicate --source-root id: {source_id}")
        roots[source_id] = Path(path).expanduser().resolve()
    return roots


def verify_lock(
    lock: dict[str, Any], science_repo: Path, source_roots: dict[str, Path]
) -> None:
    require(lock["schema_version"] == 1, "unsupported schema_version")
    policy = lock["policy"]
    require(policy["runtime_authority"] == "Rust SessionActor", "authority drift")
    require(
        policy["external_runtime_authority"] == "denied",
        "external runtime authority is not denied",
    )
    require(
        policy["root_license_overrides_nested_license"] is False,
        "root license must not override nested licenses",
    )
    require(
        policy["unreviewed_data_is_scientific_truth"] is False,
        "unreviewed external data cannot be scientific truth",
    )
    require(
        policy["direct_provider_or_billable_calls_during_admission"] is False,
        "admission must not authorize provider or billable calls",
    )
    require(
        policy["proprietary_material_policy"].startswith("do-not-copy-or-derive"),
        "proprietary material policy must remain clean-room only",
    )

    sources = lock["sources"]
    require(
        {source["id"] for source in sources} == EXPECTED_SOURCES,
        "the four requested source repositories are not all locked exactly once",
    )
    transitive = lock["transitive_sources"]
    require(
        {source["id"] for source in transitive} == EXPECTED_TRANSITIVE_SOURCES,
        "the transitive SCP skill source is not locked exactly once",
    )

    all_sources = sources + transitive
    for index, source in enumerate(all_sources):
        verify_source_shape(source, f"all_sources[{index}]")

    ocs = next(source for source in sources if source["id"] == "qzzqzzb-openclaudescience")
    rejected = {
        component["path"]
        for component in ocs["components"]
        if component["disposition"] == "reject-license"
    }
    require(
        rejected == REQUIRED_PROPRIETARY_REJECTIONS,
        "the four proprietary OpenClaudeScience skill trees must remain explicitly rejected",
    )
    clean_room = [
        component
        for component in ocs["components"]
        if component["disposition"] == "clean-room-reimplement"
    ]
    require(
        len(clean_room) == 1 and "without" in clean_room[0]["reason"].lower(),
        "a clearly independent clean-room replacement commitment is required",
    )

    verify_protected_foundation(lock, science_repo)
    verify_carried_ledgers(lock, science_repo)
    verify_no_proprietary_copy(science_repo)

    known = {source["id"]: source for source in all_sources}
    unknown_roots = set(source_roots) - set(known)
    require(not unknown_roots, f"unknown --source-root ids: {sorted(unknown_roots)}")
    for source_id, root in source_roots.items():
        verify_local_source(known[source_id], root)


def main() -> int:
    script_path = Path(__file__).resolve()
    default_repo = script_path.parent.parent
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--lock",
        type=Path,
        default=default_repo
        / "docs"
        / "science"
        / "5.0"
        / "ecosystem-admission.lock.json",
    )
    parser.add_argument("--science-repo", type=Path, default=default_repo)
    parser.add_argument(
        "--source-root",
        action="append",
        default=[],
        metavar="ID=PATH",
        help="optionally verify an exact local upstream checkout; repeatable",
    )
    args = parser.parse_args()

    try:
        lock = load_json(args.lock.resolve(), "ecosystem admission lock")
        source_roots = parse_source_roots(args.source_root)
        verify_lock(lock, args.science_repo.resolve(), source_roots)
    except (KeyError, OSError, ValueError, json.JSONDecodeError) as error:
        print(f"FAIL: {error}", file=sys.stderr)
        return 1

    suffix = (
        f"; exact local sources={len(args.source_root)}"
        if args.source_root
        else "; local sources not requested"
    )
    print(
        "PASS: ecosystem admission verified "
        f"(foundation, ledgers, licenses, authority{suffix})"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
