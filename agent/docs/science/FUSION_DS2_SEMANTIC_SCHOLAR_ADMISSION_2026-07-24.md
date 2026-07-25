# Lumen Science DS-2 — Semantic Scholar admission record

**Checkout:** /Users/lei/Documents/Codex/2026-07-24/ji-3/work/lumen-science-fusion
**Branch:** codex/science-fusion-full
**Decision:** offline fixture product path accepted; live pending-deny

## Native Rust product slice

No Semantic Scholar runtime or source code was imported. Rust Lumen remains
the only permission/execution/artifact/evidence/provenance/replay/audit authority.

- Descriptor: SEMANTIC_SCHOLAR (connectors.rs)
- Egress: api.semanticscholar.org only
- Rate limit: 1 req/3000ms (local conservative policy; official unauthenticated pool is 1000 req/s shared, introductory API key 1 req/s)
- Selected fields: paperId, title, url, year, venue, externalIds
- Excluded fields: abstract, citationCount, authors, references, citations
- Exchange count: 1
- Max results: 50

## Official-source review

- https://api.semanticscholar.org/api-docs/
- S2 data ODC-BY; API TOS at docs URL

## Offline evidence

- cargo check: passed
- Unit tests: search_path selects safe fields, parse_search reads bibliographic fields, fails closed on malformed records, live probe ignored
- Fixture: connectors/connector_semantic_scholar_search.json

## Negative tests

- malformed JSON: fail closed
- missing paperId: fail closed
- empty title: fail closed
- missing total: fail closed
- missing data array: fail closed
- excluded-field sentinel: parser ignores (proven by fixture containing abstract/author/citationCount fields that are not parsed)

## Live admission remains closed

- Requires explicit network authorization
- Optional API key for identified quota (not higher throughput)
