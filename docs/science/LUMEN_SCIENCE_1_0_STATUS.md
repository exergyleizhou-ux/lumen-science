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
| Skills | **ACCEPT partial** — 5 approved with controlled tools; 22 pending compute |
| 5.0 | **SPEC only** |

## Live matrix (authorized public probes)

| Connector / path | Result | Evidence |
|------------------|--------|----------|
| PubMed | PASS | `outputs/evidence/live-1.0.0/cargo-live-probes-final.log` |
| ChEMBL | PASS | same |
| Europe PMC | PASS | same |
| Crossref | PASS | `CROSSREF_MAILTO` set for probe |
| UniProt | PASS | schema drift fixed (totalResults optional) |
| arXiv | FAIL (503 rate/upstream) | logged |
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

## Allowed claim

> Lumen Science 1.0.0 is a local-first, auditable scientific workbench with
> offline sequence analysis, multi-platform release builds with signed checksums,
> Motif-class review surfaces, honest connector admission, controlled skill
> surfaces, and verified live literature/compound probes — not a 5.0 lab OS.
