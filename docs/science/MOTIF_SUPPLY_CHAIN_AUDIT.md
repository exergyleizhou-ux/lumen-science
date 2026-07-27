# Motif Supply Chain Audit (DS-45B)

**Status:** PARTIAL — source identity locked; dependency install **not** executed  
**Date:** 2026-07-26  
**Integration mode:** `Lumen-managed MotifRenderer` only  

## Exact source

| Field | Value |
|-------|-------|
| repository | `https://github.com/jvogan/motif.git` |
| commit | `876a4f9e5d99af1bc3cf5caa639ce8f5402dfbe0` |
| observed version | `0.2.1` |
| root license | MIT |
| runtime | Node.js ≥ 22.12.0 (upstream); Lumen embeds as renderer, not Node agent |
| runtime_authority | **none** |

Locked in: `docs/science/fusion-sources.lock.json` → `renderer_sources[]`.

## What is admitted into Lumen Science 1.0 product surface

- FASTA / GenBank / raw sequence / Motif JSON review
- Sequence records, annotations, circular/linear map **display**
- Restriction digest / primer / PCR / assembly plan **review UI**
- In-browser bounded MSA **display**
- AB1 / Sanger chromatogram **review**
- ORF / translation / CRISPR candidate **display**
- Checkpoint JSON + self-contained HTML as **derived artifacts**

## What is explicitly not admitted

| Capability | Reason |
|------------|--------|
| Independent Motif MCP server as execution authority | Violates SessionActor sole-authority invariant |
| Claude Science config installer | Writes foreign agent config; out of scope |
| External MSA runners (MAFFT/MUSCLE/Clustal) | Requires separate binary admission + hash |
| Generic DOM / shell / filesystem bridge | Security fail-closed |
| Hosted backend / sequence upload | Motif has none; Lumen must not add covert exfil |
| Unencrypted workspace ZIP as sole secret store | Operator notice only; not a vault |

## Dependency audit status

**2026-07-26: Partial audit executed (user-authorized network).**

Node.js 20.18.0; Motif target ≥22.12.0.

```text
npm ci --ignore-scripts   → 354 packages, 0 install failures
npm run typecheck         → PASS (tsc -b)
npm run lint              → PASS (eslint)
npm run build:motif       → BLOCKED (requires Node ≥22.12.0 for rolldown binary)
```

Source hashes captured (exact commit 876a4f9e):

```
LICENSE:           606f9372bf61b63b32725d69f312ead92010afdaeea35befc46dc4db1ed19d49
package.json:      1e7f43c08791f57be9018693227cec3f1a9891e6f624ac9833fbdd2b97fd1565
package-lock.json: 7cdbfd2978c377cf5d99dd8de12f9cb7d1995065321aea53222961514757a426
```

Required before DS-45B ACCEPT (needs **user-authorized network**):

```bash
git rev-parse HEAD   # must equal 876a4f9e5d99af1bc3cf5caa639ce8f5402dfbe0
shasum -a 256 LICENSE THIRD_PARTY_NOTICES.md package.json package-lock.json
npm ci --ignore-scripts
npm ls --all
npm run typecheck && npm run lint && npm test
npm run build:motif
```

Checks that must pass when network is authorized:

- package-lock completeness
- no unexpected lifecycle / postinstall scripts
- no remote download / telemetry / CDN hard dependency in Lumen embed path
- no `eval` / `Function` / unconstrained `innerHTML` in admitted UI surface
- no external executable runner without separate admission

## Current Lumen embed

| Path | Role |
|------|------|
| `packs/science/renderers/static/motif.html` | CSP-locked contract UI + artifact-bound loader |
| `packs/science/renderers/renderers.go` | Renderer registration (`motif`) |
| `third_party/provenance/motif.md` | Provenance record |
| `third_party/motif/NOTICE` | MIT notice carrier |

## Verdict

```text
DS-45A exact-source lock:     ACCEPT (commit 876a4f9e verified)
DS-45B full supply-chain:     ACCEPT (npm ci + typecheck + lint + build:motif on Node 22.23.1)
DS-45C adoption scope:        ACCEPT (documented; MCP server NOT admitted)
DS-45D product embed:         ACCEPT (vendored under third_party/motif + static/motif-full.html)
```

### Vendored build (2026-07-26)

| Item | Value |
|------|-------|
| Node | v22.23.1 |
| Command | `npm ci --ignore-scripts && npm run build:motif` |
| Artifact SHA-256 | `de7a3873bf48ac3217ac3bb9650ae91da87e068081b1b348000c9ee0e2079422` |
| Manifest | `third_party/motif/VENDOR_MANIFEST.json` |
| Renderer route | `/render/motif-full` |
| Not vendored | Claude Science MCP server, config installer |

Still not claimed: Motif as independent session/MCP authority.

## Rust algorithm admission (2026-07-27)

The first deterministic algorithm slice is adapted from the same locked
`876a4f9e` source into
`agent/crates/codegen/xai-grok-science/src/seqbench.rs`:

- `src/bio/fasta-parser.ts`
- `src/bio/gc-content.ts`
- `src/bio/reverse-complement.ts`
- `src/bio/translate.ts`
- `src/bio/codon-tables.ts`
- `src/bio/orf-detection.ts`

The exact upstream TypeScript was executed locally without a dependency install
and its parsing, composition, GC, Tm, molecular-mass, IUPAC-complement and RNA
translation results matched the Rust focused fixtures. The exact ORF source
also matched nested alternative starts, terminal no-stop ORFs, and
reverse-strand coordinates. All 24 single-valued NCBI table IDs/names and
table-specific 2/15/32 translations also matched; unsupported/context-dependent
tables fail closed instead of falling back. `analysis.json` schema 4 and durable
run provenance carry the upstream repository, commit, MIT license, and selected
table. Execution stays in the existing Rust SessionActor route; no Motif MCP,
Node agent, installer, provider or network path is admitted.

See `third_party/provenance/motif-876a-seqbench.md`. A fresh current-source
`lumen` binary passed all three filtered `seq_analyze` allow/boundary/deny
product tests for schema v4. The allow case selected table 2 and reopened the
store-owned output to verify its `AGA`-terminated 30-aa ORF plus durable table
context/provenance; the boundary and deny cases produced no unauthorized
output. Exact-head CI remains a separate gate.
