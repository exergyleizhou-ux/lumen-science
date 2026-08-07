#!/usr/bin/env python3
"""Vendor Biomni's Apache-2.0 tool descriptors as inert Lumen candidates.

Biomni's descriptor modules are Python literals, but this importer never
imports or executes them. It accepts exactly one ``description = <literal>``
assignment per module, validates every record, preserves the exact source
bytes, and generates a zero-approved Lumen catalog.

Use ``--write`` to update generated files. The default mode is read-only and
fails when the checked-in vendor tree or catalog is stale.
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
VENDOR_REL = Path("third_party/biomni-tool-descriptions")
CATALOG_REL = Path("packs/science/skills/ecosystem/biomni-tool-catalog.json")

EXISTING_CONNECTOR_ROUTES: dict[str, str] = {
    "query_uniprot": "x.ai/science/connector_fetch:uniprot",
    "query_alphafold": "x.ai/science/connector_fetch:alphafold",
    "query_interpro": "x.ai/science/connector_fetch:interpro",
    "query_pdb": "x.ai/science/connector_fetch:rcsb-pdb",
    "query_pdb_identifiers": "x.ai/science/connector_fetch:rcsb-pdb",
    "query_kegg": "x.ai/science/connector_fetch:kegg",
    "query_stringdb": "x.ai/science/connector_fetch:string-db",
    "query_clinvar": "x.ai/science/connector_fetch:clinvar",
    "query_geo": "x.ai/science/connector_fetch:geo",
    "query_dbsnp": "x.ai/science/connector_fetch:dbsnp",
    "query_ucsc": "x.ai/science/connector_fetch:ucsc",
    "query_ensembl": "x.ai/science/connector_fetch:ensembl",
    "query_opentarget": "x.ai/science/connector_fetch:opentargets",
    "query_gnomad": "x.ai/science/connector_fetch:gnomad",
    "query_reactome": "x.ai/science/connector_fetch:reactome",
    "query_gtopdb": "x.ai/science/connector_fetch:gtopdb",
    "query_pubchem": "x.ai/science/connector_fetch:pubchem",
    "query_chembl": "x.ai/science/connector_fetch:chembl",
    "query_arxiv": "x.ai/science/connector_fetch:arxiv",
    "query_pubmed": "x.ai/science/connector_fetch:pubmed",
}

CANDIDATE_CONNECTORS: dict[str, str] = {
    "query_iucn": "candidate-connector:iucn-red-list",
    "query_paleobiology": "candidate-connector:paleobiology-database",
    "query_jaspar": "candidate-connector:jaspar",
    "query_worms": "candidate-connector:worms",
    "query_cbioportal": "candidate-connector:cbioportal",
    "query_monarch": "candidate-connector:monarch",
    "query_openfda": "candidate-connector:openfda",
    "query_gwas_catalog": "candidate-connector:gwas-catalog",
    "query_regulomedb": "candidate-connector:regulomedb",
    "query_pride": "candidate-connector:pride",
    "query_remap": "candidate-connector:remap",
    "query_mpd": "candidate-connector:mouse-phenome-database",
    "query_emdb": "candidate-connector:emdb",
    "query_synapse": "candidate-connector:synapse",
    "query_unichem": "candidate-connector:unichem",
    "query_clinicaltrials": "candidate-connector:clinicaltrials-gov",
    "query_dailymed": "candidate-connector:dailymed",
    "query_quickgo": "candidate-connector:quickgo",
    "query_encode": "candidate-connector:encode",
    "region_to_ccre_screen": "candidate-connector:screen-ccre",
    "get_genes_near_ccre": "candidate-connector:screen-ccre",
    "query_fda_adverse_events": "candidate-connector:openfda",
    "get_fda_drug_label_info": "candidate-connector:dailymed",
    "check_fda_drug_recalls": "candidate-connector:openfda",
    "analyze_fda_safety_signals": "candidate-connector:openfda",
}


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
        f"git {' '.join(args)} failed in {repo}: "
        f"{result.stderr.strip() or result.stdout.strip()}",
    )
    return result.stdout.strip()


def atomic_write(path: Path, value: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(f".{path.name}.{os.getpid()}.tmp")
    temporary.write_bytes(value)
    os.replace(temporary, path)


def parse_descriptor(path: Path) -> list[dict[str, Any]]:
    tree = ast.parse(path.read_text(encoding="utf-8"), filename=str(path))
    require(
        len(tree.body) == 1 and isinstance(tree.body[0], ast.Assign),
        f"{path} must contain exactly one literal assignment",
    )
    assignment = tree.body[0]
    require(
        len(assignment.targets) == 1
        and isinstance(assignment.targets[0], ast.Name)
        and assignment.targets[0].id == "description",
        f"{path} must assign only to description",
    )
    try:
        value = ast.literal_eval(assignment.value)
    except (TypeError, ValueError) as error:
        raise ValueError(f"{path} description is not a safe literal: {error}") from error
    require(isinstance(value, list), f"{path} description is not a list")
    return value


def validate_parameter(
    parameter: Any, *, tool_name: str, group: str, index: int
) -> dict[str, Any]:
    require(
        isinstance(parameter, dict),
        f"{tool_name} {group}[{index}] is not an object",
    )
    require(
        isinstance(parameter.get("name"), str) and parameter["name"].strip(),
        f"{tool_name} {group}[{index}] has no name",
    )
    require(
        isinstance(parameter.get("type"), str) and parameter["type"].strip(),
        f"{tool_name} {group}[{index}] has no type",
    )
    require(
        isinstance(parameter.get("description"), str),
        f"{tool_name} {group}[{index}] has no description",
    )
    return {
        "name": parameter["name"],
        "type": parameter["type"],
        "description": parameter["description"].strip(),
        "default": parameter.get("default"),
    }


def risk_flags(
    module: str,
    name: str,
    description: str,
    parameters: list[dict[str, Any]],
) -> list[str]:
    flags: set[str] = set()
    names = " ".join(parameter["name"] for parameter in parameters).lower()
    haystack = f"{module} {name} {description} {names}".lower()
    if re.search(r"\b(url|endpoint)\b", names):
        flags.add("caller-supplied-network-target")
    if re.search(r"(path|file|directory|output|data_dir)", names):
        flags.add("filesystem-input-or-output")
    if re.search(r"(code|script|command|source_code|repl)", haystack):
        flags.add("code-or-command-execution")
    if module == "lab_automation" or re.search(
        r"(liquid handler|robot|cell sort|genome editing|electrophoresis)", haystack
    ):
        flags.add("physical-or-wet-lab-action")
    if re.search(r"(download|fetch|query|search|web)", haystack):
        flags.add("network-or-download")
    if re.search(r"(llm|claude|agent)", haystack):
        flags.add("model-dependent")
    if re.search(r"(patient|clinical|adverse|safety|diagnos)", haystack):
        flags.add("clinical-or-safety-sensitive")
    return sorted(flags)


def admission_track(module: str, name: str, flags: list[str]) -> str:
    if "physical-or-wet-lab-action" in flags:
        return "typed-device-or-protocol-safety-gate"
    if "code-or-command-execution" in flags:
        return "clean-room-controlled-compute"
    if name in EXISTING_CONNECTOR_ROUTES:
        return "map-to-existing-lumen-connector"
    if name in CANDIDATE_CONNECTORS:
        return "new-lumen-connector"
    if module in {"database", "literature"}:
        return "connector-review-required"
    if "filesystem-input-or-output" in flags:
        return "store-owned-artifact-adapter"
    return "deterministic-science-adapter"


def route_for(name: str) -> list[str]:
    route = EXISTING_CONNECTOR_ROUTES.get(name) or CANDIDATE_CONNECTORS.get(name)
    return [route] if route else []


def load_source(repo: Path) -> dict[str, Any]:
    lock = json.loads((repo / LOCK_REL).read_text(encoding="utf-8"))
    source = next(
        (item for item in lock["sources"] if item["id"] == SOURCE_ID),
        None,
    )
    require(source is not None, f"{SOURCE_ID} missing from {LOCK_REL}")
    return source


def build_outputs(
    repo: Path, source_root: Path, source: dict[str, Any]
) -> tuple[dict[Path, bytes], dict[str, Any]]:
    require((source_root / ".git").exists(), f"not a Git checkout: {source_root}")
    require(
        git(source_root, "rev-parse", "HEAD") == source["exact_commit"],
        "Biomni source HEAD does not match the ecosystem lock",
    )
    require(git(source_root, "status", "--porcelain") == "", "Biomni source is dirty")

    descriptor_root = source_root / "biomni/tool/tool_description"
    descriptor_paths = sorted(descriptor_root.glob("*.py"))
    require(
        len(descriptor_paths) == source["inventory"]["tool_modules"],
        f"Biomni has {len(descriptor_paths)} descriptor modules, "
        f"expected {source['inventory']['tool_modules']}",
    )

    vendored: dict[str, bytes] = {
        "LICENSE": (source_root / "LICENSE").read_bytes(),
        "license_info.md": (source_root / "license_info.md").read_bytes(),
    }
    source_files: dict[str, str] = {}
    candidates: list[dict[str, Any]] = []
    module_counts: dict[str, int] = {}
    seen_names: set[str] = set()
    risk_totals: dict[str, int] = {}

    for descriptor_path in descriptor_paths:
        module = descriptor_path.stem
        source_relative = descriptor_path.relative_to(source_root).as_posix()
        vendored_relative = f"tool-descriptions/{descriptor_path.name}"
        source_bytes = descriptor_path.read_bytes()
        require(
            re.search(
                rb"(?<![A-Za-z0-9])sk-[A-Za-z0-9_-]{20,}"
                rb"|Bearer\s+[A-Za-z0-9._~+/-]{20,}",
                source_bytes,
                re.IGNORECASE,
            )
            is None,
            f"credential-shaped value in Biomni descriptor: {source_relative}",
        )
        vendored[vendored_relative] = source_bytes
        source_files[vendored_relative] = sha256(source_bytes)

        records = parse_descriptor(descriptor_path)
        module_counts[module] = len(records)
        for index, record in enumerate(records):
            require(isinstance(record, dict), f"{source_relative}[{index}] is not an object")
            name = record.get("name")
            description = record.get("description")
            require(
                isinstance(name, str) and re.fullmatch(r"[A-Za-z0-9_-]+", name),
                f"{source_relative}[{index}] has an invalid name",
            )
            require(name not in seen_names, f"duplicate Biomni tool name: {name}")
            seen_names.add(name)
            require(
                isinstance(description, str) and description.strip(),
                f"{name} has no description",
            )
            required_raw = record.get("required_parameters")
            optional_raw = record.get("optional_parameters", [])
            require(isinstance(required_raw, list), f"{name} required_parameters is not a list")
            require(isinstance(optional_raw, list), f"{name} optional_parameters is not a list")
            required = [
                validate_parameter(value, tool_name=name, group="required", index=i)
                for i, value in enumerate(required_raw)
            ]
            optional = [
                validate_parameter(value, tool_name=name, group="optional", index=i)
                for i, value in enumerate(optional_raw)
            ]
            parameter_names = [item["name"] for item in required + optional]
            require(
                len(parameter_names) == len(set(parameter_names)),
                f"{name} repeats a parameter name",
            )
            flags = risk_flags(module, name, description, required + optional)
            for flag in flags:
                risk_totals[flag] = risk_totals.get(flag, 0) + 1

            candidates.append(
                {
                    "skill_id": f"ecosystem/biomni/{name}",
                    "display_name": name,
                    "description": description.strip(),
                    "discipline": module.replace("_", " ").title(),
                    "source_kind": "tool-descriptor",
                    "source_repository": source["repository"],
                    "exact_commit": source["exact_commit"],
                    "source_path": source_relative,
                    "vendored_path": vendored_relative,
                    "source_sha256": sha256(source_bytes),
                    "file_license": source["root_license"],
                    "parameter_contract": {
                        "required": required,
                        "optional": optional,
                    },
                    "risk_flags": flags,
                    "admission_track": admission_track(module, name, flags),
                    "candidate_lumen_routes": route_for(name),
                    "required_upstream_tools": [],
                    "prompt_injection_audit": {
                        "status": "pending",
                        "reason": "Descriptor is preserved locally; scientific and safety review is pending.",
                    },
                    "runtime_permissions": {
                        "session_actor_required": True,
                        "may_call_lumen_tools_only": True,
                        "controlled_tools": [],
                        "independent_execution_authority": False,
                        "network": "denied-until-per-tool-admission",
                        "shell": "denied",
                        "filesystem": "denied",
                        "device": "denied",
                    },
                    "final_disposition": "quarantined",
                    "admission_reason": (
                        "The upstream descriptor is discovery metadata only. A Lumen-owned "
                        "typed adapter, fixtures, evidence, provenance, and explicit admission "
                        "are required before execution."
                    ),
                }
            )

    require(
        len(candidates) == source["inventory"]["declared_tools"],
        f"Biomni has {len(candidates)} tools, expected {source['inventory']['declared_tools']}",
    )
    require(
        module_counts.get("database") == source["inventory"]["database_tools"],
        "Biomni database descriptor count changed",
    )

    notice = f"""# Biomni tool-description vendor notice

Source: {source["repository"]}
Exact commit: `{source["exact_commit"]}`
License: Apache-2.0

Only the 22 literal tool-description modules are preserved here. They are
non-runtime provenance and discovery input. Biomni agent loops, Python/R/Bash
execution, schema pickle files, downloaded datasets, and physical lab execution
are not vendored or authorized.

Every generated candidate is quarantined with zero controlled tools. Rust
SessionActor remains the sole execution, permission, artifact, evidence,
provenance, terminal-state, and replay authority.
""".encode()
    vendored["NOTICE.md"] = notice

    vendored_hashes = {path: sha256(value) for path, value in sorted(vendored.items())}
    manifest = {
        "schema_version": 1,
        "source": source["repository"],
        "commit": source["exact_commit"],
        "root_license": source["root_license"],
        "runtime_authority": "none",
        "descriptor_modules": len(descriptor_paths),
        "tool_records": len(candidates),
        "module_counts": module_counts,
        "source_files": source_files,
        "vendored_files": vendored_hashes,
        "generated_catalog": CATALOG_REL.as_posix(),
    }
    manifest_bytes = (json.dumps(manifest, ensure_ascii=False, indent=2) + "\n").encode()
    vendored["VENDOR_MANIFEST.json"] = manifest_bytes
    sums = "".join(
        f"{sha256(value)}  {relative}\n"
        for relative, value in sorted(vendored.items())
        if relative != "SHA256SUMS"
    ).encode()
    vendored["SHA256SUMS"] = sums

    catalog = {
        "schema_version": 1,
        "generated_at": "2026-07-27",
        "source": {
            "id": SOURCE_ID,
            "catalog_kind": "tool-descriptors",
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
            "modules": len(module_counts),
            "database_tools": module_counts["database"],
            "mapped_to_existing_lumen_connectors": sum(
                candidate["admission_track"] == "map-to-existing-lumen-connector"
                for candidate in candidates
            ),
            "new_connector_candidates": sum(
                candidate["admission_track"] == "new-lumen-connector"
                for candidate in candidates
            ),
            "risk_flag_totals": risk_totals,
        },
        "skills": candidates,
    }
    catalog_bytes = (json.dumps(catalog, ensure_ascii=False, indent=2) + "\n").encode()

    outputs = {
        repo / VENDOR_REL / relative: value for relative, value in vendored.items()
    }
    outputs[repo / CATALOG_REL] = catalog_bytes
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
    default_repo = script.parent.parent
    parser = argparse.ArgumentParser()
    parser.add_argument("--science-repo", type=Path, default=default_repo)
    parser.add_argument("--source-root", type=Path, required=True)
    parser.add_argument("--write", action="store_true")
    args = parser.parse_args()
    repo = args.science_repo.resolve()
    source_root = args.source_root.resolve()

    try:
        outputs, catalog = build_outputs(repo, source_root, load_source(repo))
        stale = sorted(
            path
            for path, value in outputs.items()
            if not path.is_file() or path.read_bytes() != value
        )
        unexpected = sorted(existing_files(repo) - set(outputs))
        if args.write:
            for path in unexpected:
                path.unlink()
            for path, value in outputs.items():
                atomic_write(path, value)
            print(
                f"WROTE: Biomni tool corpus ({catalog['summary']['total']} tools, "
                f"{catalog['summary']['approved']} approved)"
            )
            return 0
        require(not stale, "stale generated files: " + ", ".join(map(str, stale[:8])))
        require(
            not unexpected,
            "unexpected generated files: " + ", ".join(map(str, unexpected[:8])),
        )
        print(
            f"PASS: Biomni tool corpus is current "
            f"({catalog['summary']['total']} tools, {catalog['summary']['approved']} approved)"
        )
        return 0
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(f"FAIL: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
