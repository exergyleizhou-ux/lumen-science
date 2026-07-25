# arxiv (arxiv) — provenance record

**Milestone:** DS-auto  
**Date:** 2026-07-24  

## Upstream source
- Source: synsci-openscience (Apache-2.0)
- SHA-256: 46b9cf1fcae2c3c083a26fc6e1cd3e2fa20a3ced721cbbc3919da32ac6abdf49

## Service identity
- Service owner: Cornell University / arXiv
- Official base URL: https://export.arxiv.org/api/
- Egress hosts: export.arxiv.org
- TOS: https://info.arxiv.org/help/api/index.html

## Rust Lumen adaptation
- Module: connectors/arxiv.rs  
- Adapter: ArxivAdapter registered in global REGISTRY

## Data policy
- Auth class: none
- Rate limit: 1 request per 3000ms
- License: arXiv metadata freely accessible; individual article rights vary by submitter/license
- Data class: 

## Evidence
- Fixture: fixtures/connector_arxiv_*.json
