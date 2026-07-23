# Science format-converter supply-chain audit

Seam contract: **S2**. Status: **not approved for runtime integration**.

This audit covers the deferred PDF, DOCX, XLSX, PPTX, and Notebook preview
batch. It is an approval record, not an implementation approval: the current
Rust preview path intentionally remains limited to its audited v1 formats.

## Findings on 2026-07-23

| Candidate | Observed evidence | Decision |
|---|---|---|
| MarkItDown | Local reference material declares MIT and includes the Microsoft MIT notice. Its own guidance requires Python 3.10+ and optional format extras. | Not approved: no pinned release/source revision, lock file, or complete transitive dependency license inventory was supplied. |
| `pdftotext` | A machine-local executable is present. | Not approved: the system package/version/license provenance is not fixed by this repository and would make preview output host-dependent. |
| `soffice` | A bundled-runtime executable is present. | Not approved: the runtime package identity, license notices, feature set, and sandbox contract are not committed here. |
| Notebook | An `.ipynb` file is JSON, but rendering outputs or executing cells would create a separate S4 execution/provenance boundary. | Not an S2 converter; defer execution to a separately approved S4 design. |

## Non-negotiable admission requirements

Before any deferred format is added to `xai-grok-science::preview`, the change
must include all of the following:

1. a pinned converter release or committed source revision and its exact
   license/NOTICE text;
2. a machine-readable, reviewed inventory for every runtime dependency used by
   the selected format extras;
3. a repository-owned invocation boundary with deterministic version capture,
   no ambient credential access, bounded input/output/time, and no network;
4. hostile-input tests (zip bombs, path traversal, macros/embedded links, and
   malformed documents) plus a byte-hash-bound artifact/evidence result;
5. a provenance file naming the converter, version, invocation contract, and
   redistribution obligations; and
6. a new L4 product-path test through the sole Rust `SessionActor`.

No MarkItDown code, Python environment, system converter, Office runtime, or
Notebook executor is copied, installed, or invoked by the current Science
runtime. This preserves the existing zero-network default and prevents an
unreviewed converter from becoming a second execution authority.
