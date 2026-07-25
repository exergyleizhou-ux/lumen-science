# gnomad (gnomad) — provenance record

**Milestone:** DS-auto  
**Date:** 2026-07-24  

## Upstream source
- Source: synsci-openscience (Apache-2.0)
- SHA-256: 6d707bddd75dc10dd3adff078d54222b9af894151987201ff4582ec6e9f952ad

## Service identity
- Service owner: Broad Institute
- Official base URL: https://gnomad.broadinstitute.org/api/
- Egress hosts: gnomad.broadinstitute.org
- TOS: https://gnomad.broadinstitute.org/terms

## Rust Lumen adaptation
- Module: connectors/gnomad.rs  
- Adapter: GnomadAdapter registered in global REGISTRY

## Data policy
- Auth class: none
- Rate limit: 1 request per 1000ms
- License: gnomAD data ODC Public Domain Dedication and License (ODC-By for some subsets); Broad Institute terms
- Data class: 

## Evidence
- Fixture: fixtures/connector_gnomad_*.json
