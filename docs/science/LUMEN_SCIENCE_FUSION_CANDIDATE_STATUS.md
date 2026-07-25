# Lumen Science Fusion Candidate — Status (superseded)

> **Superseded by `1.0.0-rc.1`.** See `docs/science/LUMEN_SCIENCE_1_0_RC_STATUS.md`.

## Verdict (historical — fusion candidate era)

```text
PRODUCT NAME:     Lumen Science Fusion candidate → advanced to 1.0.0-rc.1
FORMAL TIP NOW:   1.0.0-rc.1 (multi-platform release + CI; live still pending)
NOT YET:          Lumen Science 5.0
```

### Level gates (planning book)

| Level | Scope | Verdict |
|-------|-------|---------|
| L1 DS-0～38 | connector inventory + dispositions | **ACCEPT** (lock 42/0; active 40; rejected 2 not fetchable) |
| L2 DS-39～47 | offline product loop | **ACCEPT offline** (CLI + dogfood + Motif vendor) |
| L3 DS-48～58 | CI/release/live | **PARTIAL** — release+CI done; live pending |
| 5.0 LS5-* | governed embodied science | **SPEC/SCAFFOLD only** |

## Machine gates after this honesty pass

```bash
# fusion lock
python3 -c "import json;d=json.load(open('docs/science/fusion-sources.lock.json'));assert len(d['items'])==42;assert all(i.get('final_disposition') for i in d['items'])"

# skills: zero fake approvals
python3 -c "import json;d=json.load(open('packs/science/skills/registry.json'));assert d['summary']['approved']==0"

# active registry must not resolve rejected IDs (Rust tests)
```

## What was fixed in this pass

1. `fusion-sources.lock.json` — 42 resolved / 0 unresolved; Motif + open-science source locks  
2. BioGRID / KEGG — removed from **runtime** registry; kept in `rejected_registry()`  
3. Skills registry schema v2 — full DS-43 fields; **0 approved**  
4. MotifRenderer contract page (CSP, artifact-bound, admission matrix)  
5. Motif provenance + NOTICE + supply-chain audit (partial, honest)  
6. Product path + Fusion candidate status docs  
7. 5.0 README demoted from “1.0 COMPLETE” theater  
8. **Productivity CLI `lumen-science`**: `seq analyze`, `artifact *`, `pipeline offline`, `gates`  
9. **seqbench** Motif-class offline analyzer (GC, RC, translate, ORF, restriction)  
10. **Offline dogfood** `scripts/dogfood-science-offline.sh` + `install-science.sh`  

## Verified productivity path (local)

```text
go test seqbench + pipeline + artifacts  → PASS
lumen-science doctor                     → PASS
lumen-science gates                      → PASS
lumen-science pipeline offline           → review=pass + SHA-256 chain
```

## Still open before formal v1.0.0 (not rc)

- Authorized live connector proof (network + keys)  
- Optional binary code signing / notarization  
- Skill prompt-injection audits → controlled tool surfaces (`approved` still 0)  
- Rust MCP/notebook under SessionActor (Go remains contract prototype)  

## Allowed marketing sentence

> Lumen Science 1.0.0-rc.1 is a local-first scientific workbench: offline
> sequence analysis with SHA-256 artifacts, multi-platform release builds,
> honest connector admission, and a Motif-class review surface — without
> claiming autonomous science or unsupervised lab control.
