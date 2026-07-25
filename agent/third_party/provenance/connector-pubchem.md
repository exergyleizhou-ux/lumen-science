# pubchem (pubchem) — provenance record

**Milestone:** DS-auto  
**Date:** 2026-07-24  

## Upstream source
- Source: synsci-openscience (Apache-2.0)
- SHA-256: 8d898974c5b57a756d24029e2a8bbe5927dd8a7f9bc7fd63438835c187d4e0f6

## Service identity
- Service owner: NCBI / NIH
- Official base URL: https://pubchem.ncbi.nlm.nih.gov/rest/pug/
- Egress hosts: pubchem.ncbi.nlm.nih.gov
- TOS: https://www.ncbi.nlm.nih.gov/home/about/policies/

## Rust Lumen adaptation
- Module: connectors/pubchem.rs  
- Adapter: PubchemAdapter registered in global REGISTRY

## Data policy
- Auth class: none
- Rate limit: 2 requests per 1000ms (two HTTP calls per search)
- License: PubChem data freely available; NCBI policies apply
- Data class: 

## Evidence
- Fixture: fixtures/connector_pubchem_*.json
