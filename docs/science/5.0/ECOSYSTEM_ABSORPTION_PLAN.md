# Lumen Science ecosystem absorption plan

**Status:** Phase 0 source/license gate and Phase 1 local SCP/Biomni capability
catalog implemented; capability admission continues

**Date:** 2026-07-27

This plan covers the exact current default-branch sources selected for
absorption:

| Source | Exact commit | Root license | Current Lumen disposition |
|---|---|---|---|
| `snap-stanford/Biomni` | `400c1f366b96a35ca253e13c9b06c5076af41d65` | Apache-2.0 | 224 typed tools plus 273 resource/knowledge records locally cataloged; native adapters pending |
| `jvogan/motif` | `876a4f9e5d99af1bc3cf5caa639ce8f5402dfbe0` | MIT | renderer vendored; first FASTA/metrics/reverse-complement/translation slice adapted into actor-gated Rust |
| `aipoch/open-science` | `fd2853f0b9bdb6c063ccc1e741687584ab94bf9a` | Apache-2.0 | 1,082 carried files; 228-file upstream delta under review |
| `qzzqzzb/OpenClaudeScience` | `4a5f2ab2879ebd4f806155c796e247da94bb1625` | MIT at root, mixed nested licenses | MIT components and catalog may be adapted; four proprietary skill trees rejected |
| `InternScience/scp` (transitive catalog source) | `cea5398564032aea65a78e246d06c30ae945e03f` | MIT | all 207 skill documents enter quarantine/catalog analysis before tool admission |

The machine-readable authority for these facts is
`ecosystem-admission.lock.json`. Verify the local Lumen history and ledgers:

```bash
python3 scripts/verify-ecosystem-admission.py
python3 scripts/test-ecosystem-admission.py
```

An auditor with the five exact local checkouts can additionally prove their Git
heads, clean state, licenses, nested license exceptions, and source file hashes
without network access:

```bash
python3 scripts/verify-ecosystem-admission.py \
  --source-root snap-stanford-biomni=/path/to/Biomni \
  --source-root jvogan-motif=/path/to/motif \
  --source-root aipoch-open-science=/path/to/open-science \
  --source-root qzzqzzb-openclaudescience=/path/to/OpenClaudeScience \
  --source-root internscience-scp-skills=/path/to/scp
```

## What “absorb everything useful” means

An upstream file is not automatically a Lumen capability. Every component gets
one explicit disposition:

1. **Adopt** — permissively licensed, deterministic mechanics can be vendored
   with exact provenance and focused tests.
2. **Adapt** — keep the useful behavior but route decisions, writes, network,
   execution, approval, artifacts, and terminal state through Rust
   `SessionActor`.
3. **Quarantine** — keep a local, hash-addressed candidate and its metadata, but
   do not expose it as an approved runtime skill.
4. **Reject authority/data trust** — do not import a peer agent runtime,
   arbitrary executor, unpinned downloader, or unverified scientific data as a
   product authority.
5. **Reject license / clean-room reimplement** — never copy or derive from
   restricted material. Independently implement the useful product capability
   from open standards and Lumen-owned requirements.

This retains one product invariant:

```text
Rust SessionActor
  = execution + permission + terminal state + artifacts + evidence
  + provenance + replay authority
```

Python, TypeScript, browser renderers, MCP/SCP endpoints, knowledge documents,
and external binaries may be driven adapters. None may decide or attest that a
scientific operation succeeded.

## Proof that the earlier work was not lost

The ecosystem gate requires twelve cumulative foundation commits to remain
ancestors of the current checkout, including:

- the original LS5-K39 baseline;
- `seq_analyze` SessionActor closure;
- per-session permission gates;
- verified Rust ACP artifact serving;
- project migration, kernel admission, and review-record authority;
- the selective Lumen v0.1.251 safety admission.

It also verifies live source markers for the Begin/Finish commands, handle,
actor route, four ACP methods, and built-binary negative tests. Finally it
checks that these carried inventories have not regressed:

- Open Science: at least 1,082 shipped files, with the previous ledger still
  reporting zero unadopted source files at its pinned baseline;
- Science skills: at least 27 total and 10 approved, all with no independent
  execution authority;
- connectors: exactly the existing 42 final dispositions;
- Motif: the same exact vendored commit with `runtime_authority: none`.

This is stronger than checking that old commits merely exist: deletion of the
current authority routes, markers, or carried ledgers also fails the gate.

## Source-specific decisions

### Biomni

Biomni contributes breadth:

- 224 declared tools across 22 subject modules;
- 40 database-tool descriptions;
- 76 academic data-lake entries;
- 113 software/library catalog entries;
- 82 local protocol documents;
- a four-class discovery model: tools, data, software, and know-how.

The useful parts are its taxonomy, descriptor vocabulary, resource discovery,
know-how metadata, and the list of database/API candidates. They become
Lumen-owned catalogs and individually admitted connectors.

The following never enter the product as written:

- `A1`/LangGraph/ReAct/FastMCP as a peer runtime;
- model-generated direct Python/R/Bash and dynamic `exec`;
- arbitrary URL/MCP calls, mutable Git installs, unpinned binaries, pickle
  registries, or automatic 11 GB downloads;
- protocol or data-lake content without per-resource licenses and digests;
- device/lab execution outside typed device gates.

High-value connector candidates not already covered by Lumen include
ClinicalTrials.gov, QuickGO, ENCODE, GWAS Catalog, PRIDE, EMDB, Monarch,
cBioPortal, openFDA, DailyMed, JASPAR, ReMap, SCREEN cCRE, UniChem, Synapse, and
RegulomeDB. Each must receive the normal descriptor/parser/fixture/registry/
SessionActor/artifact/evidence/replay product slice; a Python function name is
not connector proof.

### Motif

The current Lumen vendor pin is already Motif's latest `main`, so changing the
base did not discard or stale the integration.

Direct candidates are deterministic parsers and algorithms for FASTA, GenBank,
ABI, translation, ORF detection, restriction analysis, primer/PCR design,
Gibson/Golden Gate assembly, local MSA, sequence difference, plasmid layout,
and SVG serialization. Typed inert analysis-result schemas and dependency
validation are also valuable.

The workbench remains a renderer over verified artifacts. Its MCP server,
installer, page globals, browser download receipts, in-memory workspace, and
Node/PATH external-tool discovery are not product authorities.

Its bundled biological data is not knowledge truth. The named pUC19 and pBR322
sequence lengths conflict with authoritative references, and the enzyme array
contains 154 entries despite a “200+” comment. Such records remain UI/test
fixtures until accession-bound, byte-compared, licensed, and reviewed.

### Open Science

The previous absorption is retained. The new `d8f11e..fd2853` delta contains
228 files, with the highest-value new work concentrated in:

- bounded skill archive inspection;
- conversation-bound attachment ownership and cancellation;
- explicit candidate preview and user approval;
- bounded GitHub skill preview/download;
- upload repository and file-reference confinement;
- workspace/skill search and draft-preservation UX.

These mechanics should be adopted with Apache change notices. Upstream ACP,
agent-framework, approval-broker, notebook, and compute decision paths must not
replace the Rust actor or the one Rust ACP transport.

### OpenClaudeScience and SCP skills

Useful MIT components include the science catalog/search UX, Git-native
knowledge-base concepts, and a separately MIT-licensed patent workflow after
it is decomposed into controlled tools.

The science catalog is generated from a pinned `InternScience/scp` source with
207 skill documents. Lumen will preserve every skill as a local,
hash-addressed candidate and extract:

- stable identity and source SHA;
- discipline and trigger description;
- required upstream tools and endpoints;
- candidate mappings to existing Lumen connectors;
- license/data-risk/safety annotations;
- prompt-injection audit status;
- final approved, pending, or rejected disposition.

Embedded Python examples and SCP-Hub endpoints are reference material only.
They do not become executable skills, credentials, or remote authority.

Four nested OpenClaudeScience directories — `skills/docx`, `skills/pdf`,
`skills/pptx`, and `skills/xlsx` — carry a separate Anthropic proprietary
license that forbids copying, retaining, deriving, and redistribution. The root
MIT license does not override it. Lumen will independently implement the
functional requirements using open formats and Lumen-owned code; no source
text, prompt, script, or asset from those directories will be copied or used as
a derivative basis.

## Delivery order

### Phase 0 — source, license, and no-regression authority gate

Implemented in this slice:

- exact source commits and license hashes;
- nested-license exception handling;
- component adopted/adapted/quarantined/rejected matrix;
- protected cumulative Science history and source-marker verification;
- negative tamper tests;
- optional five-checkout offline proof.

### Phase 1 — local ecosystem catalog

Implemented:

- vendored the permissively licensed SCP skill documents and required license
  into a non-runtime, hash-addressed source area;
- generated a Lumen catalog for all 207 skills;
- wired the catalog into the desktop `skills:list` inventory in both developer
  and packaged-resource layouts;
- exposed only compact search metadata — never upstream endpoints, example
  clients, runtime permissions, or executable source;
- replaced the raw-JSON-only Skills view with a local searchable catalog that
  shows discipline, upstream tool-reference count, candidate Lumen routes, and
  the explicit quarantine state;
- extracted 528 upstream tool references;
- kept all 207 skills quarantined and zero approved;
- flagged 186 documents containing executable client examples;
- redacted credential-shaped values from 17 files while preserving both source
  and local hashes;
- marked the one empty upstream description and generated its catalog fallback
  from the document title;
- added one-to-one source/catalog/hash and secret-tamper gates;
- added a desktop fail-closed test proving that a catalog entry cannot be
  relabelled as approved and still appear in the product;
- preserved Biomni's 22 Apache-2.0 literal tool-description modules without
  importing or executing Python;
- generated all 224 Biomni tool candidates, including their typed parameter
  contracts, source hashes, risk flags, and Lumen-native admission tracks;
- mapped 20 descriptions to existing Lumen connectors and identified 21
  explicit new-connector candidates;
- flagged 119 filesystem-facing, 63 network/download, 31 caller-selected
  endpoint, 13 code/command, and 9 physical/wet-lab candidates for stronger
  gates;
- kept all 224 Biomni candidates quarantined with zero controlled tools and
  network/shell/filesystem/device access denied;
- added one-to-one descriptor/source/hash checks and a negative test proving
  that matching manifest hashes cannot turn an inert descriptor into
  executable Python;
- cataloged all 76 Biomni data references with the licenses/access restrictions
  reported by Biomni and whether they remain in its commercial-mode subset;
- cataloged all 113 software identities while explicitly retaining unknown
  version, repository, and license fields instead of implying installation;
- cataloged all 82 protocol references with source hashes but did not copy
  their Addgene/Thermo Fisher bodies because publisher licenses are unresolved;
- preserved the two know-how documents that explicitly declare CC BY 4.0,
  including their source/vendored hashes, while keeping their instructions and
  executable examples quarantined;
- combined the 207 SCP documents, 224 Biomni tool descriptions, and 273 Biomni
  resources into one searchable 704-item desktop capability view.

Remaining:

- admit controlled adapters individually; a catalog mapping is not execution
  proof.

### Phase 2 — secure skill import delta

Implemented in the first preview slice:

- adapted Open Science's shared YAML frontmatter parser so identity fields and
  arbitrary flat string metadata use one parser in main and renderer;
- preserved imported metadata without allowing it to override authoritative
  `name` or `description`;
- added a 4 MiB cumulative preview-content budget independent of the larger
  import budget;
- unified shallowest Skill-root selection with the upstream bundle-path
  contract;
- added read-only candidate previews for local Markdown, ZIP/Skill bundles,
  and scanned GitHub Skills;
- made GitHub preview lazy and bounded: only root `SKILL.md` is downloaded,
  while asset names remain relative metadata;
- disabled network-fetching media elements for untrusted preview Markdown;
- kept preview separate from selection, import, approval, and execution.
- directly ported the 860-line streaming ZIP classifier and its 707-line,
  28-case adversarial parity suite from the exact `fd2853` source;
- kept that classifier source-only until a SessionActor-owned attachment
  import route exists; the stubbed desktop ACP runtime remains unchanged.

Remaining:

- connect the bounded archive classifier to a Rust-authorized attachment
  import route without restoring the rejected desktop ACP runtime;
- Bind imports to current session/turn/attachment bytes and SHA-256.
- Persist quarantine records through the Lumen authority path.
- Add negative fixtures for ZIP traversal, decompression bombs, nested
  archives, stale approvals, replacement-target swaps, and cross-session reuse.

### Phase 3 — deterministic scientific mechanics

Implemented in the first four Rust algorithm slices:

- directly adapted Motif FASTA parsing semantics: first-whitespace headers,
  PIR comments, ASCII-only sequence cleaning, and explicit gap-removal counts;
- adapted nucleotide composition, GC/AT fraction, Tm and molecular-weight
  calculations, plus average/monoisotopic protein mass tables;
- preserved Motif's complete DNA/RNA IUPAC reverse-complement behavior and
  fixed lowercase RNA normalization before standard-code translation;
- recorded the exact Motif repository, commit and license in both schema-v5
  `analysis.json` and durable provenance, with component paths in the analysis;
- executed the exact upstream TypeScript locally and matched the Rust fixture
  results for parsing, all numeric metrics, IUPAC complements, and translation;
- retained the existing SessionActor allow/deny/timeout/cancel and
  owner/project/call artifact boundaries;
- rebuilt the current `lumen` binary and passed all three filtered
  `seq_analyze` product tests, including reopening the store-owned
  `analysis.json` to verify schema 4, Motif commit, composition output, table
  2's `AGA`-terminated 30-aa ORF, and durable table context/provenance;
- directly adapted Motif's standard-table ORF scanner: `TTG`/`CTG`/`ATG`
  initiators, nested starts, implicit terminal ORFs, six-frame scanning,
  reverse-strand coordinate mapping, and length ordering now land in
  schema-v5 output with an explicit 50-record Lumen cap;
- executed the exact Motif ORF source locally and matched Rust on nested,
  terminal, and reverse-strand fixtures;
- directly adapted all 24 single-valued NCBI translation tables Motif ships,
  including exact names, initiators, stops, and codon overrides; tables 27, 28
  and 31 remain explicitly unsupported because context-dependent sense/stop
  semantics cannot be represented honestly by one codon map;
- froze `translationTableId` in the durable actor run before approval, refused
  unsupported IDs before run creation, and made any post-Allow option swap
  fail terminally without artifacts, evidence, or provenance;
- executed the exact Motif registry, translation, and table-aware ORF sources
  locally and matched all IDs/names plus tables 2, 15, and 32 fixtures.
- directly adapted Motif's complete 30-enzyme default restriction panel and
  scanner, including overlapping recognition, both strands, mirrored Type IIS
  cuts and circular-origin hits, with an explicit 100-hit Lumen cap;
- froze `topology` into the durable run before approval and made a post-Allow
  linear/circular swap fail terminally without artifacts, evidence or
  provenance;
- executed the exact upstream TypeScript locally and matched the full catalog
  fingerprint, reverse BsaI, circular EcoRI and IUPAC fixtures.

Remaining:

- admit additional Motif parsers/algorithms in small, attributed slices;
- Use independent authoritative fixtures for named biological data.
- Return all derived output as store-owned, hash-addressed artifacts.
- Add typed analysis envelopes and dependency/cross-reference validation.

### Phase 4 — knowledge and connector expansion

- Build a read-only, versioned, license-aware knowledge registry.
- Import Biomni know-how only after citation and underlying-source review.
- Admit new API connectors one by one using offline fixtures first.
- Treat protocol text as cited evidence, never as direct wet-lab commands.

### Phase 5 — clean-room Office/PDF capability

- Specify document ingest, preview, validation, edit-intent, export, and
  round-trip contracts from open file-format standards.
- Reuse already admitted Lumen/Open Science preview and save-dialog seams.
- Keep parsing/rendering workers confined and content-addressed.
- Prove that deny/timeout/cancel produces no authoritative output.

## Evidence boundary

Phase 0, the Phase 1 SCP/Biomni catalogs, the first Phase 2 read-only Skill
preview/classifier slices, and the first Phase 3 Motif-to-Rust algorithm slice
currently have source audit and focused test evidence. The 704 candidate
capabilities are local and searchable in the desktop Skills tab, but none of
the new 704 entries is executable or product-approved. The streaming archive
classifier is a tested source module, not a product route; SessionActor-bound
attachment import remains open. Motif sequence metrics do execute only through
the already actor-gated `seq_analyze` path and produce store-owned hashed
artifacts. A fresh current-source binary passed the three filtered
`seq_analyze` allow/boundary/deny product tests for the schema-v4 table-aware
route; the allow case selected table 2 and the negative cases produced no
unauthorized output. A fresh schema-v5 run passed the same three product tests
with circular restriction scanning: the allow case verified the 30-enzyme
panel and origin-spanning EcoRI cut, while invalid topology and deny produced
no unauthorized output. This does not claim that the 224 Biomni tools,
remaining connectors/Motif algorithms, or clean-room document features are
already product-complete.

For each later slice, report separately:

- source/check;
- focused offline tests and exact counts;
- rebuilt-binary/product proof;
- CI;
- live/provider proof;
- release;
- deployment.

No live/provider/billable invocation is part of ecosystem admission.
