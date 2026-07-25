# geo (geo) — provenance record

**Milestone:** DS-auto  
**Date:** 2026-07-24  

## Upstream source
- Source: synsci-openscience (Apache-2.0)
- SHA-256: 14e867fd536a6dd0aedcd9bbee3902b72a31c37f06e7c912843c69eb99a9c27c

## Service identity
- Service owner: NCBI / NIH
- Official base URL: https://eutils.ncbi.nlm.nih.gov/entrez/eutils/
- Egress hosts: eutils.ncbi.nlm.nih.gov
- TOS: https://www.ncbi.nlm.nih.gov/home/about/policies/

## Rust Lumen adaptation
- Module: connectors/geo.rs  
- Adapter: GeoAdapter registered in global REGISTRY

## Data policy
- Auth class: none
- Rate limit: 3 requests per 1000ms
- License: NCBI policies; GEO data freely available
- Data class: 

## Evidence
- Fixture: fixtures/connector_geo_*.json
