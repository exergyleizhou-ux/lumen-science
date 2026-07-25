# Lumen Science Fusion Candidate — Status (2026-07-26)

## Verdict

```text
PRODUCT NAME:     Lumen Science Fusion candidate
NOT YET:          Lumen Science 1.0 formal release
NOT YET:          Lumen Science 5.0
```

### Level gates (planning book)

| Level | Scope | Verdict |
|-------|-------|---------|
| L1 DS-0～38 | connector inventory + dispositions | **PARTIAL → repaired toward ACCEPT** (lock 42/0; active runtime 40; rejected 2 out of fetch registry) |
| L2 DS-39～47 | offline product loop | **REJECT as complete** (Go prototypes + Motif contract UI; Rust SessionActor loop not fully proven) |
| L3 DS-48～58 | CI/release/live | **REJECT** (no formal tag/release assets/live auth proof) |
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

## Still open before true 1.0

- Per-connector negative-test battery depth (many thin adapters)  
- Rust MCP / notebook path under SessionActor  
- Motif `npm ci` supply-chain ACCEPT (needs user network auth)  
- Skill prompt-injection audits → controlled tool surfaces  
- Release tag, multi-platform signed binaries, authorized live proofs  

## Allowed marketing sentence

> Lumen Science is a local-first scientific workbench: offline sequence
> analysis with SHA-256 artifacts, honest connector admission, and a
> Motif-class review surface — without claiming autonomous science or
> unsupervised lab control.
