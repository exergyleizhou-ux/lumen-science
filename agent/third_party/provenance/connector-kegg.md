# kegg (kegg) — provenance record

**Milestone:** DS-auto  
**Date:** 2026-07-24  

## Upstream source
- Source: synsci-openscience (Apache-2.0)
- SHA-256: bad1968c53f238db2a894bfbf714ff1a71cd48a5eb46252ef7905a5e8e546971

## Service identity
- Service owner: Kanehisa Laboratories / Kyoto University
- Official base URL: https://rest.kegg.jp/
- Egress hosts: rest.kegg.jp
- TOS: https://www.kegg.jp/kegg/legal.html

## Rust Lumen adaptation
- Module: connectors/kegg.rs  
- Adapter: KeggAdapter registered in global REGISTRY

## Data policy
- Auth class: none
- Rate limit: pending-license-review
- License: ⚠️ KEGG requires paid subscription for commercial use. Academic use free with attribution. See https://www.kegg.jp/kegg/legal.html
- Data class: 

## Evidence
- Fixture: fixtures/connector_kegg_*.json
