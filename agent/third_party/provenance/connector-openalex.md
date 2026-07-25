# openalex (openalex) — provenance record

**Milestone:** DS-auto  
**Date:** 2026-07-24  

## Upstream source
- Source: synsci-openscience (Apache-2.0)
- SHA-256: 411164e9fb7cd118843e751f80b0eda4b53055f59e634e063bf21b6324935241

## Service identity
- Service owner: OurResearch / OpenAlex
- Official base URL: https://api.openalex.org/
- Egress hosts: api.openalex.org
- TOS: https://openalex.org/OpenAlex_termsofservice.pdf

## Rust Lumen adaptation
- Module: connectors/openalex.rs  
- Adapter: OpenalexAdapter registered in global REGISTRY

## Data policy
- Auth class: api_key
- Rate limit: 1 request per 1000ms (conservative local cap)
- License: OpenAlex dataset CC0; API service use subject to OpenAlex terms; underlying article content retains article-level rights
- Data class: 

## Evidence
- Fixture: fixtures/connector_openalex_*.json
