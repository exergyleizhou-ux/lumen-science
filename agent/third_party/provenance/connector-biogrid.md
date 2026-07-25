# biogrid (biogrid) — provenance record

**Milestone:** DS-auto  
**Date:** 2026-07-24  

## Upstream source
- Source: synsci-openscience (Apache-2.0)
- SHA-256: 20a224469ec4b288498a9a88456075dadf7d59cc15289abfe9d4215073ae952d

## Service identity
- Service owner: BioGRID (Université de Montréal / Princeton)
- Official base URL: https://webservice.thebiogrid.org/
- Egress hosts: webservice.thebiogrid.org
- TOS: https://thebiogrid.org/terms.php

## Rust Lumen adaptation
- Module: connectors/biogrid.rs  
- Adapter: BiogridAdapter registered in global REGISTRY

## Data policy
- Auth class: access_key
- Rate limit: not applicable (rejected)
- License: BioGRID data CC BY 4.0; requires free registration
- Data class: 

## Evidence
- Fixture: fixtures/connector_biogrid_*.json
