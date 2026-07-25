# uniprot (uniprot) — provenance record

**Milestone:** DS-auto  
**Date:** 2026-07-24  

## Upstream source
- Source: synsci-openscience (Apache-2.0)
- SHA-256: d8ef0c3276ee835936f739d6c5c6e5d72a7e575ec03d4ffc3f24700a100f3aed

## Service identity
- Service owner: UniProt Consortium (EMBL-EBI / SIB / PIR)
- Official base URL: https://rest.uniprot.org/uniprotkb/
- Egress hosts: rest.uniprot.org
- TOS: https://www.uniprot.org/help/license

## Rust Lumen adaptation
- Module: connectors/uniprot.rs  
- Adapter: UniprotAdapter registered in global REGISTRY

## Data policy
- Auth class: none
- Rate limit: 1 request per 1000ms
- License: CC-BY-4.0 for copyrightable database content; UniProt provides no correctness warranty; some data may be covered by patents
- Data class: 

## Evidence
- Fixture: fixtures/connector_uniprot_*.json
