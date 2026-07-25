# crossref (crossref) — provenance record

**Milestone:** DS-auto  
**Date:** 2026-07-24  

## Upstream source
- Source: synsci-openscience (Apache-2.0)
- SHA-256: 6ffb5740603bd9f40923bdff0e8229b6134c76ebd07fd781bfb3fa2e5cd38285

## Service identity
- Service owner: Crossref
- Official base URL: https://api.crossref.org/
- Egress hosts: api.crossref.org
- TOS: https://www.crossref.org/documentation/retrieve-metadata/

## Rust Lumen adaptation
- Module: connectors/crossref.rs  
- Adapter: CrossrefAdapter registered in global REGISTRY

## Data policy
- Auth class: none
- Rate limit: 1 request per 1000ms (conservative local cap)
- License: Bibliographic facts generally not copyrightable; Crossref-generated metadata CC0; abstracts retain publisher/author rights
- Data class: 

## Evidence
- Fixture: fixtures/connector_crossref_*.json
