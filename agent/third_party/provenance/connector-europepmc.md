# europepmc (europepmc) — provenance record

**Milestone:** DS-auto  
**Date:** 2026-07-24  

## Upstream source
- Source: synsci-openscience (Apache-2.0)
- SHA-256: 6d0715995d17b13dcbf80b20cd7d3e8257ddb1cc11e36d7a9d4e570973ff525f

## Service identity
- Service owner: Europe PMC / EMBL-EBI
- Official base URL: https://www.ebi.ac.uk/europepmc/webservices/rest/
- Egress hosts: www.ebi.ac.uk
- TOS: https://europepmc.org/Copyright

## Rust Lumen adaptation
- Module: connectors/europepmc.rs  
- Adapter: EuropepmcAdapter registered in global REGISTRY

## Data policy
- Auth class: none
- Rate limit: 1 request per 1000ms (conservative local cap, not an official service quota)
- License: Article-level; Europe PMC metadata freely accessible; individual articles have their own copyright/license
- Data class: 

## Evidence
- Fixture: fixtures/connector_europepmc_*.json
