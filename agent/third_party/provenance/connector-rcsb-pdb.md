# rcsb-pdb (rcsb-pdb) — provenance record

**Milestone:** DS-auto  
**Date:** 2026-07-24  

## Upstream source
- Source: synsci-openscience (Apache-2.0)
- SHA-256: 8cd1757d0aa7a6305f9c1fdebfc7cd3e7f2b0c865e2cc368e2a68cdce8332a71

## Service identity
- Service owner: RCSB Protein Data Bank (Rutgers/UCSD/UCSF)
- Official base URL: https://search.rcsb.org/rcsbsearch/v2/
- Egress hosts: search.rcsb.org, data.rcsb.org
- TOS: https://www.rcsb.org/pages/policies

## Rust Lumen adaptation
- Module: connectors/rcsb_pdb.rs  
- Adapter: RcsbPdbAdapter registered in global REGISTRY

## Data policy
- Auth class: none
- Rate limit: 1 request per 1000ms
- License: PDB data CC0; RCSB PDB terms
- Data class: 

## Evidence
- Fixture: fixtures/connector_rcsb-pdb_*.json
