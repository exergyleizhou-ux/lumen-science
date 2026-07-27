#!/usr/bin/env python3
"""Build a license-aware, non-executable catalog of Biomni resources.

The catalog covers every entry in Biomni's data-lake and software inventories,
all tracked local protocol files, and the two explicitly CC-BY-4.0 know-how
documents. Protocol bodies and underlying datasets are deliberately not copied:
their publisher/data licenses are separate from Biomni's Apache-2.0 code.

No upstream Python is imported or executed. ``env_desc*.py`` must contain only
literal assignments and is parsed with ``ast.literal_eval``.
"""

from __future__ import annotations

import argparse
import ast
import hashlib
import json
import os
import re
import subprocess
import sys
from pathlib import Path
from typing import Any


SOURCE_ID = "snap-stanford-biomni"
LOCK_REL = Path("docs/science/5.0/ecosystem-admission.lock.json")
VENDOR_REL = Path("third_party/biomni-resource-catalog")
CATALOG_REL = Path("packs/science/skills/ecosystem/biomni-resource-catalog.json")


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def sha256(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def git(repo: Path, *args: str) -> str:
    result = subprocess.run(
        ["git", "-C", str(repo), *args],
        check=False,
        capture_output=True,
        text=True,
    )
    require(
        result.returncode == 0,
        f"git {' '.join(args)} failed: {result.stderr.strip() or result.stdout.strip()}",
    )
    return result.stdout.strip()


def atomic_write(path: Path, value: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(f".{path.name}.{os.getpid()}.tmp")
    temporary.write_bytes(value)
    os.replace(temporary, path)


def parse_literal_assignments(path: Path) -> dict[str, Any]:
    tree = ast.parse(path.read_text(encoding="utf-8"), filename=str(path))
    values: dict[str, Any] = {}
    for node in tree.body:
        require(
            isinstance(node, ast.Assign)
            and len(node.targets) == 1
            and isinstance(node.targets[0], ast.Name),
            f"{path} contains non-literal top-level code",
        )
        try:
            values[node.targets[0].id] = ast.literal_eval(node.value)
        except (TypeError, ValueError) as error:
            raise ValueError(f"{path} contains a non-literal value: {error}") from error
    return values


def slug(value: str) -> str:
    normalized = re.sub(r"[^a-z0-9]+", "-", value.lower()).strip("-")
    return normalized[:96] or "unnamed"


DATA_LICENSE_RULES: tuple[tuple[tuple[str, ...], dict[str, str]], ...] = (
    (
        ("bindingdb",),
        {
            "source_category": "BindingDB",
            "license": "custom non-commercial grant",
            "commercial_status": "commercial-license-required",
        },
    ),
    (
        ("broad_repurposing",),
        {
            "source_category": "Broad Repurposing Hub",
            "license": "CC BY 4.0",
            "commercial_status": "allowed-with-attribution",
        },
    ),
    (
        ("ddinter_",),
        {
            "source_category": "DDInter",
            "license": "CC BY-NC-SA 4.0",
            "commercial_status": "non-commercial-only",
        },
    ),
    (
        ("disgenet",),
        {
            "source_category": "DisGeNET",
            "license": "CC BY-NC-SA 4.0",
            "commercial_status": "non-commercial-only",
        },
    ),
    (
        ("enamine_",),
        {
            "source_category": "Enamine",
            "license": "proprietary",
            "commercial_status": "commercial-license-required",
        },
    ),
    (
        ("evebio_",),
        {
            "source_category": "EveBio",
            "license": "proprietary or unspecified",
            "commercial_status": "permission-required",
        },
    ),
    (
        ("go-plus",),
        {
            "source_category": "Gene Ontology",
            "license": "CC BY 4.0",
            "commercial_status": "allowed-with-attribution",
        },
    ),
    (
        ("gtex_",),
        {
            "source_category": "GTEx",
            "license": "dbGaP controlled access",
            "commercial_status": "authorization-required",
        },
    ),
    (
        ("proteinatlas",),
        {
            "source_category": "Human Protein Atlas",
            "license": "CC BY-SA 3.0",
            "commercial_status": "share-alike-review-required",
        },
    ),
    (
        ("msigdb_",),
        {
            "source_category": "MSigDB",
            "license": "custom",
            "commercial_status": "commercial-license-required",
        },
    ),
    (
        ("omim",),
        {
            "source_category": "OMIM",
            "license": "custom",
            "commercial_status": "commercial-license-required",
        },
    ),
    (
        (
            "affinity_capture-",
            "co-fractionation",
            "dosage_growth_defect",
            "genetic_interaction",
            "proximity_label-",
            "reconstituted_complex",
            "synthetic_growth_defect",
            "synthetic_lethality",
            "synthetic_rescue",
            "two-hybrid",
        ),
        {
            "source_category": "BioGRID",
            "license": "OSL 3.0",
            "commercial_status": "license-review-required",
        },
    ),
    (
        ("czi_census_",),
        {
            "source_category": "CZI Cell Census",
            "license": "CC BY 4.0",
            "commercial_status": "allowed-with-attribution",
        },
    ),
    (
        ("depmap_",),
        {
            "source_category": "DepMap",
            "license": "CC BY 4.0",
            "commercial_status": "allowed-with-attribution",
        },
    ),
    (
        ("genebass_",),
        {
            "source_category": "GeneBass",
            "license": "ODC-By 1.0",
            "commercial_status": "allowed-with-attribution",
        },
    ),
    (
        ("gwas_catalog",),
        {
            "source_category": "GWAS Catalog",
            "license": "Apache-2.0 (as reported by Biomni)",
            "commercial_status": "license-review-required",
        },
    ),
    (
        ("hp.obo",),
        {
            "source_category": "Human Phenotype Ontology",
            "license": "custom free-use terms",
            "commercial_status": "license-review-required",
        },
    ),
    (
        ("mcpas-",),
        {
            "source_category": "McPAS-TCR",
            "license": "CC BY-NC-SA 4.0",
            "commercial_status": "non-commercial-only",
        },
    ),
    (
        ("mirdb_",),
        {
            "source_category": "miRDB",
            "license": "custom non-commercial",
            "commercial_status": "non-commercial-only",
        },
    ),
    (
        ("mirtarbase_",),
        {
            "source_category": "miRTarBase",
            "license": "CC BY-NC 4.0",
            "commercial_status": "non-commercial-only",
        },
    ),
    (
        ("mousemine_",),
        {
            "source_category": "MouseMine",
            "license": "CC BY 4.0",
            "commercial_status": "allowed-with-attribution",
        },
    ),
    (
        ("virus-host_ppi_p-hipster",),
        {
            "source_category": "P-HIPSTER",
            "license": "CC BY 4.0",
            "commercial_status": "allowed-with-attribution",
        },
    ),
    (
        ("txgnn_",),
        {
            "source_category": "TXGNN",
            "license": "MIT",
            "commercial_status": "allowed-with-notice",
        },
    ),
)


def data_license(name: str) -> dict[str, str]:
    lowered = name.lower()
    for prefixes, result in DATA_LICENSE_RULES:
        if any(lowered.startswith(prefix) for prefix in prefixes):
            return result
    return {
        "source_category": "unclassified",
        "license": "unknown",
        "commercial_status": "license-review-required",
    }


def source_kind_for_software(description: str) -> str:
    lowered = description.lower()
    if "[python package]" in lowered:
        return "python-package"
    if "[r package]" in lowered:
        return "r-package"
    if "[cli tool]" in lowered:
        return "cli-tool"
    return "software-identity-unclassified"


def common_candidate(
    *,
    source: dict[str, Any],
    skill_id: str,
    name: str,
    description: str,
    discipline: str,
    source_kind: str,
    source_path: str,
    source_hash: str,
    risk_flags: list[str],
    admission_track: str,
    extra: dict[str, Any],
) -> dict[str, Any]:
    return {
        "skill_id": skill_id,
        "display_name": name,
        "description": description,
        "discipline": discipline,
        "source_kind": source_kind,
        "source_repository": source["repository"],
        "exact_commit": source["exact_commit"],
        "source_path": source_path,
        "source_sha256": source_hash,
        "file_license": source["root_license"],
        "candidate_lumen_routes": [],
        "required_upstream_tools": [],
        "parameter_contract": {"required": [], "optional": []},
        "risk_flags": sorted(risk_flags),
        "admission_track": admission_track,
        "prompt_injection_audit": {
            "status": "pending",
            "reason": "External resource metadata is searchable; content and claims are not admitted.",
        },
        "runtime_permissions": {
            "session_actor_required": True,
            "may_call_lumen_tools_only": True,
            "controlled_tools": [],
            "independent_execution_authority": False,
            "network": "denied-until-per-resource-admission",
            "shell": "denied",
            "filesystem": "denied",
            "device": "denied",
        },
        "final_disposition": "quarantined",
        "admission_reason": (
            "A versioned source, applicable license, offline fixture or exact bytes, "
            "and Lumen evidence/provenance review are required before use."
        ),
        **extra,
    }


def knowledge_metadata(text: str, path: Path) -> tuple[str, str]:
    title_match = re.search(r"^#\s+(.+)$", text, re.MULTILINE)
    require(title_match is not None, f"know-how document has no H1: {path}")
    license_match = re.search(r"^\*\*License\*\*:\s*(.+)$", text, re.MULTILINE)
    require(
        license_match is not None and license_match.group(1).strip() == "CC BY 4.0",
        f"know-how document lacks explicit CC BY 4.0 metadata: {path}",
    )
    short_match = re.search(
        r"^\*\*Short Description\*\*:\s*(.+)$", text, re.MULTILINE
    )
    description = (
        short_match.group(1).strip()
        if short_match
        else f"Biomni know-how document: {title_match.group(1).strip()}."
    )
    return title_match.group(1).strip(), description


def build_outputs(
    repo: Path, source_root: Path, source: dict[str, Any]
) -> tuple[dict[Path, bytes], dict[str, Any]]:
    require((source_root / ".git").exists(), f"not a Git checkout: {source_root}")
    require(
        git(source_root, "rev-parse", "HEAD") == source["exact_commit"],
        "Biomni source HEAD does not match the lock",
    )
    require(git(source_root, "status", "--porcelain") == "", "Biomni source is dirty")

    env_path = source_root / "biomni/env_desc.py"
    commercial_env_path = source_root / "biomni/env_desc_cm.py"
    env = parse_literal_assignments(env_path)
    commercial_env = parse_literal_assignments(commercial_env_path)
    data_lake = env["data_lake_dict"]
    software = env["library_content_dict"]
    commercial_data = commercial_env["data_lake_dict"]
    commercial_software = commercial_env["library_content_dict"]
    require(
        len(data_lake) == source["inventory"]["data_lake_entries"],
        "Biomni data-lake count changed",
    )
    require(
        len(software) == source["inventory"]["software_catalog_entries"],
        "Biomni software count changed",
    )

    protocol_paths = sorted((source_root / "biomni/tool/protocols").glob("*/*.txt"))
    require(
        len(protocol_paths) == source["inventory"]["local_protocols"],
        "Biomni local protocol count changed",
    )
    know_how_paths = sorted((source_root / "biomni/know_how").glob("*.md"))
    require(len(know_how_paths) == 2, "expected two Biomni know-how documents")

    vendored: dict[str, bytes] = {
        "LICENSE": (source_root / "LICENSE").read_bytes(),
        "license_info.md": (source_root / "license_info.md").read_bytes(),
        "catalog-source/env_desc.py": env_path.read_bytes(),
        "catalog-source/env_desc_cm.py": commercial_env_path.read_bytes(),
    }
    candidates: list[dict[str, Any]] = []
    env_hash = sha256(env_path.read_bytes())

    for name, description in sorted(data_lake.items()):
        license_record = data_license(name)
        commercial_included = name in commercial_data
        risk_flags = ["external-data", "scientific-claim-source"]
        if license_record["commercial_status"] not in {
            "allowed-with-attribution",
            "allowed-with-notice",
        }:
            risk_flags.append("license-or-access-restriction")
        candidates.append(
            common_candidate(
                source=source,
                skill_id=f"ecosystem/biomni-resource/data/{slug(name)}",
                name=name,
                description=description,
                discipline="Data Resource",
                source_kind="data-resource",
                source_path="biomni/env_desc.py",
                source_hash=env_hash,
                risk_flags=risk_flags,
                admission_track="dataset-license-and-version-admission",
                extra={
                    "content_vendored": False,
                    "scientific_truth": False,
                    "commercial_mode_included": commercial_included,
                    "license_review": license_record,
                },
            )
        )

    for name, description in sorted(software.items()):
        software_kind = source_kind_for_software(description)
        candidates.append(
            common_candidate(
                source=source,
                skill_id=f"ecosystem/biomni-resource/software/{slug(name)}",
                name=name,
                description=description,
                discipline="Scientific Software",
                source_kind="software-resource",
                source_path="biomni/env_desc.py",
                source_hash=env_hash,
                risk_flags=["executable-dependency", "unpinned-version", "license-unverified"],
                admission_track="dependency-identity-license-and-sandbox-review",
                extra={
                    "software_kind": software_kind,
                    "version": None,
                    "upstream_repository": None,
                    "content_vendored": False,
                    "commercial_mode_included": name in commercial_software,
                    "license_review": {
                        "license": "unknown",
                        "commercial_status": "license-review-required",
                    },
                },
            )
        )

    protocol_inventory: dict[str, str] = {}
    for protocol_path in protocol_paths:
        source_path = protocol_path.relative_to(source_root).as_posix()
        digest = sha256(protocol_path.read_bytes())
        protocol_inventory[source_path] = digest
        publisher = protocol_path.parent.name
        title = protocol_path.stem
        candidates.append(
            common_candidate(
                source=source,
                skill_id=(
                    f"ecosystem/biomni-resource/protocol/{publisher}/"
                    f"{slug(title)}-{digest[:8]}"
                ),
                name=title,
                description=(
                    f"Protocol reference from {publisher}: {title}. Full text is not "
                    "vendored pending underlying publisher-license review."
                ),
                discipline="Laboratory Protocol",
                source_kind="protocol-reference",
                source_path=source_path,
                source_hash=digest,
                risk_flags=[
                    "physical-or-wet-lab-action",
                    "publisher-license-unverified",
                    "content-not-vendored",
                ],
                admission_track="citation-license-and-protocol-safety-review",
                extra={
                    "publisher": publisher,
                    "content_vendored": False,
                    "scientific_truth": False,
                    "license_review": {
                        "license": "not established by Biomni metadata",
                        "commercial_status": "permission-review-required",
                    },
                },
            )
        )

    knowledge_inventory: dict[str, str] = {}
    for know_how_path in know_how_paths:
        source_path = know_how_path.relative_to(source_root).as_posix()
        source_bytes = know_how_path.read_bytes()
        text = source_bytes.decode("utf-8")
        title, description = knowledge_metadata(text, know_how_path)
        relative = f"knowledge/{know_how_path.name}"
        vendored[relative] = source_bytes
        knowledge_inventory[source_path] = sha256(source_bytes)
        candidates.append(
            common_candidate(
                source=source,
                skill_id=f"ecosystem/biomni-resource/knowledge/{slug(know_how_path.stem)}",
                name=title,
                description=description,
                discipline="Scientific Know-How",
                source_kind="knowledge-document",
                source_path=source_path,
                source_hash=sha256(source_bytes),
                risk_flags=[
                    "instructional-content",
                    "contains-executable-examples",
                    "scientific-review-required",
                ],
                admission_track="cited-knowledge-review-and-controlled-tool-mapping",
                extra={
                    "vendored_path": relative,
                    "content_vendored": True,
                    "scientific_truth": False,
                    "license_review": {
                        "license": "CC BY 4.0",
                        "commercial_status": "allowed-with-attribution",
                    },
                },
            )
        )

    expected_total = (
        source["inventory"]["data_lake_entries"]
        + source["inventory"]["software_catalog_entries"]
        + source["inventory"]["local_protocols"]
        + len(know_how_paths)
    )
    require(len(candidates) == expected_total, "Biomni resource total changed")
    ids = [candidate["skill_id"] for candidate in candidates]
    require(len(ids) == len(set(ids)), "Biomni resource ids are not unique")

    notice = f"""# Biomni resource-catalog vendor notice

Source: {source["repository"]}
Exact commit: `{source["exact_commit"]}`

Apache-2.0 catalog literals and two explicitly CC-BY-4.0 know-how documents
are preserved. The 76 underlying datasets, 113 software packages, 82 protocol
bodies, CRISPick links, and Addgene sequence table are not mirrored or
executable. Their separate licenses, versions, access rights, and scientific
claims require individual admission.
""".encode()
    vendored["NOTICE.md"] = notice

    credential_pattern = re.compile(
        rb"(?<![A-Za-z0-9])sk-[A-Za-z0-9_-]{20,}"
        rb"|Bearer\s+[A-Za-z0-9._~+/-]{20,}",
        re.IGNORECASE,
    )
    for relative, value in vendored.items():
        require(
            credential_pattern.search(value) is None,
            f"credential-shaped value in Biomni resource vendor input: {relative}",
        )

    vendored_hashes = {path: sha256(value) for path, value in sorted(vendored.items())}
    manifest = {
        "schema_version": 1,
        "source": source["repository"],
        "commit": source["exact_commit"],
        "runtime_authority": "none",
        "data_records": len(data_lake),
        "software_records": len(software),
        "protocol_references": len(protocol_paths),
        "knowledge_documents": len(know_how_paths),
        "protocol_source_inventory": protocol_inventory,
        "knowledge_source_inventory": knowledge_inventory,
        "vendored_files": vendored_hashes,
        "generated_catalog": CATALOG_REL.as_posix(),
    }
    vendored["VENDOR_MANIFEST.json"] = (
        json.dumps(manifest, ensure_ascii=False, indent=2) + "\n"
    ).encode()
    vendored["SHA256SUMS"] = "".join(
        f"{sha256(value)}  {relative}\n"
        for relative, value in sorted(vendored.items())
        if relative != "SHA256SUMS"
    ).encode()

    counts = {
        kind: sum(candidate["source_kind"] == kind for candidate in candidates)
        for kind in (
            "data-resource",
            "software-resource",
            "protocol-reference",
            "knowledge-document",
        )
    }
    catalog = {
        "schema_version": 1,
        "generated_at": "2026-07-27",
        "source": {
            "id": SOURCE_ID,
            "catalog_kind": "resource-inventory",
            "repository": source["repository"],
            "exact_commit": source["exact_commit"],
            "root_license": source["root_license"],
            "vendor_root": VENDOR_REL.as_posix(),
        },
        "authority": {
            "runtime_authority": "Rust SessionActor",
            "source_runtime_authority": "none",
            "catalog_is_executable": False,
            "direct_upstream_calls_admitted": False,
            "bulk_auto_approval": False,
        },
        "summary": {
            "total": len(candidates),
            "approved": 0,
            "quarantined": len(candidates),
            **counts,
            "data_commercial_mode_included": len(commercial_data),
            "software_commercial_mode_included": len(commercial_software),
            "noncommercial_or_restricted_data": sum(
                candidate["source_kind"] == "data-resource"
                and "license-or-access-restriction" in candidate["risk_flags"]
                for candidate in candidates
            ),
        },
        "skills": candidates,
    }

    outputs = {
        repo / VENDOR_REL / relative: value for relative, value in vendored.items()
    }
    outputs[repo / CATALOG_REL] = (
        json.dumps(catalog, ensure_ascii=False, indent=2) + "\n"
    ).encode()
    return outputs, catalog


def existing_files(repo: Path) -> set[Path]:
    files: set[Path] = set()
    vendor = repo / VENDOR_REL
    if vendor.exists():
        files.update(path for path in vendor.rglob("*") if path.is_file())
    catalog = repo / CATALOG_REL
    if catalog.exists():
        files.add(catalog)
    return files


def main() -> int:
    script = Path(__file__).resolve()
    repo_default = script.parent.parent
    parser = argparse.ArgumentParser()
    parser.add_argument("--science-repo", type=Path, default=repo_default)
    parser.add_argument("--source-root", type=Path, required=True)
    parser.add_argument("--write", action="store_true")
    args = parser.parse_args()
    repo = args.science_repo.resolve()
    source_root = args.source_root.resolve()
    lock = json.loads((repo / LOCK_REL).read_text(encoding="utf-8"))
    source = next(item for item in lock["sources"] if item["id"] == SOURCE_ID)

    try:
        outputs, catalog = build_outputs(repo, source_root, source)
        stale = [
            path
            for path, value in outputs.items()
            if not path.is_file() or path.read_bytes() != value
        ]
        unexpected = existing_files(repo) - set(outputs)
        if args.write:
            for path in unexpected:
                path.unlink()
            for path, value in outputs.items():
                atomic_write(path, value)
            print(
                f"WROTE: Biomni resource catalog "
                f"({catalog['summary']['total']} records, 0 approved)"
            )
            return 0
        require(not stale, f"stale generated files: {stale[:8]}")
        require(not unexpected, f"unexpected generated files: {sorted(unexpected)[:8]}")
        print(
            f"PASS: Biomni resource catalog is current "
            f"({catalog['summary']['total']} records, 0 approved)"
        )
        return 0
    except (KeyError, OSError, ValueError, json.JSONDecodeError) as error:
        print(f"FAIL: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
