# hpa (hpa) — provenance record

**Milestone:** DS-auto  
**Date:** 2026-07-24  

## Upstream source
- Source: synsci-openscience (Apache-2.0)
- SHA-256: 3d50b7eab40e699cfc6553c8889022dac0b33f9d99d0d2450fce0db51a4e0e00

## Service identity
- Service owner: Human Protein Atlas (KTH / Uppsala / SciLifeLab)
- Official base URL: https://www.proteinatlas.org/
- Egress hosts: www.proteinatlas.org
- TOS: https://www.proteinatlas.org/about/licence

## Rust Lumen adaptation
- Module: connectors/hpa.rs  
- Adapter: HpaAdapter registered in global REGISTRY

## Data policy
- Auth class: none
- Rate limit: 1 request per 1000ms
- License: HPA data CC BY-SA 3.0
- Data class: 

## Evidence
- Fixture: fixtures/connector_hpa_*.json
