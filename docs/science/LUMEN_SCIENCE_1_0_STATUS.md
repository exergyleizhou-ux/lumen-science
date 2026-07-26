# Lumen Science 1.0.0 — Formal status

**Date:** 2026-07-26  
**Version:** `1.0.0`  
**Tag:** `v1.0.0`

## Verdict

```text
PRODUCT:     Lumen Science 1.0.0
STATUS:      FORMAL RELEASE (local-first scientific workbench)
ACCEPT:      Offline productivity, multi-platform release, CI, live probe subset,
             GPG-signed checksums, 5 controlled skills approved
NEVER:       5.0 embodied lab, autonomous scientist, medical certification
```

## Level gates

| Level | Verdict |
|-------|---------|
| L1 connectors | **ACCEPT** — 42/0 dispositions; 40 runtime; 2 rejected not fetchable |
| L2 offline loop | **ACCEPT** — seqbench, pipeline offline, Motif vendor, dogfood |
| L3 CI/release/live | **ACCEPT** — make release (5 platforms), SHA256SUMS + GPG, CI smoke/gates; live subset PASS |
| Skills | **ACCEPT partial** — 10 approved with controlled tools; 17 GPU/remote pending |
| 5.0 | **SPEC only** |

## Live matrix (authorized public probes)

| Connector / path | Result | Evidence |
|------------------|--------|----------|
| PubMed | PASS | `outputs/evidence/live-1.0.0/cargo-live-probes-final.log` |
| ChEMBL | PASS | same |
| Europe PMC | PASS | same |
| Crossref | PASS | `CROSSREF_MAILTO` set for probe |
| UniProt | PASS | schema drift fixed (totalResults optional) |
| arXiv | path fixed (`/api/query`); live 503 rate/upstream | logged |
| Semantic Scholar | FAIL (429) | logged |
| OpenAlex | FAIL (API key required) | logged |
| `lumen-science brief` ×4 topics | PASS | aspirin, BRCA2, CRISPR Cas9, metformin |

## Release

```bash
cd packs/science && make release
./scripts/sign-release.sh
# outputs/release/1.0.0/SHA256SUMS + SHA256SUMS.asc
```

## Install

```bash
./scripts/install-science.sh
lumen-science version   # 1.0.0
```

## Remaining non-blockers

- Optional Apple notarization / Windows Authenticode (org certs)  
- Remaining GPU/remote skills pending  
- OpenAlex/S2/arXiv live when keys or rate limits allow  

## Version / tag model (post-1.0.0)

```text
v1.0.0 (tag, GitHub Release)  = FROZEN 1.0 formal product — offline workbench
main (branch tip)             = 5.0 software preview (WP-2～8), NOT a formal release
                                 RealDevice=Disabled, all 5.0 paths are Preview gated
```

- `git checkout v1.0.0` to install the frozen 1.0 release.
- `main` carries the WP-2～8 software preview surface (project records, evidence queries,
  workflow validate+dry-run, kernel admission, multimodal index, review records,
  remote dry-run plan). These are **Preview** paths behind `FeatureGates`; no
  hardware, no 5.0 GA, no all-skills-approved — it is **not** a formal 5.0 release.
- Clarity: no silent tag lag. Anyone pulling main gets the preview tip; 1.0 users
  stay on the tag.

## Allowed claim

> Lumen Science 1.0.0 is a local-first, auditable scientific workbench with
> offline sequence analysis, multi-platform release builds with signed checksums,
> Motif-class review surfaces, honest connector admission, controlled skill
> surfaces, and verified live literature/compound probes — not a 5.0 lab OS.
