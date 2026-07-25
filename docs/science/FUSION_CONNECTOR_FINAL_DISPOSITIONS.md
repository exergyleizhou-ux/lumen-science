# Lumen Science — 42 Connector Final Dispositions

**Date:** 2026-07-25
**Authority:** Rust Lumen SessionActor
**Status:** All 42 connectors have a final disposition. Zero unresolved.

## Summary

| Disposition | Count |
|-------------|-------|
| `implemented` | 40 |
| `rejected-unsafe-or-duplicate` | 1 (BioGRID) |
| `rejected-license-or-terms` | 1 (KEGG) |
| **Total** | **42** |
| **Unresolved** | **0** |

## Full Registry

| # | DS | ID | Display Name | Admission | Disposition |
|---|-----|----|-------------|-----------|-------------|
| 0 | DS-0 | `pubmed` | PubMed | approved | implemented |
| 1 | DS-0 | `chembl` | ChEMBL | approved | implemented |
| 2 | DS-0 | `crossref` | Crossref | approved | implemented |
| 3 | DS-0 | `uniprot` | UniProtKB | approved | implemented |
| 4 | DS-0 | `europepmc` | Europe PMC | approved | implemented |
| 5 | DS-0 | `openalex` | OpenAlex | approved | implemented |
| 6 | DS-2 | `semantic-scholar` | Semantic Scholar | approved | implemented |
| 7 | DS-3 | `arxiv` | arXiv | approved | implemented |
| 8 | DS-4 | `rcsb-pdb` | RCSB PDB | approved | implemented |
| 9 | DS-5 | `alphafold` | AlphaFold DB | approved | implemented |
| 10 | DS-6 | `ensembl` | Ensembl | approved | implemented |
| 11 | DS-7 | `pubchem` | PubChem | approved | implemented |
| 12 | DS-8 | `biorxiv` | bioRxiv / medRxiv | approved | implemented |
| 13 | DS-9 | `interpro` | InterPro | approved | implemented |
| 14 | DS-10 | `pdbe` | PDBe | approved | implemented |
| 15 | DS-11 | `sifts` | SIFTS | approved | implemented |
| 16 | DS-12 | `clinvar` | ClinVar | approved | implemented |
| 17 | DS-13 | `dbsnp` | dbSNP | approved | implemented |
| 18 | DS-14 | `eutils` | NCBI E-utilities | approved | implemented |
| 19 | DS-15 | `gnomad` | gnomAD | approved | implemented |
| 20 | DS-16 | `mygene` | MyGene.info | approved | implemented |
| 21 | DS-17 | `myvariant` | MyVariant.info | approved | implemented |
| 22 | DS-18 | `ncbi-gene` | NCBI Gene | approved | implemented |
| 23 | DS-19 | `ucsc` | UCSC Genome Browser | approved | implemented |
| 24 | DS-20 | `bindingdb` | BindingDB | approved | implemented |
| 25 | DS-21 | `chebi` | ChEBI | approved | implemented |
| 26 | DS-22 | `gtopdb` | GtoPdb | approved | implemented |
| 27 | DS-23 | `surechembl` | SureChEMBL | approved | implemented |
| 28 | DS-24 | `biogrid` | BioGRID | **rejected** | rejected-unsafe-or-duplicate |
| 29 | DS-25 | `intact` | IntAct | approved | implemented |
| 30 | DS-26 | `kegg` | KEGG | **rejected** | rejected-license-or-terms |
| 31 | DS-27 | `opentargets` | Open Targets | approved | implemented |
| 32 | DS-28 | `reactome` | Reactome | approved | implemented |
| 33 | DS-29 | `string-db` | STRING | approved | implemented |
| 34 | DS-30 | `wikipathways` | WikiPathways | approved | implemented |
| 35 | DS-31 | `arrayexpress` | ArrayExpress | approved | implemented |
| 36 | DS-32 | `depmap` | DepMap | approved | implemented |
| 37 | DS-33 | `expression-atlas` | Expression Atlas | approved | implemented |
| 38 | DS-34 | `geo` | NCBI GEO | approved | implemented |
| 39 | DS-35 | `gtex` | GTEx | approved | implemented |
| 40 | DS-36 | `hpa` | Human Protein Atlas | approved | implemented |
| 41 | DS-37 | `single-cell-atlas` | Single Cell Atlas | approved | implemented |

## Rejection Evidence

### BioGRID (DS-24) — rejected-unsafe-or-duplicate

- **Reason:** `accessKey` credential passed as URL query parameter, violating Lumen `credential-never-in-URL`.
- **Evidence:** `docs/science/FUSION_DS24_BIOGRID_REJECTION_CLOSURE.md`
- **Alternative:** IntAct (DS-25), NCBI Gene (DS-18) for interaction data.

### KEGG (DS-26) — rejected-license-or-terms

- **Reason:** Commercial use requires paid subscription. No blanket redistribution license.
- **Evidence:** `docs/science/FUSION_DS26_KEGG_LICENSE_CLOSURE.md`
- **Re-evaluation:** If KEGG adopts CC0 or CC-BY-4.0, or Lumen Science obtains commercial license.

## Machine Gate

```bash
# Verify 42 total, 0 unresolved
jq '.items | length' docs/science/fusion-sources.lock.json
# Expected: 42

# Verify no null dispositions
jq '[.items[] | select(.final_disposition == null)] | length' docs/science/fusion-sources.lock.json
# Expected: 0
```
