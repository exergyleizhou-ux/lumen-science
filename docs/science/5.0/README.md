# Lumen Science 5.0 — Governed Embodied Science

**Target Version**: 5.0
**Current Phase**: WP-1 (LS5-0 through LS5-5) — Baseline & Contract
**Status**: IN PROGRESS

## Intermediate Versions

| Version | Deliverable | Status |
|---------|-------------|--------|
| 1.0 | Connector fusion + offline product loop | ✅ COMPLETE |
| 2.0 | Research Project + Evidence Graph | 🔜 NEXT |
| 3.0 | Reproducible Compute + Collaboration | ⏳ PLANNED |
| 4.0 | Digital Twin + BOS Dummy Lab | ⏳ PLANNED |
| 5.0 | Governed Embodied Science | ⏳ PLANNED |

## Work Packages

| WP | Milestones | Scope | Status |
|----|-----------|-------|--------|
| WP-1 | LS5-0 ~ LS5-5 | Baseline freeze + 5.0 contract | ✅ ACCEPT |
| WP-2 | LS5-6 ~ LS5-10 | Research Project aggregate + Evidence Graph | 🔜 |
| WP-3 | LS5-11 ~ LS5-14 | Evidence queries + migration | ⏳ |
| WP-4 | LS5-15 ~ LS5-18 | Workflow engine | ⏳ |
| WP-5 | LS5-19 ~ LS5-22 | Multi-kernel + reproduction | ⏳ |
| WP-6 | LS5-23 ~ LS5-29 | Multimodal workbench | ⏳ |
| WP-7 | LS5-30 ~ LS5-36 | Multi-role review + collaboration | ⏳ |
| WP-8 | LS5-37 ~ LS5-43 | Remote compute + HPC | ⏳ |
| WP-9 | LS5-44 ~ LS5-47 | BOS + Dummy Lab | ⏳ |
| WP-10 | LS5-48 ~ LS5-51 | Digital Twin | ⏳ |
| WP-11 | LS5-52 ~ LS5-55 | Real device admission + safety | ⏳ |
| WP-12 | LS5-56 ~ LS5-59 | Sensor trust chain + HIL + pilot | ⏳ |
| WP-13 | LS5-60 ~ LS5-66 | Security, scale, governance, ops | ⏳ |
| WP-14 | LS5-67 ~ LS5-70 | CI, cross-platform, migration, RC | ⏳ |
| WP-15 | LS5-71 ~ LS5-72 | Release + canary | ⏳ |

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
