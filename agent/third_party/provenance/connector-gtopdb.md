# gtopdb (gtopdb) — provenance record

**Milestone:** DS-auto  
**Date:** 2026-07-24  

## Upstream source
- Source: synsci-openscience (Apache-2.0)
- SHA-256: dd22d2a0b57cf7804be6509550f83d2562189fbbcf0a457da3a9755cda2e5134

## Service identity
- Service owner: IUPHAR/BPS Guide to PHARMACOLOGY
- Official base URL: https://www.guidetopharmacology.org/services/
- Egress hosts: www.guidetopharmacology.org
- TOS: https://www.guidetopharmacology.org/about.jsp

## Rust Lumen adaptation
- Module: connectors/gtopdb.rs  
- Adapter: GtopdbAdapter registered in global REGISTRY

## Data policy
- Auth class: none
- Rate limit: 1 request per 1000ms
- License: GtoPdb data CC BY-SA 4.0
- Data class: 

## Evidence
- Fixture: fixtures/connector_gtopdb_*.json
