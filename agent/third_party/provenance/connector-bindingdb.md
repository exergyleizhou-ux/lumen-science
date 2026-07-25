# bindingdb (bindingdb) — provenance record

**Milestone:** DS-auto  
**Date:** 2026-07-24  

## Upstream source
- Source: synsci-openscience (Apache-2.0)
- SHA-256: 975c19510e860f00698f8939c965c59767eea630f7c77fe478d522e901f3d160

## Service identity
- Service owner: BindingDB (UCSD/Skaggs)
- Official base URL: https://bindingdb.org/rest/
- Egress hosts: bindingdb.org
- TOS: https://www.bindingdb.org/rwd/bind/termsofuse.jsp

## Rust Lumen adaptation
- Module: connectors/bindingdb.rs  
- Adapter: BindingdbAdapter registered in global REGISTRY

## Data policy
- Auth class: none
- Rate limit: 1 request per 2000ms
- License: BindingDB data CC BY 4.0
- Data class: 

## Evidence
- Fixture: fixtures/connector_bindingdb_*.json
