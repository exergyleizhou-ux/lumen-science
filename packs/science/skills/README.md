# Lumen Science Skills Registry

ACP (Agent Capability Protocol) extension descriptors for science-domain skills.

## Format

Each skill is described by a `skill.json` descriptor:

```json
{
  "skill_id": "science/literature-survey",
  "display_name": "Literature Survey",
  "version": "1.0.0",
  "license": "MIT",
  "category": "research",
  "description": "Conduct a systematic literature survey across PubMed and ChEMBL.",
  "tools": ["pubmed_search", "chembl_search", "artifact_write"],
  "entry_point": "literature-survey.md",
  "protocol": "ACP-extension",
  "dependencies": [],
  "data_sources": ["pubmed", "chembl"],
  "admission": {
    "status": "approved",
    "reviewed_at": "2026-07-25",
    "reviewer": "lumen-science-admission"
  }
}
```

## Categories

- `research` — literature survey, research brief
- `analysis` — statistical analysis, data processing
- `visualization` — chart design, molecule viewing, figure preparation
- `quality` — integrity audit, traceability review
- `compute` — C2D computation, modeling
- `biology` — molecular biology, genomics
- `chemistry` — cheminformatics, drug discovery

## Admission Policy

- MIT/BSD/Apache-2.0: auto-approved after review
- GPL: permanently rejected
- Unknown/unlicensed: pending manual review
- Commercial data terms: separate review required

## Registry

See `registry.json` for the complete skill registry.
