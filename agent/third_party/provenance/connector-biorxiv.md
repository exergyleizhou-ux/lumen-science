# biorxiv (biorxiv) — provenance record

**Milestone:** DS-auto  
**Date:** 2026-07-24  

## Upstream source
- Source: synsci-openscience (Apache-2.0)
- SHA-256: c45db042ec72c57b88f23f3ed122246f1f5eed9b1f9a59b03657d1cee07b21f8

## Service identity
- Service owner: Cold Spring Harbor Laboratory (CSHL)
- Official base URL: https://api.biorxiv.org/
- Egress hosts: api.biorxiv.org
- TOS: https://www.biorxiv.org/about/terms

## Rust Lumen adaptation
- Module: connectors/biorxiv.rs  
- Adapter: BiorxivAdapter registered in global REGISTRY

## Data policy
- Auth class: none
- Rate limit: 1 request per 2000ms (conservative local cap)
- License: Preprint content rights vary by author; bioRxiv TOS at https://www.biorxiv.org/about/terms
- Data class: 

## Evidence
- Fixture: fixtures/connector_biorxiv_*.json
