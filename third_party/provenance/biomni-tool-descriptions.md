# Provenance — Biomni tool-description catalog

| Field | Value |
|---|---|
| Repository | https://github.com/snap-stanford/Biomni.git |
| Commit | `400c1f366b96a35ca253e13c9b06c5076af41d65` |
| Root license | Apache-2.0 |
| Descriptor modules | 22 |
| Tool records | 224 |
| Database tool records | 40 |
| Local source | `third_party/biomni-tool-descriptions/tool-descriptions/*.py` |
| Lumen catalog | `packs/science/skills/ecosystem/biomni-tool-catalog.json` |
| Runtime authority | none |
| Admission | 224 quarantined, 0 approved |

## What was reused

Biomni publishes its tool vocabulary as 22 modules whose only top-level
statement is a literal `description = [...]` assignment. Lumen preserves those
exact Apache-2.0 bytes and extracts:

- stable tool name, description, subject module, and typed parameter contract;
- exact source path, commit, and SHA-256;
- candidate mapping to an existing or proposed Lumen connector;
- filesystem, network, command, device, model, and clinical risk flags;
- the Lumen-native admission track for an independent implementation.

The importer parses the Python syntax tree with `ast.literal_eval`; it never
imports or executes an upstream module.

## What was not reused

The following Biomni components are not runtime dependencies:

- `A1`, ReAct, LangGraph, FastMCP, or any peer agent loop;
- direct Python, R, Bash, shell, or model-generated execution;
- caller-selected URL fetching and unbounded downloads;
- pickle schema databases or the downloadable data lake;
- physical lab/device operations;
- data or protocol material whose underlying license and scientific source has
  not been reviewed independently.

Biomni's own `license_info.md` records proprietary, controlled-access,
non-commercial, and share-alike data sources. The Apache-2.0 code license does
not convert those datasets into unrestricted Lumen assets.

## Admission boundary

All 224 records remain discovery metadata. The catalog contains zero controlled
tools, denies network/shell/filesystem/device access, and requires Rust
SessionActor. A tool becomes executable only after a Lumen-owned typed adapter,
offline fixtures, scientific validation, explicit approval, store-owned
artifacts, evidence, provenance, terminal-state, and replay tests exist.
