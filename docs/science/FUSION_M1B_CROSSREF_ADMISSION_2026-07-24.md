# Lumen Science M1B — Crossref admission record

**Checkout:** `/Users/lei/Documents/Codex/2026-07-24/ji-3/work/lumen-science-fusion`

**Branch:** `codex/science-fusion-full`, based on
`main@8bd51b51ff874ecf035f52398898c2fbd40e9390`

**Decision:** offline fixture product path accepted; live Crossref remains
**pending-deny**.

## Native Rust product slice

No Crossref runtime code was imported. Rust Lumen remains the sole permission,
execution, artifact, evidence, provenance, replay, and audit authority.

- `xai-grok-science/src/connectors/crossref.rs` constructs one bounded Works
  query and parses the response fail-closed.
- The descriptor permits only HTTPS `api.crossref.org`, one list request per
  second, one exchange per run, at most 50 results.
- The query selects only `DOI,title,container-title`. It neither selects nor
  consumes `abstract`, `link`, or full-text fields.
- `xai-grok-shell/src/extensions/science.rs` routes the fixture exchange
  through the existing SessionActor permission and durable fetch protocol.
- The product response carries a mandatory service-specific `user_notice`.

## Current official-source review

Reviewed 2026-07-24:

- https://www.crossref.org/documentation/retrieve-metadata/rest-api/
- https://www.crossref.org/documentation/retrieve-metadata/
- https://community.crossref.org/t/refining-rest-api-limits-for-improved-stability-and-reliability/16137

Crossref documents public REST access without sign-up and recommends a contact
email for the polite pool. Its current list-query public-pool limit is one
request per second with one concurrent request. Bibliographic facts are
generally not subject to copyright and Crossref-generated metadata is CC0;
abstract rights remain with publishers or authors.

The ignored live probe therefore requires a runtime-only
`CROSSREF_MAILTO`. It hashes the complete request URL for evidence and never
prints or persists the email or URL.

## Offline evidence

No Crossref, provider, or other live endpoint was called.

- Science test binary: 67 tests discovered; 64 passed, 0 failed, 3 explicit
  live probes ignored.
- Strict Science clippy: passed with `-D warnings`.
- Fresh `xai-grok-pager-bin --bin lumen` build: passed in an isolated target.
- Rebuilt-binary ACP connector test: `running 1 test`; 1 passed, 0 failed. The
  one test executes PubMed, ChEMBL, and Crossref fixture cases, checks
  artifacts, notice, redacted audits, citation evidence, provenance, and
  same-store reopen.
- Built binary SHA-256:
  `7d2a2cfaedbc5e2886a3f03fcae058ca3ce5539f2c35ae6862c050a171bccfce`.

Durable logs and hashes are under
`agent/work/crossref-p3-evidence/`.

The first test-profile build attempt exhausted disk while compiling and never
started the test. Only this task's regenerable incremental cache was removed;
the retry used `CARGO_INCREMENTAL=0`, reached `running 1 test`, and passed.

## Live admission remains closed

Live proof requires all of:

1. explicit authorization for a Crossref network request;
2. an operator-approved contact email supplied only as `CROSSREF_MAILTO`;
3. retention of redacted response/status/rate-limit evidence;
4. a visible presentation-point test for the returned notice.

Until then, the accepted evidence level is offline fixture L4, not live L5.
