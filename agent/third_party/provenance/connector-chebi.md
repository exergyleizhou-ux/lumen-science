# chebi (chebi) — provenance record

**Milestone:** DS-auto  
**Date:** 2026-07-24  

## Upstream source
- Source: synsci-openscience (Apache-2.0)
- SHA-256: d3d984583a4e3dda7ebd0a1253d1898bb47501376902e5b4920a5fa22dfd1fe7

## Service identity
- Service owner: EMBL-EBI
- Official base URL: https://www.ebi.ac.uk/ols4/api/
- Egress hosts: www.ebi.ac.uk
- TOS: https://www.ebi.ac.uk/about/terms-of-use

## Rust Lumen adaptation
- Module: connectors/chebi.rs  
- Adapter: ChebiAdapter registered in global REGISTRY

## Data policy
- Auth class: none
- Rate limit: 1 request per 1000ms
- License: ChEBI data CC BY 4.0; EMBL-EBI terms
- Data class: 

## Evidence
- Fixture: fixtures/connector_chebi_*.json
