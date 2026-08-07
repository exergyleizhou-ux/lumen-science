# Provenance — InternScience/SCP skill corpus

| Field | Value |
|---|---|
| Repository | https://github.com/InternScience/scp.git |
| Commit | `cea5398564032aea65a78e246d06c30ae945e03f` |
| Root license | MIT |
| Skill documents | 207 |
| Local source | `third_party/internscience-scp/skills/*/SKILL.md` |
| Lumen catalog | `packs/science/skills/ecosystem/scp-catalog.json` |
| Runtime authority | none |
| Admission | sanitized source corpus; 207 quarantined, 0 approved |

## Modifications

The source documents are retained for local capability analysis, with these
prominent changes:

- credential-shaped values are replaced by
  `<REDACTED_UPSTREAM_SECRET>`;
- `VENDOR_MANIFEST.json` records the upstream SHA-256, local SHA-256, and
  redaction count for every affected file;
- a Lumen-owned catalog adds source identity, exact hashes, candidate route
  mappings, prompt-injection status, and fail-closed runtime permissions;
- an empty upstream skill description is represented in the catalog by its
  document title and explicitly marked as a metadata-quality issue.

The original MIT license remains in `third_party/internscience-scp/LICENSE`.

## Authority and network boundary

The vendored Markdown is not loaded as an approved runtime skill. Embedded
Python, MCP/SCP clients, endpoint URLs, API-key examples, and claims that a
workflow works against a live service are inert source material only.

Each skill must separately pass:

1. file and dependency license review;
2. prompt-injection and scientific-claim review;
3. mapping to admitted, typed Lumen tools;
4. offline fixture and parser tests;
5. SessionActor approval, terminal-state, artifact, evidence, provenance, and
   replay tests.

Until then, network, shell, filesystem, and controlled tool lists are empty or
denied. Bulk approval is forbidden.
