# chembl (chembl) — provenance record

**Milestone:** DS-auto  
**Date:** 2026-07-24  

## Upstream source
- Source: synsci-openscience (Apache-2.0)
- SHA-256: 950ae2524b33144d21de692e9bb830be90324c43c764a7779f8e02ad130ffa90

## Service identity
- Service owner: EMBL-EBI / ChEMBL
- Official base URL: https://www.ebi.ac.uk/chembl/api/data/
- Egress hosts: www.ebi.ac.uk
- TOS: https://www.ebi.ac.uk/about/terms-of-use

## Rust Lumen adaptation
- Module: connectors/chembl.rs  
- Adapter: ChemblAdapter registered in global REGISTRY

## Data policy
- Auth class: none
- Rate limit: 5 requests per 1000ms
- License: ChEMBL data CC-BY-SA-3.0; EMBL-EBI terms
- Data class: 

## Evidence
- Fixture: fixtures/connector_chembl_*.json
