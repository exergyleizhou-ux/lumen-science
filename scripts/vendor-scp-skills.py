#!/usr/bin/env python3
"""Vendor the pinned InternScience/SCP skill corpus as quarantined source material.

This script intentionally does not create executable Lumen skills.  It copies
the permissively licensed source documents with exact hashes, then generates a
Lumen-owned catalog whose runtime permissions are fail-closed.  Direct SCP Hub
endpoints and example Python remain inert provenance until an individual skill
maps only to admitted Lumen tools.

Use ``--write`` to update generated files.  The default mode is read-only and
fails when the checked-in vendor tree or catalog is stale.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
from pathlib import Path
from typing import Any


SOURCE_ID = "internscience-scp-skills"
VENDOR_REL = Path("third_party/internscience-scp")
CATALOG_REL = Path("packs/science/skills/ecosystem/scp-catalog.json")
LOCK_REL = Path("docs/science/5.0/ecosystem-admission.lock.json")


def fail(message: str) -> None:
    raise ValueError(message)


def require(condition: bool, message: str) -> None:
    if not condition:
        fail(message)


def digest_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def run_git(repo: Path, *args: str) -> str:
    result = subprocess.run(
        ["git", "-C", str(repo), *args],
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        fail(
            f"git {' '.join(args)} failed in {repo}: "
            f"{result.stderr.strip() or result.stdout.strip()}"
        )
    return result.stdout.strip()


def decode_yaml_scalar(value: str) -> str:
    value = value.strip()
    if len(value) >= 2 and value[0] == value[-1] == '"':
        try:
            decoded = json.loads(value)
            return decoded if isinstance(decoded, str) else str(decoded)
        except json.JSONDecodeError:
            return value[1:-1]
    if len(value) >= 2 and value[0] == value[-1] == "'":
        return value[1:-1].replace("''", "'")
    return value


def parse_frontmatter(text: str, path: Path) -> dict[str, str]:
    require(text.startswith("---\n"), f"{path} has no YAML frontmatter")
    parts = text.split("---", 2)
    require(len(parts) == 3, f"{path} has unterminated YAML frontmatter")
    fields: dict[str, str] = {}
    for line in parts[1].splitlines():
        match = re.match(r"^([A-Za-z0-9_-]+):\s*(.*)$", line)
        if match:
            fields[match.group(1)] = decode_yaml_scalar(match.group(2))
    require(fields.get("name", "").strip() != "", f"{path} has no skill name")
    return fields


SECRET_PATTERNS: tuple[tuple[str, re.Pattern[str]], ...] = (
    (
        "scp-style-secret",
        re.compile(r"(?<![A-Za-z0-9])sk-[A-Za-z0-9_-]{20,}"),
    ),
    (
        "bearer-token",
        re.compile(r"(?i)\bBearer\s+[A-Za-z0-9._~+/-]{20,}"),
    ),
)


def sanitize_source(value: bytes) -> tuple[bytes, list[dict[str, Any]]]:
    text = value.decode("utf-8")
    redactions: list[dict[str, Any]] = []
    for kind, pattern in SECRET_PATTERNS:
        text, count = pattern.subn("<REDACTED_UPSTREAM_SECRET>", text)
        if count:
            redactions.append({"kind": kind, "count": count})
    return text.encode(), redactions


def fallback_description(text: str, source_id: str) -> str:
    body = text.split("---", 2)[2] if text.startswith("---") else text
    title = re.search(r"^#\s+(.+?)\s*$", body, re.MULTILINE)
    if title:
        return title.group(1).strip()
    return source_id.replace("-", " ").replace("_", " ").title()


def extract_tools(text: str) -> list[dict[str, str]]:
    tools: list[dict[str, str]] = []
    seen: set[tuple[str, str]] = set()
    in_tools = False
    for line in text.splitlines():
        if line.strip() == "## Tools Used":
            in_tools = True
            continue
        if in_tools and line.startswith("## "):
            break
        if not in_tools:
            continue
        match = re.search(
            r"\*\*`([^`]+)`\*\*.*?(https://[^\s)`]+)",
            line,
        )
        if not match:
            continue
        name = match.group(1).strip()
        endpoint = match.group(2).rstrip(".,")
        key = (name, endpoint)
        if key not in seen:
            tools.append({"name": name, "endpoint": endpoint})
            seen.add(key)
    return tools


def extract_endpoints(text: str) -> list[str]:
    endpoints = {
        value.rstrip(".,;)'\"`")
        for value in re.findall(r"https://[^\s<>\"]+", text)
        if not value.startswith("https://github.com/InternScience/scp")
    }
    return sorted(endpoints)


def extract_discipline(text: str) -> str:
    match = re.search(r"\*\*Discipline\*\*:\s*([^|\n]+)", text)
    return match.group(1).strip() if match else "unclassified"


ROUTE_RULES: tuple[tuple[tuple[str, ...], str], ...] = (
    (("pubmed",), "x.ai/science/connector_fetch:pubmed"),
    (("chembl",), "x.ai/science/connector_fetch:chembl"),
    (("uniprot",), "x.ai/science/connector_fetch:uniprot"),
    (("pubchem",), "x.ai/science/connector_fetch:pubchem"),
    (("alphafold",), "x.ai/science/connector_fetch:alphafold"),
    (("interpro",), "x.ai/science/connector_fetch:interpro"),
    (("rcsb", "pdbcode", "pdb "), "x.ai/science/connector_fetch:rcsb-pdb"),
    (("clinvar",), "x.ai/science/connector_fetch:clinvar"),
    (("dbsnp", "rsid"), "x.ai/science/connector_fetch:dbsnp"),
    (("gnomad",), "x.ai/science/connector_fetch:gnomad"),
    (("ensembl",), "x.ai/science/connector_fetch:ensembl"),
    (("opentarget", "open targets"), "x.ai/science/connector_fetch:opentargets"),
    (("reactome",), "x.ai/science/connector_fetch:reactome"),
    (("string-db", "stringdb", "string network"), "x.ai/science/connector_fetch:string-db"),
    (("geo ", "gene expression omnibus"), "x.ai/science/connector_fetch:geo"),
    (("ucsc",), "x.ai/science/connector_fetch:ucsc"),
    (("biorxiv",), "x.ai/science/connector_fetch:biorxiv"),
    (("arxiv",), "x.ai/science/connector_fetch:arxiv"),
    (("semantic scholar",), "x.ai/science/connector_fetch:semantic-scholar"),
    (("openalex",), "x.ai/science/connector_fetch:openalex"),
    (("europe pmc", "europepmc"), "x.ai/science/connector_fetch:europepmc"),
    (("gtopdb", "guide to pharmacology"), "x.ai/science/connector_fetch:gtopdb"),
    (("quickgo", "gene ontology"), "candidate-connector:quickgo"),
    (("clinicaltrials", "clinical trials"), "candidate-connector:clinicaltrials-gov"),
    (("encode",), "candidate-connector:encode"),
    (("gwas",), "candidate-connector:gwas-catalog"),
    (("monarch",), "candidate-connector:monarch"),
    (("openfda", "fda "), "candidate-connector:openfda"),
)


def candidate_routes(
    name: str, description: str, discipline: str, tools: list[dict[str, str]]
) -> list[str]:
    haystack = " ".join(
        [name, description, discipline]
        + [tool["name"] for tool in tools]
        + [tool["endpoint"] for tool in tools]
    ).lower()
    routes = {
        route
        for needles, route in ROUTE_RULES
        if any(needle in haystack for needle in needles)
    }
    return sorted(routes)


def atomic_write(path: Path, value: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(f".{path.name}.{os.getpid()}.tmp")
    temporary.write_bytes(value)
    os.replace(temporary, path)


def load_source_spec(repo: Path) -> dict[str, Any]:
    lock = json.loads((repo / LOCK_REL).read_text(encoding="utf-8"))
    source = next(
        (
            item
            for item in lock["transitive_sources"]
            if item["id"] == SOURCE_ID
        ),
        None,
    )
    require(source is not None, f"{SOURCE_ID} is missing from {LOCK_REL}")
    return source


def build_outputs(
    repo: Path, source_root: Path, source: dict[str, Any]
) -> tuple[dict[Path, bytes], dict[str, Any]]:
    require((source_root / ".git").exists(), f"not a Git checkout: {source_root}")
    head = run_git(source_root, "rev-parse", "HEAD")
    require(head == source["exact_commit"], f"source HEAD is {head}, expected {source['exact_commit']}")
    require(
        run_git(source_root, "status", "--porcelain") == "",
        "source checkout is dirty",
    )

    skill_paths = sorted((source_root / "skills").glob("*/SKILL.md"))
    expected_count = source["inventory"]["skill_documents"]
    require(
        len(skill_paths) == expected_count,
        f"source has {len(skill_paths)} skill documents, expected {expected_count}",
    )

    vendored_files: dict[str, bytes] = {
        "LICENSE": (source_root / "LICENSE").read_bytes(),
        "README.md": (source_root / "README.md").read_bytes(),
    }
    source_hashes: dict[str, str] = {
        relative: digest_bytes(value) for relative, value in vendored_files.items()
    }
    redactions_by_path: dict[str, list[dict[str, Any]]] = {}
    catalog_skills: list[dict[str, Any]] = []
    seen_ids: set[str] = set()
    total_tool_refs = 0
    skills_with_direct_endpoints = 0
    skills_with_execution_examples = 0

    for skill_path in skill_paths:
        relative = skill_path.relative_to(source_root).as_posix()
        source_value = skill_path.read_bytes()
        value, redactions = sanitize_source(source_value)
        text = value.decode("utf-8")
        fields = parse_frontmatter(text, skill_path)
        source_id = skill_path.parent.name
        skill_id = f"ecosystem/scp/{source_id}"
        require(skill_id not in seen_ids, f"duplicate skill id: {skill_id}")
        seen_ids.add(skill_id)

        metadata_quality_issues: list[str] = []
        description = fields.get("description", "").strip()
        if not description:
            description = fallback_description(text, source_id)
            metadata_quality_issues.append(
                "upstream frontmatter description was empty; catalog fallback uses the document title"
            )
        tools = extract_tools(text)
        endpoints = extract_endpoints(text)
        discipline = extract_discipline(text)
        contains_execution_example = any(
            marker in text
            for marker in (
                "ClientSession",
                "streamablehttp_client",
                "sse_client",
                "subprocess.",
                "exec(",
                "<YOUR_SCP_HUB_API_KEY>",
            )
        )
        total_tool_refs += len(tools)
        skills_with_direct_endpoints += int(bool(endpoints))
        skills_with_execution_examples += int(contains_execution_example)

        catalog_skills.append(
            {
                "skill_id": skill_id,
                "display_name": fields["name"],
                "description": description,
                "discipline": discipline,
                "source_repository": source["repository"],
                "exact_commit": source["exact_commit"],
                "source_path": relative,
                "source_sha256": digest_bytes(source_value),
                "vendored_sha256": digest_bytes(value),
                "source_redactions": redactions,
                "metadata_quality_issues": metadata_quality_issues,
                "file_license": fields.get("license") or source["root_license"],
                "required_upstream_tools": tools,
                "source_endpoints_not_admitted": endpoints,
                "source_contains_executable_example": contains_execution_example,
                "candidate_lumen_routes": candidate_routes(
                    fields["name"], description, discipline, tools
                ),
                "prompt_injection_audit": {
                    "status": "pending",
                    "reason": "Source preserved locally; no runtime admission review has been completed.",
                },
                "runtime_permissions": {
                    "session_actor_required": True,
                    "may_call_lumen_tools_only": True,
                    "controlled_tools": [],
                    "independent_execution_authority": False,
                    "network": "denied-until-per-skill-admission",
                    "shell": "denied",
                    "filesystem": "denied",
                },
                "final_disposition": "quarantined",
                "admission_reason": "Required tools and scientific claims must map to admitted Lumen connectors/adapters with offline fixtures and evidence.",
            }
        )
        vendored_files[relative] = value
        source_hashes[relative] = digest_bytes(source_value)
        if redactions:
            redactions_by_path[relative] = redactions

    catalog = {
        "schema_version": 1,
        "generated_at": "2026-07-27",
        "source": {
            "id": SOURCE_ID,
            "repository": source["repository"],
            "exact_commit": source["exact_commit"],
            "root_license": source["root_license"],
            "vendor_root": VENDOR_REL.as_posix(),
        },
        "authority": {
            "runtime_authority": "Rust SessionActor",
            "source_runtime_authority": "none",
            "catalog_is_executable": False,
            "direct_scp_hub_calls_admitted": False,
            "bulk_auto_approval": False,
        },
        "summary": {
            "total": len(catalog_skills),
            "approved": 0,
            "quarantined": len(catalog_skills),
            "required_upstream_tool_references": total_tool_refs,
            "skills_with_direct_source_endpoints": skills_with_direct_endpoints,
            "skills_with_executable_examples": skills_with_execution_examples,
        },
        "skills": sorted(catalog_skills, key=lambda item: item["skill_id"]),
    }

    vendored_hashes = {
        relative: digest_bytes(value)
        for relative, value in sorted(vendored_files.items())
    }
    notice = (
        "# InternScience/SCP skill source notice\n\n"
        f"- Source: {source['repository']}\n"
        f"- Commit: `{source['exact_commit']}`\n"
        f"- Root license: {source['root_license']} (see `LICENSE`)\n"
        f"- Vendored skill documents: {len(skill_paths)}\n\n"
        "These documents are retained as non-executable, quarantined source material.\n"
        "Credential-shaped strings are replaced with `<REDACTED_UPSTREAM_SECRET>`;\n"
        "the manifest records both source and vendored SHA-256 values and every redaction.\n"
        "Their embedded SCP Hub endpoints, API-key examples, Python clients, tool calls,\n"
        "and scientific claims are not admitted Lumen runtime capabilities. Each skill\n"
        "requires an individual prompt-injection, license/data, scientific, and\n"
        "controlled-tool review before approval. Rust SessionActor remains the only\n"
        "execution, permission, artifact, evidence, provenance, and replay authority.\n"
    ).encode()
    manifest = {
        "schema_version": 1,
        "upstream": source["repository"],
        "commit": source["exact_commit"],
        "license": source["root_license"],
        "runtime_authority": "none",
        "content_role": "quarantined source corpus; not executable skills",
        "catalog": CATALOG_REL.as_posix(),
        "skill_documents": len(skill_paths),
        "source_files": source_hashes,
        "vendored_files": vendored_hashes,
        "redactions": redactions_by_path,
    }
    sums = "".join(
        f"{digest}  {relative}\n" for relative, digest in vendored_hashes.items()
    ).encode()

    outputs: dict[Path, bytes] = {
        repo / VENDOR_REL / relative: value
        for relative, value in vendored_files.items()
    }
    outputs[repo / VENDOR_REL / "NOTICE.md"] = notice
    outputs[repo / VENDOR_REL / "VENDOR_MANIFEST.json"] = (
        json.dumps(manifest, ensure_ascii=False, indent=2) + "\n"
    ).encode()
    outputs[repo / VENDOR_REL / "SHA256SUMS"] = sums
    outputs[repo / CATALOG_REL] = (
        json.dumps(catalog, ensure_ascii=False, indent=2) + "\n"
    ).encode()
    return outputs, catalog


def main() -> int:
    script = Path(__file__).resolve()
    repo = script.parent.parent
    parser = argparse.ArgumentParser()
    parser.add_argument("--source-root", type=Path, required=True)
    parser.add_argument(
        "--write",
        action="store_true",
        help="write the vendored corpus and generated catalog; default is check-only",
    )
    args = parser.parse_args()

    try:
        source = load_source_spec(repo)
        outputs, catalog = build_outputs(repo, args.source_root.resolve(), source)
        mismatches = [
            path.relative_to(repo).as_posix()
            for path, expected in outputs.items()
            if not path.is_file() or path.read_bytes() != expected
        ]
        if args.write:
            for path, value in outputs.items():
                atomic_write(path, value)
            print(
                "WROTE: SCP quarantine corpus "
                f"({catalog['summary']['total']} skills, {len(outputs)} generated files)"
            )
            return 0
        require(not mismatches, "generated SCP files are stale: " + ", ".join(mismatches[:20]))
    except (KeyError, OSError, ValueError, json.JSONDecodeError) as error:
        print(f"FAIL: {error}", file=sys.stderr)
        return 1

    print(
        "PASS: SCP quarantine corpus is current "
        f"({catalog['summary']['total']} skills, 0 approved)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
