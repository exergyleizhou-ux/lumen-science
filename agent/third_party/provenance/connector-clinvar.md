# clinvar (clinvar) — provenance record

**Milestone:** DS-auto  
**Date:** 2026-07-24  

## Upstream source
- Source: synsci-openscience (Apache-2.0)
- SHA-256: 2719340e456bff16798b93fca59ce387d19331b3c79ce0315f3546b6bd52bce9

## Service identity
- Service owner: NCBI / NIH
- Official base URL: https://eutils.ncbi.nlm.nih.gov/entrez/eutils/
- Egress hosts: eutils.ncbi.nlm.nih.gov
- TOS: https://www.ncbi.nlm.nih.gov/home/about/policies/

## Rust Lumen adaptation
- Module: connectors/clinvar.rs  
- Adapter: ClinvarAdapter registered in global REGISTRY

## Data policy
- Auth class: none
- Rate limit: 3 requests per 1000ms
- License: NCBI policies; ClinVar data freely available
- Data class: 

## Evidence
- Fixture: fixtures/connector_clinvar_*.json
