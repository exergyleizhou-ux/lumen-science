# string-db (string-db) — provenance record

**Milestone:** DS-auto  
**Date:** 2026-07-24  

## Upstream source
- Source: synsci-openscience (Apache-2.0)
- SHA-256: c9e0519400f79ac77d2a03787f6cbc89585a2cdf8df852a2dbd399a4ad049c30

## Service identity
- Service owner: STRING Consortium (CPR / EMBL / SIB / KU / TUD)
- Official base URL: https://string-db.org/api/
- Egress hosts: string-db.org
- TOS: https://string-db.org/cgi/access

## Rust Lumen adaptation
- Module: connectors/string_db.rs  
- Adapter: StringDbAdapter registered in global REGISTRY

## Data policy
- Auth class: none
- Rate limit: 1 request per 1000ms
- License: STRING data CC BY 4.0
- Data class: 

## Evidence
- Fixture: fixtures/connector_string-db_*.json
