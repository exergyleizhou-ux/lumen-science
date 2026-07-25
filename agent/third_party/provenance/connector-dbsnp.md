# dbsnp (dbsnp) — provenance record

**Milestone:** DS-auto  
**Date:** 2026-07-24  

## Upstream source
- Source: synsci-openscience (Apache-2.0)
- SHA-256: 3d86526dbac01d4dd682d3c088b41f47ad7adcee2a9f3790026ffa9ee82de858

## Service identity
- Service owner: NCBI / NIH
- Official base URL: https://eutils.ncbi.nlm.nih.gov/entrez/eutils/
- Egress hosts: eutils.ncbi.nlm.nih.gov
- TOS: https://www.ncbi.nlm.nih.gov/home/about/policies/

## Rust Lumen adaptation
- Module: connectors/dbsnp.rs  
- Adapter: DbsnpAdapter registered in global REGISTRY

## Data policy
- Auth class: none
- Rate limit: 3 requests per 1000ms
- License: NCBI policies; dbSNP data freely available
- Data class: 

## Evidence
- Fixture: fixtures/connector_dbsnp_*.json
