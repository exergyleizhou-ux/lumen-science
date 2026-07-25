# interpro (interpro) — provenance record

**Milestone:** DS-auto  
**Date:** 2026-07-24  

## Upstream source
- Source: synsci-openscience (Apache-2.0)
- SHA-256: c09931410c2abbf1578c803845bcb887f3daf2a8edf5f6acdd4e4027d4db674c

## Service identity
- Service owner: EMBL-EBI / InterPro Consortium
- Official base URL: https://www.ebi.ac.uk/interpro/api/entry/interpro/
- Egress hosts: www.ebi.ac.uk
- TOS: https://www.ebi.ac.uk/about/terms-of-use

## Rust Lumen adaptation
- Module: connectors/interpro.rs  
- Adapter: InterproAdapter registered in global REGISTRY

## Data policy
- Auth class: none
- Rate limit: 1 request per 1000ms
- License: InterPro data CC0; EMBL-EBI terms
- Data class: 

## Evidence
- Fixture: fixtures/connector_interpro_*.json
