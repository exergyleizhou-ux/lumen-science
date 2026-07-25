# opentargets (opentargets) — provenance record

**Milestone:** DS-auto  
**Date:** 2026-07-24  

## Upstream source
- Source: synsci-openscience (Apache-2.0)
- SHA-256: e0c740e4e750ce917f6d0ae8d33ea8570a5a74aa2680e7bac014be01bf56c8f8

## Service identity
- Service owner: Open Targets (EMBL-EBI / Wellcome Sanger / GSK / Biogen / Celgene)
- Official base URL: https://api.platform.opentargets.org/api/v4/graphql
- Egress hosts: api.platform.opentargets.org
- TOS: https://platform.opentargets.org/documentation

## Rust Lumen adaptation
- Module: connectors/opentargets.rs  
- Adapter: OpentargetsAdapter registered in global REGISTRY

## Data policy
- Auth class: none
- Rate limit: 1 request per 1000ms
- License: Open Targets data CC BY 4.0
- Data class: 

## Evidence
- Fixture: fixtures/connector_opentargets_*.json
