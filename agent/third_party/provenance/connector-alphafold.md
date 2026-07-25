# alphafold (alphafold) — provenance record

**Milestone:** DS-auto  
**Date:** 2026-07-24  

## Upstream source
- Source: synsci-openscience (Apache-2.0)
- SHA-256: 80b5afd376a38eca56e2e3d5726a8847a5b40f8e76c037e5ff027a096cdd760d

## Service identity
- Service owner: EMBL-EBI / Google DeepMind
- Official base URL: https://alphafold.ebi.ac.uk/
- Egress hosts: alphafold.ebi.ac.uk
- TOS: https://www.ebi.ac.uk/about/terms-of-use

## Rust Lumen adaptation
- Module: connectors/alphafold.rs  
- Adapter: AlphafoldAdapter registered in global REGISTRY

## Data policy
- Auth class: none
- Rate limit: 1 request per 500ms (per accession in fan-out)
- License: AlphaFold DB data CC-BY-4.0; EMBL-EBI terms apply
- Data class: 

## Evidence
- Fixture: fixtures/connector_alphafold_*.json
