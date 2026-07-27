# Lumen Science 5.0 — Governed Embodied Science

**Target Version**: 5.0  
**Status**: IN PROGRESS — WP-2～8 **software preview / contract** (not product GA)  
**Prerequisite**: **Lumen Science CLI/MCP** formal release — **DONE** (`v1.0.0`, current line **`v1.0.1`**)

## Evidence levels (how to read status)

```text
source     = types + logic in tree
unit       = cargo/go unit tests green
CLI        = lumen-science CLI path exercised
ACP        = SessionActor handler registered (not always built-binary e2e)
CI         = covered by Lumen Science CI on main
release    = versioned install asset + checksum on GitHub Release
live       = live network probe authorized and green
```

A WP marked **Preview** means at least `source` + `unit` (often CLI/ACP).
It does **not** mean full Workflow Actor, multi-kernel live execution, signed collab
packages, live SSH/Slurm, Dummy Lab built-binary e2e, or 5.0 RC.

## Intermediate Versions

| Version | Deliverable | Status (honest) |
|---------|-------------|-----------------|
| 1.0 | Offline workbench + multi-platform assets on Release | ✅ **RELEASED** (`v1.0.1` current; MANIFEST + SHA256SUMS; Desktop still `1.1.0-dev`) |
| 2.0 (WP-2/3) | ResearchProject + EvidenceGraph + Queries | 🔧 **PREVIEW** — source/unit/CLI/ACP; not full restart/replay release |
| 3.0 (WP-4/5) | Workflow DAG + dry-run + kernel admission | 🔧 **PREVIEW** — validate/dry-run models only; no full Workflow Actor |
| 4.0 (WP-6) | Multimodal index + review | 🔧 **PREVIEW** — index/admission surface; not full pinned parser product chain |
| 4.0+ (WP-7/8) | Collaboration + remote compute | 🔧 **PREVIEW** — local records + remote **plan/dry-run only** |
| 5.0 (WP-9~15) | Devices / twin / HIL / release canary | ⏸ DEFERRED (hardware / ops) |

## Work Packages

| WP | Scope | Status | Evidence |
|----|-------|--------|----------|
| WP-1 | Baseline freeze + 5.0 contract docs | ✅ ACCEPT | source + docs |
| WP-2 | ResearchProject + EvidenceGraph product path | 🔧 Preview | source, unit, CLI, ACP |
| WP-3 | Evidence queries + trace + compare + consistency + migration | 🔧 Preview | source, unit, CLI |
| WP-4 | Workflow validate + dry-run (allowed steps only) | 🔧 Preview | source, unit, CLI (not full actor) |
| WP-5 | Kernel admission + reproduction status | 🔧 Preview | source, unit (admission model) |
| WP-6 | Multimodal index preview | 🔧 Preview | source, unit |
| WP-7 | Multi-role review + collaboration records | 🔧 Preview | source, unit, CLI records |
| WP-8 | Remote compute plan/dry-run (no live HPC) | 🔧 Preview | source, unit (plan only) |
| WP-9～15 | Devices / twin / HIL / release canary | ⏸ DEFERRED | see `DEFERRED.md` |

## Global Invariants

```text
Rust Lumen SessionActor = sole execution authority
FeatureGates            = RealDevice/DeviceCommand Disabled by default
EvidenceGraph           = sole evidence relationship authority (V2+)
Workflow                = white-listed StepKinds only; never arbitrary shell
```

## Lumen core admission

The newer Lumen core is the source of truth, but Science admits it in reviewed
security/correctness slices until a shared-core Platform API exists. See
[`CORE_V0_1_251_ADMISSION.md`](CORE_V0_1_251_ADMISSION.md) and the
machine-readable
[`core-v0.1.251-admission.lock.json`](core-v0.1.251-admission.lock.json).
This is not a claim that the embedded `0.1.222` line already has complete
v0.1.251 source or release parity.

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
