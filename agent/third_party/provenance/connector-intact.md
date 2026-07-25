# intact (intact) — provenance record

**Milestone:** DS-auto  
**Date:** 2026-07-24  

## Upstream source
- Source: synsci-openscience (Apache-2.0)
- SHA-256: 87710155878bf8795e8f230cbf8047ed9db7f46bb286e8c72b17b12aa1e7d3fd

## Service identity
- Service owner: EMBL-EBI / IntAct
- Official base URL: https://www.ebi.ac.uk/intact/ws/
- Egress hosts: www.ebi.ac.uk
- TOS: https://www.ebi.ac.uk/about/terms-of-use

## Rust Lumen adaptation
- Module: connectors/intact.rs  
- Adapter: IntactAdapter registered in global REGISTRY

## Data policy
- Auth class: none
- Rate limit: 1 request per 1000ms
- License: IntAct data freely available; EMBL-EBI terms
- Data class: 

## Evidence
- Fixture: fixtures/connector_intact_*.json
