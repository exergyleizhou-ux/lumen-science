---
name: research-brief
description: Generate a multi-source Research Brief using the 4-source pipeline (PubMed + ChEMBL + GEO + Oasis datasets). Structured output with provenance for every source.
runAs: inline
allowedTools: [science_domain_call, science_list_domains, science_brief_generate, write_file, read_file]
---

## Purpose
Produce a structured Research Brief from the 4-source Lumen pipeline, suitable for grant proposals, literature reviews, or experiment planning.

## Sources
1. **PubMed** — literature search, abstracts, PMC fulltext
2. **ChEMBL** — compound bioactivities, target data
3. **GEO** — gene expression datasets
4. **Oasis** — verified datasets with provenance

## Output Format
```markdown
# Research Brief: {Topic}

## Literature Landscape
- Key findings from PubMed/OpenAlex
- Recent advances (last 5 years)

## Molecular Targets
- ChEMBL compound-target interactions
- Known mechanisms

## Expression Data
- GEO datasets, fold changes, conditions

## Oasis Verified Datasets
- Available C2D-ready datasets
- Provenance links

## Summary & Next Steps
```

## Rules
- Every section must cite at least one domain tool result
- Brief is saved to `reports/brief.md` with provenance.jsonl entries
