---
name: traceability-review
description: Full traceability chain validation — from chart/data back to MCP query to raw domain tool output. Verifies the provenance links are complete and uncorrupted.
runAs: inline
allowedTools: [read_file, grep, glob, lsp_definition]
---

## Purpose
Validate the complete traceability chain for a research artifact:
chart → computed data → MCP result → domain tool call → query parameters.

## Workflow
1. Identify all figures/tables/charts in the artifact
2. Trace each data point back through intermediate files
3. Verify each MCP call in provenance.jsonl matches the claimed query
4. Check content hashes match (if provenance records content_hash)
5. Flag broken chains and missing intermediate steps

## Output
- `reports/traceability-report.md`: full chain for each claim
- Status per link: `OK`, `BROKEN`, `MISSING`, `UNVERIFIED`
