# ensembl (ensembl) — provenance record

**Milestone:** DS-auto  
**Date:** 2026-07-24  

## Upstream source
- Source: synsci-openscience (Apache-2.0)
- SHA-256: f7f1c0b2e637847c0a8164e4716f5f69d8b9fc20ad35f748fa6a1dfbb8e9969a

## Service identity
- Service owner: EMBL-EBI / Ensembl
- Official base URL: https://rest.ensembl.org/
- Egress hosts: rest.ensembl.org
- TOS: https://www.ebi.ac.uk/about/terms-of-use

## Rust Lumen adaptation
- Module: connectors/ensembl.rs  
- Adapter: EnsemblAdapter registered in global REGISTRY

## Data policy
- Auth class: none
- Rate limit: 3 requests per 1000ms
- License: Ensembl data freely available; EMBL-EBI terms
- Data class: 

## Evidence
- Fixture: fixtures/connector_ensembl_*.json
