# Lumen Science 5.0 — Governed Embodied Science

**Target Version**: 5.0  
**Status**: IN PROGRESS (product path starting at WP-2 Preview)  
**Prerequisite**: **Lumen Science 1.0.0** formal release — **DONE** (`v1.0.0`)

> Do not market “5.0 complete”. Hardware WPs remain deferred.

## Intermediate Versions

| Version | Deliverable | Status |
|---------|-------------|--------|
| 1.0 | Installable offline workbench + release + live subset | ✅ **RELEASED** (`v1.0.0`) |
| 2.0 | Research Project + Evidence Graph **product path** | 🔧 **PREVIEW** (WP-2 store + ACP + CLI) |
| 3.0 | Reproducible Compute + Collaboration | 📋 SPEC + domain models |
| 4.0 | Digital Twin + BOS Dummy Lab | ⏸ DEFERRED (hardware) |
| 5.0 | Governed Embodied Science | 📋 SPEC (hardware/ops deferred) |

## Work Packages

| WP | Scope | Status |
|----|-------|--------|
| WP-1 | Baseline freeze + 5.0 contract docs | ✅ ACCEPT |
| WP-2 | ResearchProject + EvidenceGraph product path | 🔧 PREVIEW — `ProjectStore`, feature gates, ACP + CLI |
| WP-3 | Evidence queries + migration | 📋 SCAFFOLD models + query module |
| WP-4 | Workflow engine | 📋 SCAFFOLD models |
| WP-5 | Multi-kernel + reproduction | 📋 SCAFFOLD models |
| WP-6 | Multimodal workbench | 📋 SCAFFOLD types |
| WP-7 | Multi-role review + collaboration | 📋 SCAFFOLD types |
| WP-8 | Remote compute + HPC | 📋 SCAFFOLD types |
| WP-9～15 | Devices / twin / HIL / release canary | ⏸ DEFERRED |

## Global Invariants

```text
Rust Lumen SessionActor = sole execution authority
PermissionManager       = sole approval authority
ArtifactRegistry        = sole durable artifact index
EvidenceGraph           = sole evidence relationship authority (V2+)
EventLog                = sole canonical replay source
FeatureGates            = V2+ opt-in; RealDevice disabled by default
```

## WP-2 quick start

```bash
# CLI
lumen-science project create --owner u1 --title "Study" --question "Q?"
lumen-science claim propose --project <id> --owner u1 --by sci --statement "..."

# Rust
# ProjectStore::create_project / propose_claim / attach_evidence
# FeatureGates::default() disables real device control

# ACP (requires session)
# x.ai/science/project_create | claim_propose | evidence_attach | ...
```

See `WP2_PRODUCT_PATH.md`.

## Next

WP-3/4: evidence query product path + workflow DAG execution under gates (still no devices).
