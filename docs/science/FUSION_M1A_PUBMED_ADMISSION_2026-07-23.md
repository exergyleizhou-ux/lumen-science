# Lumen Science M1A — PubMed admission record

**Checkout:** `/Users/lei/code/lumen`, `main@8bd51b51ff874ecf035f52398898c2fbd40e9390`  
**Decision:** offline fixture product path is accepted; live PubMed remains
**pending-deny**.

## What was reviewed

The implementation is native Rust Lumen, not imported upstream runtime code:

- `agent/crates/codegen/xai-grok-science/src/connectors/pubmed.rs` defines the
  ESearch/ESummary protocol adapter and fixture parser.
- `agent/crates/codegen/xai-grok-science/src/connectors.rs` constrains PubMed
  to HTTPS `eutils.ncbi.nlm.nih.gov`, a public-reference classification, and
  three requests per second.
- `agent/crates/codegen/xai-grok-shell/src/extensions/science.rs` requires an
  in-workspace fixture for every exchange and sends the operation through the
  existing SessionActor permission/durable-evidence path.

## M1A evidence

The M0 G5 built-binary run executed the PubMed two-exchange product path using
`connector_pubmed_esearch.json` and `connector_pubmed_esummary.json`. The
seven science product tests passed, including
`test_stdio_science_connector_fetch_product_path`; see
`agent/work/m0-evidence/g5-built-binary-science-e2e.log`.

No live network or provider call was made for M1A.

## Current NCBI terms review

The current NCBI policy requires E-utilities clients not to exceed three
requests per second, to use the E-utilities host, and to make its disclaimer
and copyright notice evident to product users. Its programming manual says
that a distributed tool and developer email must be registered with NCBI for
the corresponding registered identity.

The local rate and host controls satisfy only the first two requirements.
There is no evidence of an approved NCBI registration. A mandatory NCBI
disclaimer/copyright notice is now returned in the connector product response
as `user_notice` and is covered by the built-binary connector e2e. A rendered
TUI/WebView display of that response remains a later UI-layer acceptance gate.
Therefore M1A does **not** approve a live PubMed call. The source lock records
this as `pending-deny-live; offline-fixture-only`.

## Required explicit follow-up before live admission

1. Obtain and record the operator-approved, registered NCBI `tool` and
   developer `email` identity without committing personal data or credentials.
2. Add a user-visible NCBI disclaimer/copyright notice at the actual TUI
   presentation point and test it.
3. Receive explicit authorization for a rate-limited live probe, then retain
   its redacted evidence separately from the offline fixture proof.
