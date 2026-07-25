# gtex (gtex) — provenance record

**Milestone:** DS-auto  
**Date:** 2026-07-24  

## Upstream source
- Source: synsci-openscience (Apache-2.0)
- SHA-256: c04959dfd3d42c95a70dbd8d074127708340fe489f9f517a3076fa2456b26f69

## Service identity
- Service owner: GTEx Consortium (Broad Institute / NIH)
- Official base URL: https://gtexportal.org/api/v2/
- Egress hosts: gtexportal.org
- TOS: https://gtexportal.org/home/documentationPage

## Rust Lumen adaptation
- Module: connectors/gtex.rs  
- Adapter: GtexAdapter registered in global REGISTRY

## Data policy
- Auth class: none
- Rate limit: 1 request per 1000ms
- License: GTEx data freely available; NIH dbGaP terms for controlled-access subsets
- Data class: 

## Evidence
- Fixture: fixtures/connector_gtex_*.json
