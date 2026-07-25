# ucsc (ucsc) — provenance record

**Milestone:** DS-auto  
**Date:** 2026-07-24  

## Upstream source
- Source: synsci-openscience (Apache-2.0)
- SHA-256: 401949e4c3f7e38cfd64d53f778cf653ddd2706a21eb6a3cb8b9a3f0d81c82ea

## Service identity
- Service owner: UC Santa Cruz Genomics Institute
- Official base URL: https://api.genome.ucsc.edu/
- Egress hosts: api.genome.ucsc.edu
- TOS: https://genome.ucsc.edu/license/

## Rust Lumen adaptation
- Module: connectors/ucsc.rs  
- Adapter: UcscAdapter registered in global REGISTRY

## Data policy
- Auth class: none
- Rate limit: 1 request per 1000ms
- License: UCSC Genome Browser data freely available for academic use; commercial use subject to license terms
- Data class: 

## Evidence
- Fixture: fixtures/connector_ucsc_*.json
