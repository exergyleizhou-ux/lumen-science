# Lumen Science 5.0 — Governed Embodied Science

**Target Version**: 5.0  
**Status**: IN PROGRESS (WP-3～8 product-path Preview, hardware deferred)  
**Prerequisite**: **Lumen Science 1.0.0** formal release — **DONE** (`v1.0.0`)

## Intermediate Versions

| Version | Deliverable | Status |
|---------|-------------|--------|
| 1.0 | Installable offline workbench + release | ✅ **RELEASED** (`v1.0.0`) |
| 2.0 (WP-2/3) | ResearchProject + EvidenceGraph + Queries | 🔧 **PREVIEW** (store + ACP + CLI + trace/compare/migrate) |
| 3.0 (WP-4/5) | Workflow DAG + dry-run + kernel admission | 🔧 **PREVIEW** (validate/dry-run + kernels, RealDevice Disabled) |
| 4.0 (WP-6) | Multimodal index + review | 🔧 **PREVIEW** (parser/renderer index, reviewer records) |
| 4.0+ (WP-7/8) | Collaboration + remote compute | 🔧 **PREVIEW** (collab records, remote dry-run plan only) |
| 5.0 (WP-9~15) | Devices / twin / HIL / release canary | ⏸ DEFERRED (hardware) |

## Work Packages

| WP | Scope | Status |
|----|-------|--------|
| WP-1 | Baseline freeze + 5.0 contract docs | ✅ ACCEPT |
| WP-2 | ResearchProject + EvidenceGraph product path | ✅ Preview |
| WP-3 | Evidence queries + trace + compare + consistency + migration | ✅ Preview |
| WP-4 | Workflow validate + dry-run (allowed steps only) | ✅ Preview |
| WP-5 | Kernel admission + reproduction status | ✅ Preview |
| WP-6 | Multimodal index preview | ✅ Preview |
| WP-7 | Multi-role review + collaboration records | ✅ Preview |
| WP-8 | Remote compute plan/dry-run (no live HPC) | ✅ Preview |
| WP-9～15 | Devices / twin / HIL / release canary | ⏸ DEFERRED |

## Global Invariants

```text
Rust Lumen SessionActor = sole execution authority
FeatureGates            = RealDevice/DeviceCommand Disabled by default
EvidenceGraph           = sole evidence relationship authority (V2+)
Workflow                = white-listed StepKinds only; never arbitrary shell
```

## Quick start (WP-2～8 CLI surface)

```bash
lumen-science project create|list|get
lumen-science claim propose
# Evidence queries:
lumen-science project evidence trace --project ID --claim CID [--store DIR]
lumen-science project evidence compare --project ID --claim-a C1 --claim-b C2 [--store DIR]
lumen-science project evidence consistency --project ID [--store DIR]
lumen-science project migrate --run RID --owner O --title T --question Q [--store DIR]
# Workflow:
lumen-science workflow validate --spec-file WF.json [--store DIR]
# Preview surfaces:
lumen-science project multimodal --project ID [--store DIR]
lumen-science project review --project ID --reviewer R --verdict V [--store DIR]
lumen-science project collaborator --project ID --owner O --invitee I [--store DIR]
```

## Rust ACP gates

- All WP-2～8 store APIs behind `features.rs` gate checks
- Disabled gates → `ScienceError::FeatureDisabled`
- Device gates stay `Disabled`

## Not-yet / Never-claim

- Real devices (WP-9+)
- Multi-kernel live execution (dry-run ok)
- HPC live scheduling (plan/dry-run ok)
- All 27 skills approved (17 GPU/remote still pending)
- Formal 5.0 GA release
