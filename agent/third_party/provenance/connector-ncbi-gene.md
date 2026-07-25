# ncbi-gene (ncbi-gene) — provenance record

**Milestone:** DS-auto  
**Date:** 2026-07-24  

## Upstream source
- Source: synsci-openscience (Apache-2.0)
- SHA-256: a09695149eca7b8a61f00e11ab70958434f3c86006492c143b3a54a34f453e1b

## Service identity
- Service owner: NCBI / NIH
- Official base URL: https://eutils.ncbi.nlm.nih.gov/entrez/eutils/
- Egress hosts: eutils.ncbi.nlm.nih.gov
- TOS: https://www.ncbi.nlm.nih.gov/home/about/policies/

## Rust Lumen adaptation
- Module: connectors/ncbi_gene.rs  
- Adapter: NcbiGeneAdapter registered in global REGISTRY

## Data policy
- Auth class: none
- Rate limit: 3 requests per 1000ms
- License: NCBI policies; Gene data freely available
- Data class: 

## Evidence
- Fixture: fixtures/connector_ncbi-gene_*.json
