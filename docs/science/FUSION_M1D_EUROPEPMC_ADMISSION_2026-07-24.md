# Lumen Science M1D — Europe PMC admission record

**Checkout:** `/Users/lei/Documents/Codex/2026-07-24/ji-3/work/lumen-science-fusion`

**Branch:** `codex/science-fusion-full`, based on
`main@8bd51b51ff874ecf035f52398898c2fbd40e9390`

**Decision:** offline fixture product path accepted; live Europe PMC remains
**pending-deny**.

## Native Rust product slice

No Europe PMC runtime or source code was imported. Rust Lumen remains the only
permission, execution, artifact, evidence, provenance, replay, and audit
authority.

- The descriptor permits only HTTPS `www.ebi.ac.uk`.
- One search exchange is bounded to 50 records and a conservative local cap of
  one request per second. This is not represented as an official Europe PMC
  service quota because no fixed quota was found in the reviewed official
  documentation.
- The query explicitly requests JSON `resultType=lite`, disables synonym
  expansion for deterministic query semantics, and parses only source, stable
  ID, title, journal title, and publication year.
- Abstracts, full text, references, annotations, and external links are neither
  requested nor parsed. The fixture includes an `abstractText` sentinel to
  prove the parser ignores it.
- Invalid source/ID characters, missing titles, missing hit counts, and
  malformed result lists fail closed before artifact registration.
- The SessionActor/ACP product response carries a mandatory notice that
  article content has article-specific copyright and license terms.

## Official-source review

Reviewed 2026-07-24:

- https://europepmc.org/RestfulWebService
- https://europepmc.org/developers
- https://europepmc.org/Copyright

Europe PMC documents the production Articles REST search endpoint, JSON output,
`idlist`/`lite`/`core` result types, cursor pagination, and a maximum
`pageSize` of 1000. This connector uses a stricter local maximum of 50 and only
the `lite` response. Europe PMC's copyright guidance says article material
usually has an explicit copyright statement, and that automated retrieval must
use approved API/OAI/bulk channels; licenses are not identical across articles.

## Offline evidence

No Europe PMC, provider, or other live endpoint was called.

- Science: 77 tests discovered; 72 passed, 0 failed, 5 live probes ignored.
- Europe PMC adapter, fixture, fail-closed, notice, artifact/evidence/
  provenance, audit, and reopen replay tests passed.
- Strict Science clippy: passed with `-D warnings`.
- Fresh exact-HEAD `xai-grok-pager-bin --bin lumen` build: passed in 12m35s
  with `CARGO_INCREMENTAL=0`, after commit `f390ca91`.
- Rebuilt-binary ACP connector test: `running 1 test`; 1 passed, 0 failed. It
  executed PubMed, ChEMBL, Crossref, UniProt, and Europe PMC fixture cases and
  checked artifacts, notices, redacted audits, citation evidence, provenance,
  and same-store reopen.
- Built binary SHA-256:
  `8171b076017c77246b8378a444e7fd4ac76a1a708ea6a067529e7feb4363b5d7`.

Evidence logs and hashes are in `agent/work/europepmc-p1-evidence/`.
`exact-head-lumen-build.log` and
`exact-head-built-binary-connector-e2e.log` are the authoritative committed
source-state evidence. The earlier unprefixed build and ACP logs are retained
as preliminary evidence and are superseded because they preceded removal of
two unrelated rustfmt-only working-tree diffs.

## Live admission remains closed

Live proof requires explicit network authorization, retained redacted
status/response evidence, and an actual presentation-point test for the
mandatory notice. Any article-content reuse also requires checking the
article-level license. Until then, this is deterministic offline evidence, not
live L5.
