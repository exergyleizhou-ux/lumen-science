# reactome (reactome) — provenance record

**Milestone:** DS-auto  
**Date:** 2026-07-24  

## Upstream source
- Source: synsci-openscience (Apache-2.0)
- SHA-256: 835bd7ff9198e999bc7bb997456ec9f621bb96039cb12d0b8c003712084d5e05

## Service identity
- Service owner: Reactome (OICR / EMBL-EBI / NYU / OHSU)
- Official base URL: https://reactome.org/ContentService/
- Egress hosts: reactome.org
- TOS: https://reactome.org/documentation/data-license-agreement

## Rust Lumen adaptation
- Module: connectors/reactome.rs  
- Adapter: ReactomeAdapter registered in global REGISTRY

## Data policy
- Auth class: none
- Rate limit: 1 request per 1000ms
- License: Reactome data CC0; ContentService free
- Data class: 

## Evidence
- Fixture: fixtures/connector_reactome_*.json
