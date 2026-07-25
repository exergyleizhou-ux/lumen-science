# Skill admission evidence — 2026-07-26

## Policy

A skill may be `final_disposition: approved` only when all hold:

1. `source_repository` + `exact_commit` + `source_sha256` present  
2. Prompt-injection audit status `pass` with written notes  
3. `runtime_permissions.controlled_tools` non-empty and Lumen-only  
4. `independent_execution_authority: false`  
5. No generic shell / filesystem bridge / remote script  

## Approved (10)

| skill_id | Controlled tools | Rationale |
|----------|------------------|-----------|
| `science/research-brief` | `lumen-science.brief`, pubmed, chembl | Product CLI + connectors only |
| `science/literature-survey` | brief + `connector_fetch` | SessionActor path only |
| `science/literature-review` | brief + connector_fetch + artifact.put | Claims require artifacts |
| `science/indication-dossier` | brief + connectors + integrity-auditor | No clinical certification claim |
| `science/integrity-auditor` | artifact verify, pipeline offline, reviewer MCP | Hash-enforced integrity |
| `science/traceability-review` | artifact list/verify, reviewer | Registered artifacts only |
| `science/motif-for-claude-science` | MotifRenderer + seq_analyze | No Motif MCP authority |
| `science/molecule-viewer` | connector_fetch + protein-3d/chem-2d | Admitted connectors only |
| `science/chart-design-system` | artifact.put + renderers | Derived figure artifacts only |
| `science/figure-publication` | artifact list/verify + image renderer | Registered figures only |

## Still pending (17)

Protein-structure / MD / single-cell **GPU** skills (alphafold2, esmfold2, openfold3,
boltz, chai1, diffdock, proteinmpnn, ligandmpnn, solublempnn, fair-esm2, evo2, borzoi,
scgpt, scvi-tools), plus `remote-compute-ssh`, `env-management`, `oasis-c2d-run`
remain `pending-compute-admission` or `pending-controlled-tool-surface` until
compute/runtime admission exists.

## Prompt-injection residual risk

Approved skills still run under model instruction risk. Mitigations:

- Artifacts rehashed on verify (code path, not model trust)  
- Reviewer cannot invent SHA-256 without registered artifact  
- Shell denied; network only via admitted adapters  

## Evidence paths

- Registry: `packs/science/skills/registry.json`  
- CLI: `lumen-science brief|seq|pipeline|artifact`  
- ACP: `x.ai/science/seq_analyze`, connector_fetch  
