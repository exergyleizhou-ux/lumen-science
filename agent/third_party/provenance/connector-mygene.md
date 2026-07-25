# mygene (mygene) — provenance record

**Milestone:** DS-auto  
**Date:** 2026-07-24  

## Upstream source
- Source: synsci-openscience (Apache-2.0)
- SHA-256: d24c51ab114f6aaadc4eb7f9bea5f25758a7f5c9c196e5c35d63119a68cb2e17

## Service identity
- Service owner: Su Lab / Scripps Research
- Official base URL: https://mygene.info/v3/
- Egress hosts: mygene.info
- TOS: https://mygene.info/

## Rust Lumen adaptation
- Module: connectors/mygene.rs  
- Adapter: MygeneAdapter registered in global REGISTRY

## Data policy
- Auth class: none
- Rate limit: 1 request per 1000ms
- License: MyGene.info data from multiple sources; service free
- Data class: 

## Evidence
- Fixture: fixtures/connector_mygene_*.json
