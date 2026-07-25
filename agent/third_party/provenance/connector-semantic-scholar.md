# Semantic Scholar — provenance record

**Milestone:** DS-2
**Date:** 2026-07-24

## Upstream source

- Repository: synthetic-sciences/openscience (Apache-2.0)
- Commit: 083ef91ac29e7083c01aa97ae7db835dc87a6e94
- Source file: backend/cli/src/science/connectors/literature/semantic-scholar.ts
- SHA-256: 6cecd632a4a400bce418f7a73f9d8f0172202b37b48b2f603759b292fc95a56d

## Service identity

- Service owner: Allen Institute for AI (AI2)
- Official base URL: https://api.semanticscholar.org/graph/v1/
- Egress host: api.semanticscholar.org

## Rust Lumen adaptation

- Module: connectors/semantic_scholar.rs
- Descriptor: SEMANTIC_SCHOLAR (connectors.rs)
- Adapter: SemanticScholarAdapter (registered in global REGISTRY)
- No upstream source code imported

## License / terms

- S2 data: ODC-BY
- API TOS: https://api.semanticscholar.org/api-docs/

## Evidence

- Fixture: fixtures/connector_semantic_scholar_search.json
- Evidence: agent/work/semantic-scholar-evidence/.summary
