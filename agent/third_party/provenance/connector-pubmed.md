# pubmed (pubmed) — provenance record

**Milestone:** DS-auto  
**Date:** 2026-07-24  

## Upstream source
- Source: synsci-openscience (Apache-2.0)
- SHA-256: 7b89da33bf89be634c94c795af07b21dfcf61273200e960e54c39c0cb009ae04

## Service identity
- Service owner: National Center for Biotechnology Information (NCBI/NIH)
- Official base URL: https://eutils.ncbi.nlm.nih.gov/entrez/eutils/
- Egress hosts: eutils.ncbi.nlm.nih.gov
- TOS: https://www.ncbi.nlm.nih.gov/home/about/policies/

## Rust Lumen adaptation
- Module: connectors/pubmed.rs  
- Adapter: PubmedAdapter registered in global REGISTRY

## Data policy
- Auth class: none
- Rate limit: 3 requests per 1000ms
- License: NCBI policies; PubMed abstracts may be copyrighted; NCBI does not hold copyright to abstracts
- Data class: 

## Evidence
- Fixture: fixtures/connector_pubmed_*.json
