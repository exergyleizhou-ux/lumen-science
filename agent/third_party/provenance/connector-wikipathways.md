# wikipathways (wikipathways) — provenance record

**Milestone:** DS-auto  
**Date:** 2026-07-24  

## Upstream source
- Source: synsci-openscience (Apache-2.0)
- SHA-256: 3abadc3679d9cd72f223ce990cc6da9f5749bfe0d114da43aa3176aba553713b

## Service identity
- Service owner: WikiPathways (Gladstone / Maastricht / EBI)
- Official base URL: https://www.wikipathways.org/
- Egress hosts: www.wikipathways.org
- TOS: https://www.wikipathways.org/index.php/WikiPathways:License

## Rust Lumen adaptation
- Module: connectors/wikipathways.rs  
- Adapter: WikipathwaysAdapter registered in global REGISTRY

## Data policy
- Auth class: none
- Rate limit: 1 request per 5000ms (large catalog fetch)
- License: WikiPathways data CC0
- Data class: 

## Evidence
- Fixture: fixtures/connector_wikipathways_*.json
