# Lumen Science 5.0 — Governed Embodied Science

**Target Version**: 5.0  
**Status**: SPEC / SCAFFOLD ONLY — **not a product release**  
**Prerequisite**: Formal **Lumen Science 1.0** release tag (DS-48～58) must ACCEPT first.

> Data models under `xai-grok-science::{project,workflow,device,...}` are
> contract sketches with unit tests. They are **not** SessionActor product-path
> ACCEPT milestones. Do not market “5.0 implemented”.

## Intermediate Versions

| Version | Deliverable | Status |
|---------|-------------|--------|
| Fusion candidate | Connector inventory + offline prototypes + Motif contract UI | 🔧 IN PROGRESS |
| 1.0 | Installable, auditable, signed release + live proof | ❌ NOT REACHED |
| 2.0 | Research Project + Evidence Graph (product path) | 📋 SPEC + domain models |
| 3.0 | Reproducible Compute + Collaboration | 📋 SPEC + domain models |
| 4.0 | Digital Twin + BOS Dummy Lab | 📋 SPEC (hardware deferred) |
| 5.0 | Governed Embodied Science | 📋 SPEC (hardware/ops deferred) |

## Work Packages

| WP | Milestones | Scope | Status |
|----|-----------|-------|--------|
| WP-1 | LS5-0 ~ LS5-5 | Baseline freeze + 5.0 contract docs | 📋 SPEC written |
| WP-2 | LS5-6 ~ LS5-10 | Research Project + Evidence Graph models | 📋 SCAFFOLD (not product-path ACCEPT) |
| WP-3 | LS5-11 ~ LS5-14 | Evidence queries + migration models | 📋 SCAFFOLD |
| WP-4 | LS5-15 ~ LS5-18 | Workflow engine models | 📋 SCAFFOLD |
| WP-5 | LS5-19 ~ LS5-22 | Multi-kernel + reproduction models | 📋 SCAFFOLD |
| WP-6 | LS5-23 ~ LS5-29 | Multimodal workbench types | 📋 SCAFFOLD |
| WP-7 | LS5-30 ~ LS5-36 | Multi-role review + collaboration types | 📋 SCAFFOLD |
| WP-8 | LS5-37 ~ LS5-43 | Remote compute + HPC types | 📋 SCAFFOLD |
| WP-9 | LS5-44 ~ LS5-47 | BOS + Dummy Lab | ⏸ DEFERRED (hardware) |
| WP-10 | LS5-48 ~ LS5-51 | Digital Twin | ⏸ DEFERRED |
| WP-11 | LS5-52 ~ LS5-55 | Real device admission + safety | ⏸ DEFERRED |
| WP-12 | LS5-56 ~ LS5-59 | Sensor trust chain + HIL + pilot | ⏸ DEFERRED |
| WP-13 | LS5-60 ~ LS5-66 | Security, scale, governance, ops | ⏸ DEFERRED |
| WP-14 | LS5-67 ~ LS5-70 | CI, cross-platform, migration, RC | ⏸ DEFERRED |
| WP-15 | LS5-71 ~ LS5-72 | Release + canary | ⏸ DEFERRED |

## Deliverables (this milestone)

- [x] LS5-0: V1 baseline confirmation (`LUMEN_SCIENCE_1_0_BASELINE.md`, `v1-release-baseline.lock.json`)
- [x] LS5-1: Golden corpus schema (deferred to WP-2 for live sample collection)
- [x] LS5-2: 10 Architecture Decision Records (`ARCHITECTURE_DECISION_RECORDS.md`)
- [x] LS5-3: Threat model — 15 attack vectors (`THREAT_MODEL.md`)
- [x] LS5-4: Schema evolution framework (`SCHEMA_EVOLUTION.md`)
- [x] LS5-5: 20 feature gates (`FEATURE_GATES.md`)

## Global Invariants (inherited from V1)

```text
Rust Lumen SessionActor = sole execution authority
PermissionManager       = sole approval authority
ArtifactRegistry        = sole durable artifact index
EvidenceGraph           = sole evidence relationship authority (V2+)
EventLog                = sole canonical replay source
ResearchResult          = sole formal research result container (V2+)
```

All external capabilities (connectors, MCP, Python, Motif, devices) are
adapters/consumers only — never independent runtime authorities.

## Next

WP-2: LS5-6 ~ LS5-10 — ResearchProject aggregate root implementation.
