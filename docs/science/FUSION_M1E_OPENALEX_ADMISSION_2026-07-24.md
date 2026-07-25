# Lumen Science M1E — OpenAlex admission record

**Checkout:** `/Users/lei/Documents/Codex/2026-07-24/ji-3/work/lumen-science-fusion`

**Branch:** `codex/science-fusion-full`, based on
`main@8bd51b51ff874ecf035f52398898c2fbd40e9390`

**Decision:** offline fixture product path accepted; live OpenAlex remains
**pending-deny**.

## Native Rust product slice

No OpenAlex runtime or source code is imported. Rust Lumen remains the only
permission, execution, artifact, evidence, provenance, replay, and audit
authority.

- The descriptor permits only HTTPS `api.openalex.org`.
- One `/works` search exchange is bounded to 50 records and a conservative
  local cap of one request per second.
- The request uses `select=id,doi,display_name,publication_year`.
- Abstracts, full text, authorships, locations, references, topics, and content
  URLs are neither selected nor parsed. The fixture includes sentinels for
  excluded fields to prove the parser ignores them.
- Invalid work IDs, missing titles, invalid years, missing hit counts, and
  malformed result arrays fail closed before artifact registration.
- The offline product route uses explicit fixture-only policy validation. It
  opens no socket and neither requires nor accepts a credential in its request.
- The ignored live probe requires runtime-only `OPENALEX_API_KEY`. The key is
  attached only to the ephemeral HTTP request and never enters
  `ValidatedRequest`, artifacts, evidence, provenance, or audit records.
- The SessionActor/ACP product response carries a mandatory CC0/API-terms and
  article-rights notice.

## Official-source review

Reviewed 2026-07-24:

- https://developers.openalex.org/
- https://developers.openalex.org/guides/authentication
- https://developers.openalex.org/guides/searching
- https://developers.openalex.org/guides/page-through-results
- https://developers.openalex.org/guides/selecting-fields
- https://developers.openalex.org/api-reference/authentication
- https://developers.openalex.org/api-reference/works/list-works
- https://openalex.org/OpenAlex_termsofservice.pdf

The current API uses `https://api.openalex.org`, requires a free API key for
normal use, meters search calls, returns a daily free budget, and reports rate
and cost information in headers and response metadata. The documented hard
service limit is 100 requests per second; this connector applies a stricter
local one-request-per-second cap. `per_page` permits up to 100, while this
connector permits at most 50 and performs only one page.

OpenAlex search considers title, abstract, and full text when matching. That
server-side search behavior does not grant content reuse rights. The `select`
parameter limits the response to four bibliographic fields, so this connector
does not retrieve the matched abstracts or full text. OpenAlex describes its
dataset as CC0, while API access remains subject to its service terms and
underlying article content retains article-level rights.

## Offline evidence

No OpenAlex, provider, or other live endpoint was called.

- Science: 82 tests discovered; 76 passed, 0 failed, 6 live probes ignored.
- OpenAlex adapter, fixture, credential boundary, fail-closed, notice,
  artifact/evidence/provenance, redacted audit, and reopen replay tests passed.
- Strict Science clippy passed with `-D warnings`.
- The shell built-binary test target compiled successfully before the product
  source commit.
- Product source commit:
  `791413c28338458472eb98502628f1307eaa58aa`.
- Fresh exact-product-source `xai-grok-pager-bin --bin lumen` build passed in
  8m16s with Cargo offline and `CARGO_INCREMENTAL=0`.
- Rebuilt-binary ACP connector test: 1 passed, 0 failed. It executed PubMed,
  ChEMBL, Crossref, UniProt, Europe PMC, and OpenAlex fixture cases and checked
  artifacts, notices, redacted audits, citation evidence, provenance, and
  same-store reopen.
- Built binary SHA-256:
  `7986919860879b7c4f63aa990fe237f7b34b48e1fac7dd978361d75114fda209`.

Evidence is in `agent/work/openalex-p1-evidence/`. The three `.summary` files
are explicitly concise summaries of the current tool output, not complete raw
compiler logs. The built-binary test log retains the exact test result.

## Live admission remains closed

Live proof requires explicit billable-network authorization, an
operator-approved runtime-only API key, retained redacted rate-limit/cost/status
and response evidence, and an actual presentation-point test for the mandatory
notice. Any article-content reuse also requires checking the article-level
license. Until then, this is accepted deterministic offline L4 evidence, not
live L5.
