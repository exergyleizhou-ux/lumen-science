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

```text
SOURCE_ACQUIRED_BUT_DEPENDENCIES_UNAVAILABLE
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
DS-45A exact-source lock:     ACCEPT
DS-45B full supply-chain:     BLOCKED_BY_EXTERNAL_AUTHORIZATION (npm ci network)
DS-45C adoption scope:        ACCEPT (documented)
DS-45D product embed:         PARTIAL (contract renderer live; full Motif bundle not vendored)
```

Do **not** claim “Motif fully integrated like Claude Science” until DS-45B ACCEPT and a vendored, hashed Motif build is served under the same CSP policy.
