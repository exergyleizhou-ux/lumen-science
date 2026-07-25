# Science Connector Admission Matrix

**Purpose**: Track each connector through the full product path before merging into main.
**Last updated**: 2026-07-25

## Product Path Checklist

Each connector must pass all 8 gates:
1. **descriptor** — fixed descriptor (host, scheme, bounds, timeout, rights)
2. **parser** — fail-closed parser with malformed/null/oversize tests
3. **fixture** — offline fixture (success, empty, malformed, timeout, bounds)
4. **registry** — registered in `connectors::registry()`
5. **SessionActor** — routed through `SessionHandle` → permission → artifact
6. **artifact** — durable artifact with SHA-256 and provenance
7. **replay** — `events_after(seq)` canonical replay equality
8. **built-binary** — `cargo test` on built binary, N > 0 real tests

## Connectors

| # | Connector | desc | parser | fixture | registry | actor | artifact | replay | binary | Status |
|---|---|---|---|---|---|---|---|---|---|---|
| 1 | pubmed | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | DONE |
| 2 | chembl | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | PENDING |
| 3 | crossref | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | PENDING |
| 4 | uniprot | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | PENDING |
| 5 | europepmc | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | PENDING |
| 6 | openalex | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | PENDING |
| 7 | semantic-scholar | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | PENDING |
| 8 | arxiv | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | PENDING |
| 9 | biorxiv | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | PENDING |
| 10 | rcsb-pdb | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | PENDING |
| 11 | pdbe | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | PENDING |
| 12 | alphafold | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | PENDING |
| 13 | interpro | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | PENDING |
| 14 | sifts | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | PENDING |
| 15 | ensembl | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | PENDING |
| 16 | ncbi-gene | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | PENDING |
| 17 | dbsnp | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | PENDING |
| 18 | clinvar | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | PENDING |
| 19 | gnomad | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | PENDING |
| 20 | ucsc | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | PENDING |
| 21 | mygene | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | PENDING |
| 22 | myvariant | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | PENDING |
| 23 | pubchem | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | PENDING |
| 24 | bindingdb | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | PENDING |
| 25 | gtopdb | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | PENDING |
| 26 | surechembl | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | PENDING |
| 27 | chebi | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | PENDING |
| 28 | reactome | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | PENDING |
| 29 | string-db | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | PENDING |
| 30 | intact | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | PENDING |
| 31 | wikipathways | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | PENDING |
| 32 | opentargets | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | PENDING |
| 33 | geo | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | PENDING |
| 34 | arrayexpress | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | PENDING |
| 35 | gtex | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | PENDING |
| 36 | hpa | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | PENDING |
| 37 | expression-atlas | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | PENDING |
| 38 | single-cell-atlas | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | PENDING |
| 39 | depmap | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | PENDING |
| 40 | eutils | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | PENDING |
| 41 | biogrid | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | REJECTED |
| 42 | kegg | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | PENDING |

## Progress

| Gate | Done | Total | % |
|---|---|---|---|
| descriptor | 40 | 42 | 95% |
| parser | 40 | 42 | 95% |
| fixture | 40 | 42 | 95% |
| registry | 40 | 42 | 95% |
| SessionActor | 1 | 42 | 2% |
| artifact | 1 | 42 | 2% |
| replay | 1 | 42 | 2% |
| built-binary | 1 | 42 | 2% |
| **Overall** | **1 / 42** | | **2% product path complete** |
