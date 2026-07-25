# Deferred V5 Milestones

**Status**: DOCUMENTED
**Date**: 2026-07-26
**Principle**: Every deferred milestone is acknowledged with a specific blocking reason. No "we'll do it later" without explanation.

---

## WP-9: LS5-44 ~ LS5-47 — BOS + Dummy Lab

| Milestone | Summary | Blocking Reason |
|-----------|---------|-----------------|
| LS5-44 | EmbodiedExperimentSession | Requires real device driver contracts + safety-reviewed hardware interface specs |
| LS5-45 | DeviceDescriptor | Requires per-device manufacturer models (vendor, firmware, calibration) — no generic abstraction |
| LS5-46 | Preflight + SafetyGuard | Requires physical interlock documentation and operator SOP that cannot be simulated |
| LS5-47 | Dummy Lab | Requires deterministic device simulators (incubator, liquid handler, plate reader, microscope, actuator) with physics-level fidelity |

**Resolution path**: Contract with a wet-lab partner for device access. Build Dummy Lab simulators from vendor datasheets.

## WP-10: LS5-48 ~ LS5-51 — Digital Twin

| Milestone | Summary | Blocking Reason |
|-----------|---------|-----------------|
| LS5-48 | Digital Twin | Requires domain-specific simulation models (fluid dynamics, thermodynamics, reaction kinetics) |
| LS5-49 | Control sandbox | Requires verified controller models with bounded authority |
| LS5-50 | Semantic acceptance | Requires domain experts to define acceptance criteria per experiment class |
| LS5-51 | Dummy/Digital Twin e2e | Requires both Dummy Lab (WP-9) and Digital Twin (WP-10) complete |

**Resolution path**: Partner with computational modeling group. Build one pilot digital twin for a simple incubator model.

## WP-11: LS5-52 ~ LS5-55 — Real Device Admission + Safety

| Milestone | Summary | Blocking Reason |
|-----------|---------|-----------------|
| LS5-52 | Real device admission | Requires per-device regulatory compliance review. Cannot admit "generic serial port device" |
| LS5-53 | CommandPlan | Requires immutable command plans with hardware-level timing verification |
| LS5-54 | Two-phase execution | Requires operator presence verification hardware |
| LS5-55 | Emergency stop | Requires LLM-independent deterministic kill path — hardware-level requirement |

**Resolution path**: Begin with observe-only mode on a single low-risk device (temperature sensor). Build from there.

## WP-12: LS5-56 ~ LS5-59 — Sensor Trust Chain + HIL + Pilot

| Milestone | Summary | Blocking Reason |
|-----------|---------|-----------------|
| LS5-56 | Sensor trust chain | Requires per-sensor calibration certificates and timestamp source validation |
| LS5-57 | Hardware-in-the-loop | Requires real controller + simulated load setup |
| LS5-58 | Real device pilot | Requires named operator, independent safety reviewer, written SOP, calibration evidence |
| LS5-59 | Closed-loop pilot | Requires sequential safety escalation: observe→recommend→single→sequence→supervised |

**Resolution path**: Requires institutional safety board approval. Cannot proceed without.

## WP-13: LS5-60 ~ LS5-66 — Security, Scale, Governance, Ops

| Milestone | Summary | Blocking Reason |
|-----------|---------|-----------------|
| LS5-60 | Resource budget | Requires production profiling data — cannot be estimated from development |
| LS5-61 | Scale testing | Requires 100 projects, 10,000 runs, 1M evidence edges, 50 concurrent workflows — needs dedicated test infrastructure |
| LS5-62 | Fault injection | Requires production-like environment with disk full, power loss, clock jump simulation |
| LS5-63 | Privacy + data governance | Requires legal review for PII/PHI, HIPAA, GxP, CLIA compliance |
| LS5-64 | Model governance | Requires per-provider model version tracking infrastructure |
| LS5-65 | Supply chain lock | Requires SBOM + signature + reproducible build verification at release time |
| LS5-66 | Operations docs | Requires operational runbooks written with production experience |

**Resolution path**: Begin with LS5-65 (SBOM can be generated from Cargo.lock). Defer rest to production deployment phase.

## WP-14: LS5-67 ~ LS5-70 — CI, Cross-Platform, Migration, RC

| Milestone | Summary | Blocking Reason |
|-----------|---------|-----------------|
| LS5-67 | Complete CI | Requires dedicated CI runners with all 5 platforms + credential-free environments |
| LS5-68 | Cross-platform matrix | Requires macOS arm64/x86_64, Linux arm64/x86_64, Windows x86_64 build infrastructure |
| LS5-69 | Migration chain | Requires 1.0→2.0→3.0→4.0→5.0 release artifacts — each intermediate version must be released first |
| LS5-70 | 5.0 RC acceptance | Requires all prior milestones (WP-9 through WP-13) to be complete |

**Resolution path**: Start with LS5-67 (science CI). LS5-68 partially done (Windows + Linux CI exists). LS5-69 requires version releases first.

## WP-15: LS5-71 ~ LS5-72 — Release + Canary

| Milestone | Summary | Blocking Reason |
|-----------|---------|-----------------|
| LS5-71 | Authorized release | Requires explicit user authorization for merge, tag, push, publish — cannot be automated |
| LS5-72 | Release canary | Requires fresh install + 1.0→5.0 migration + rollback test on all platforms |

**Resolution path**: User-initiated. All preceding milestones must be complete.
