<!-- status-claim: historical -->
> **Historical record.** Point-in-time snapshot for `1.0.0-rc.1`; its numbers are
> not current. Live status: [`docs/science/status/current.json`](status/current.json).

# Lumen Science 1.0.0-rc.1 — Status

**Date:** 2026-07-26  
**Version:** `1.0.0-rc.1`  
**Tag:** `v1.0.0-rc.1`

## Verdict

```text
PRODUCT:     Lumen Science 1.0.0 Release Candidate
ENOUGH FOR:  Offline scientific productivity + multi-platform release + CI gates
             + optional live literature brief (PubMed/ChEMBL)
NOT YET:     Formal v1.0.0 (broader live matrix, optional signing)
NEVER CLAIM: Lumen Science 5.0 / autonomous lab / medical certification
```

## Level gates (planning book)

| Level | Scope | Verdict |
|-------|-------|---------|
| L1 DS-0～38 | connector inventory | **ACCEPT** — 42 dispositions, 0 unresolved; 40 active runtime; BioGRID/KEGG rejected not fetchable |
| L2 DS-39～47 | offline product loop | **ACCEPT for offline path** — seqbench + pipeline offline + artifact SHA-256 + Motif vendor + dogfood green |
| L3 DS-48～58 | CI / release / live | **ACCEPT for release+CI**; live `brief aspirin` **PASS** (PubMed+ChEMBL); full connector live matrix still operator-optional |
| 5.0 | embodied science | **SPEC only** — not a product claim |

## Release surface

```bash
cd packs/science
make release          # VERSION from ../../VERSION
# → dist/science-release/
#    lumen-science-{darwin,linux}-{arm64,amd64}[+windows]
#    lumen-mcp-* helpers
#    lumen-science-1.0.0-rc.1-*.tar.gz / .zip
#    SHA256SUMS (35 lines, shasum -c PASS)
```

Committed evidence (binaries not in git):

- `outputs/release/1.0.0-rc.1/SHA256SUMS`
- `outputs/release/1.0.0-rc.1/MANIFEST.json`
- `release/science/README.md`

Install local:

```bash
./scripts/install-science.sh
lumen-science version   # 1.0.0-rc.1
```

## CI enforcement

`.github/workflows/science-ci.yml`:

- Go unit (MCP + seqbench + pipeline)
- `make smoke` (CLI seq + pipeline offline)
- `scripts/science-machine-gates.sh`
- Cross-compile matrix (5 platforms)
- `make release` + SHA256SUMS verify on main

## Live evidence

`lumen-science brief aspirin` (2026-07-26): returned PubMed PMIDs and ChEMBL compounds with provenance. Not medical advice.

## Remaining before formal v1.0.0

1. Broader live connector matrix (operator-authorized)  
2. Optional code signing / notarization for wide distribution  
3. Skills remain `approved=0` until real prompt-injection + controlled-tool admission  

## Forbidden claims

- Fully autonomous scientist  
- Unsupervised wet-lab device control  
- All 27 skills production-approved  
- Medical / clinical certification  

## Allowed claim

> Lumen Science 1.0.0-rc.1 is a local-first, auditable scientific workbench with
> offline sequence analysis, multi-platform release builds, Motif-class review
> surfaces, honest connector admission, and optional live literature brief —
> enough for product offline use; not a 5.0 lab OS.
