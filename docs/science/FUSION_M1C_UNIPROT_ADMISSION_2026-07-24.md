# Lumen Science M1C — UniProt admission record

**Checkout:** `/Users/lei/Documents/Codex/2026-07-24/ji-3/work/lumen-science-fusion`

**Branch:** `codex/science-fusion-full`, based on
`main@8bd51b51ff874ecf035f52398898c2fbd40e9390`

**Decision:** offline fixture product path accepted; live UniProt remains
**pending-deny**.

## Native Rust product slice

No UniProt runtime or source code was imported. Rust Lumen remains the only
permission, execution, artifact, evidence, provenance, replay, and audit
authority.

- The descriptor permits only HTTPS `rest.uniprot.org`.
- One search exchange is bounded to 50 records and a conservative local rate
  of one request per second. This is a local safety cap, not a claim that
  UniProt publishes that exact service limit.
- The query selects accession, entry ID, protein name, gene names, and organism
  name. It neither requests nor parses sequence, features, references, or
  citation text.
- Malformed accessions and records lacking stable names fail closed before
  artifact registration.
- The SessionActor/ACP product response carries a mandatory CC BY 4.0,
  attribution, no-warranty, and third-party-rights notice.

## Official-source review

Reviewed 2026-07-24:

- https://www.uniprot.org/help/api
- https://www.uniprot.org/help/api_queries
- https://www.uniprot.org/help/license

UniProt documents freely available REST APIs and JSON query results. It applies
CC BY 4.0 to copyrightable database content, disclaims correctness warranties,
and notes that some data may be covered by patents or other rights.

## Offline evidence

No UniProt, provider, or other live endpoint was called.

- Science: 72 tests discovered; 68 passed, 0 failed, 4 live probes ignored.
- Strict Science clippy: passed with `-D warnings`.
- Fresh `xai-grok-pager-bin --bin lumen` build: passed in 10m32s with
  `CARGO_INCREMENTAL=0`.
- Rebuilt-binary ACP connector test: `running 1 test`; 1 passed, 0 failed. It
  executes PubMed, ChEMBL, Crossref, and UniProt fixture cases and checks
  artifacts, notices, redacted audits, citation evidence, provenance, and
  same-store reopen.
- Built binary SHA-256:
  `0dddd7384945933f232dbad1b211bc9acb42ca8fe9442ddb23e91049888c5cc1`.

Evidence logs and hashes are in `agent/work/uniprot-p1-evidence/`.

## Live admission remains closed

Live proof requires explicit network authorization, retained redacted
status/response evidence, and an actual presentation-point test for the
mandatory notice. Until then, this is offline fixture L4, not live L5.
