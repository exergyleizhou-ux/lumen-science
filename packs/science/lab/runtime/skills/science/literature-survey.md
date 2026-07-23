---
name: literature-survey
description: Force literature-first research via domain MCP + Research Brief pipeline. Ensures every claim is sourced from PubMed/OpenAlex/ClinicalTrials before synthesis.
runAs: inline
allowedTools: [science_domain_call, science_list_domains, science_brief_generate, web_fetch, read_file, write_file]
---

## Purpose
Run a structured literature survey using bio-tools domain MCP servers. Every finding must be backed by domain tool results before the Brief synthesizes.

## Workflow
1. Call `science_list_domains` to see available databases
2. Search literature domain (pubmed): `science_domain_call` with tool `search_articles`
3. Cross-reference with OpenAlex: `science_domain_call` with literature domain
4. Check clinical trials if relevant: clinical-trials domain
5. Run `science_brief_generate` to produce a structured Research Brief
6. Save results to `reports/literature-survey.md`

## Rules
- Never invent citations — every reference must come from an MCP tool result
- Prefer 4-source pipeline (PubMed + ChEMBL + GEO + Oasis)
- Tag unresolved claims with `[NEEDS EVIDENCE]`
