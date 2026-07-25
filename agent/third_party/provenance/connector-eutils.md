# eutils (eutils) — provenance record

**Milestone:** DS-auto  
**Date:** 2026-07-24  

## Upstream source
- Source: synsci-openscience (Apache-2.0)
- SHA-256: e4bd8ad50fedd1466ae45aa6c73d674ed689265fa5f4329f5c7cb96f4897dace

## Service identity
- Service owner: NCBI / NIH
- Official base URL: https://eutils.ncbi.nlm.nih.gov/entrez/eutils/
- Egress hosts: eutils.ncbi.nlm.nih.gov
- TOS: https://www.ncbi.nlm.nih.gov/home/about/policies/

## Rust Lumen adaptation
- Module: connectors/eutils.rs  
- Adapter: EutilsAdapter registered in global REGISTRY

## Data policy
- Auth class: none
- Rate limit: 3 requests per 1000ms
- License: NCBI policies; per-database data policies apply
- Data class: 

## Evidence
- Fixture: fixtures/connector_eutils_*.json
