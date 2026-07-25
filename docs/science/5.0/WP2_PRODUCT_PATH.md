# WP-2 Product Path — ResearchProject + EvidenceGraph

**Status:** PREVIEW product path (not full 5.0)  
**Date:** 2026-07-26  
**Depends on:** Lumen Science 1.0.0  

## What shipped

### Rust (`xai-grok-science`)

| Component | Path |
|-----------|------|
| Feature gates | `features.rs` — RealDevice/DeviceCommand **Disabled**; research **Preview** |
| ProjectStore | `project/store.rs` — durable project + graph + claims |
| Tests | feature gates + create/claim/evidence/ownership fail-closed |

### ACP (SessionActor-gated)

```text
x.ai/science/project_create
x.ai/science/project_get
x.ai/science/project_list
x.ai/science/project_transition
x.ai/science/claim_propose
x.ai/science/evidence_attach
```

All require live session + `store_root` inside session cwd.

### CLI (local productivity adapter)

```bash
lumen-science project create --owner O --title T --question Q [--store DIR]
lumen-science project list|get
lumen-science claim propose --project ID --owner O --by WHO --statement S
```

## Invariants

- SessionActor remains sole execution authority for ACP paths  
- Feature gates fail closed on Disabled  
- Ownership mismatch → error  
- Evidence attach requires hex artifact SHA-256  
- Device features remain Disabled  

## Not yet (later WPs)

- Workflow DAG execution product path (WP-4/5)  
- Multi-user collaboration (WP-7)  
- HPC/remote compute live (WP-8)  
- Real devices (WP-9+)  

## Acceptance for this slice

```text
cargo test -p xai-grok-science --lib store:: features::
CLI project create + claim propose
ACP handlers registered in science.rs
5.0 README updated: WP-2 product path PREVIEW
```
