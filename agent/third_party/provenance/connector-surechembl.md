# surechembl (surechembl) — provenance record

**Milestone:** DS-auto  
**Date:** 2026-07-24  

## Upstream source
- Source: synsci-openscience (Apache-2.0)
- SHA-256: 24c90c885e1724fb03f4c91edcbd46f71af470657c201e99da65deedc0766752

## Service identity
- Service owner: EMBL-EBI
- Official base URL: https://www.surechembl.org/api/
- Egress hosts: www.surechembl.org
- TOS: https://www.ebi.ac.uk/about/terms-of-use

## Rust Lumen adaptation
- Module: connectors/surechembl.rs  
- Adapter: SurechemblAdapter registered in global REGISTRY

## Data policy
- Auth class: none
- Rate limit: 1 request per 1000ms
- License: SureChEMBL data freely available; EMBL-EBI terms
- Data class: 

## Evidence
- Fixture: fixtures/connector_surechembl_*.json
