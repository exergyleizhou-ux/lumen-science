# sifts (sifts) — provenance record

**Milestone:** DS-auto  
**Date:** 2026-07-24  

## Upstream source
- Source: synsci-openscience (Apache-2.0)
- SHA-256: a46d656832d81e299a9f828219854572982e04333c30cd44688a271861a92667

## Service identity
- Service owner: EMBL-EBI / PDBe
- Official base URL: https://www.ebi.ac.uk/pdbe/api/mappings/
- Egress hosts: www.ebi.ac.uk
- TOS: https://www.ebi.ac.uk/about/terms-of-use

## Rust Lumen adaptation
- Module: connectors/sifts.rs  
- Adapter: SiftsAdapter registered in global REGISTRY

## Data policy
- Auth class: none
- Rate limit: 1 request per 500ms (per accession in fan-out)
- License: SIFTS data freely available; EMBL-EBI terms
- Data class: 

## Evidence
- Fixture: fixtures/connector_sifts_*.json
