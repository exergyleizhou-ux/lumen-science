# Lumen Science — Product Path Contract

**Date:** 2026-07-26  
**Status:** BINDING for Fusion candidate work  

## Sole authorities

```text
Rust Lumen SessionActor  = sole execution authority
PermissionManager        = sole approval authority
ArtifactRegistry         = sole durable artifact index
EventLog                 = sole canonical replay source
```

Everything else is adapter/consumer only:

```text
connectors (Rust)     → fetch + parse → register raw artifact
Go MCP prototypes     → contract prototypes only (not final single-binary authority)
Python notebook       → SessionActor-spawned, default no-network
MotifRenderer         → CSP review UI over registered artifacts
Skills (ACP)          → plans / controlled tool calls; never independent runtime
5.0 domain models     → records/specs until post-1.0 gates pass
```

## Offline product loop (Level 2 target)

```text
connector fetch (fixture or authorized live)
  → raw artifact (SHA-256)
  → notebook / transform (optional)
  → derived artifact (SHA-256)
  → reviewer (observation/claim + evidence refs only)
  → renderer / MotifRenderer (derived view artifact)
  → reopen / replay from EventLog + fixtures
```

## Current proof level (honest)

| Path | Proof |
|------|-------|
| Rust connector fixture parse | unit tests in `xai-grok-science` |
| Built-binary connector ACP | one e2e test path (debug binary historically) |
| Go artifacts/notebook/reviewer/http_bridge | 49 unit tests (prototype servers) |
| Motif full vendored build | **not** proven (deps blocked without network auth) |
| Skills approved runtime | **0** approved (27 pending honest admission) |
| Formal release binary | **not** published |

## Forbidden claims

- “Lumen Science 1.0 complete” without DS-48～58 release + live gates  
- “Claude Science parity” without Motif DS-45B ACCEPT + controlled skill tools  
- “5.0 implemented” for WP data models without SessionActor product-path proof  

## Go MCP vs Rust target

Go servers under `packs/science/mcp/` prove API contracts and negative tests.  
Final 1.0 shipping shape remains **single Lumen binary** with science crates under
SessionActor. Dual-runtime must not become a second permission authority.
