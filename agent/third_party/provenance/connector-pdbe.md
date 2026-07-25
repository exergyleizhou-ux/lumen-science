# pdbe (pdbe) — provenance record

**Milestone:** DS-auto  
**Date:** 2026-07-24  

## Upstream source
- Source: synsci-openscience (Apache-2.0)
- SHA-256: b4fb0a7ca819620b199727c8fd0ffa370b86cfcb1e44ea96bba0218e6d152db6

## Service identity
- Service owner: Protein Data Bank in Europe (EMBL-EBI)
- Official base URL: https://www.ebi.ac.uk/pdbe/
- Egress hosts: www.ebi.ac.uk
- TOS: https://www.ebi.ac.uk/about/terms-of-use

## Rust Lumen adaptation
- Module: connectors/pdbe.rs  
- Adapter: PdbeAdapter registered in global REGISTRY

## Data policy
- Auth class: none
- Rate limit: 1 request per 1000ms
- License: PDB data CC0; EMBL-EBI terms
- Data class: 

## Evidence
- Fixture: fixtures/connector_pdbe_*.json
