# depmap (depmap) — provenance record

**Milestone:** DS-auto  
**Date:** 2026-07-24  

## Upstream source
- Source: synsci-openscience (Apache-2.0)
- SHA-256: f3bc2650126624290e2b34541de4b906c6657bf7741168f28804491c125faae9

## Service identity
- Service owner: Broad Institute / Sanger Institute
- Official base URL: https://depmap.org/portal/
- Egress hosts: depmap.org
- TOS: https://depmap.org/portal/download/all/

## Rust Lumen adaptation
- Module: connectors/depmap.rs  
- Adapter: DepmapAdapter registered in global REGISTRY

## Data policy
- Auth class: none
- Rate limit: 1 request per 2000ms
- License: DepMap data CC BY 4.0
- Data class: 

## Evidence
- Fixture: fixtures/connector_depmap_*.json
