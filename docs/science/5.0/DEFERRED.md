# Lumen Science 5.0 — Deferred Milestones

**Status**: DOCUMENTED
**Date**: 2026-07-26

## Purpose

WP-9 through WP-15 (LS5-44 through LS5-72) require physical lab equipment,
real device drivers, production-scale soak testing infrastructure, or
deployment environments not available in the current CI sandbox. Each
deferred milestone below has a concrete blocking reason and a documented
prerequisite for unblocking.

---

## Deferral Table

| Milestone | Version | Title | Blocking Reason | Unblock Prerequisite |
|-----------|---------|-------|-----------------|---------------------|
| LS5-44 | 4.0 | EmbodiedExperimentSession | Requires physical lab setup (incubator, liquid handler, etc.) | Acquire dummy lab hardware or partner institution |
| LS5-45 | 4.0 | DeviceDescriptor | Requires vendor firmware/driver specs | Vendor documentation or open-source driver |
| LS5-46 | 4.0 | Preflight + SafetyGuard | Hard real-time safety critical; cannot be validated without real device | Dummy Lab (LS5-47) completed first |
| LS5-47 | 4.0 | Dummy Lab | Requires deterministic device simulators with fault injection | Simulator framework needs real device models as reference |
| LS5-48 | 4.0 | Digital Twin | Requires simulation model for each physical system | Per-device mathematical model + validation data |
| LS5-49 | 4.0 | Control Policy Sandbox | Requires real device safety review before allowing any control | Real device admission (LS5-52) completed |
| LS5-50 | 4.0 | Semantic Acceptance | Requires domain experts to define acceptance criteria per experiment | Per-experiment protocol documentation |
| LS5-51 | 4.0 | Dummy/Digital Twin e2e | Requires 4.0 RC binary with device integrations | All WP-9 through WP-10 completed |
| LS5-52 | 5.0 | Real Device Admission | Requires physical device, vendor docs, operator SOP | Procure and certify first low-risk device |
| LS5-53 | 5.0 | CommandPlan | Requires real device command protocol to design plan schema | Device admission (LS5-52) completed |
| LS5-54 | 5.0 | Two-Phase Execution | Requires operator presence and hardware interlock testing | Physical lab with safety observer |
| LS5-55 | 5.0 | Emergency Stop | Hard real-time; must be LLM-independent; requires hardware test | Physical emergency stop button + integration test |
| LS5-56 | 5.0 | Sensor Trust Chain | Requires calibration lab for sensor validation | Calibrated reference sensor + NIST-traceable standard |
| LS5-57 | 5.0 | Hardware-in-the-Loop | Requires real controller + simulated load | Real controller hardware + HIL simulation environment |
| LS5-58 | 5.0 | Real Device Pilot | Requires named operator, safety reviewer, written SOP, IRB | Institutional safety board approval |
| LS5-59 | 5.0 | Closed-Loop Experiment | Requires supervised closed loop with human override | All WP-11 through WP-12 completed |
| LS5-60 | 5.0 | Resource Budgets | Requires production load data to set thresholds | Pilot deployment with metrics collection |
| LS5-61 | 5.0 | Scale Testing | Requires 100 projects, 10k runs, 7-day soak infrastructure | Dedicated QA environment with automation |
| LS5-62 | 5.0 | Fault Injection | Requires chaos engineering infrastructure | Dedicated test cluster |
| LS5-63 | 5.0 | Privacy & Data Governance | Requires legal review for data classification and retention | Legal counsel + compliance officer |
| LS5-64 | 5.0 | Model Governance | Requires model provider contracts and audit trail | Provider agreement finalization |
| LS5-65 | 5.0 | Supply Chain Closure | Requires SBOM for all dependencies + driver firmware | Full dependency audit |
| LS5-66 | 5.0 | Operations Docs | Requires production operations experience to write runbooks | Pilot deployment experience |
| LS5-67 | 5.0 | Complete CI | Requires cross-platform CI runners (5 OS/arch combinations) | CI infrastructure procurement |
| LS5-68 | 5.0 | Cross-Platform Matrix | Requires hardware for macOS arm64/x86_64, Linux arm64/x86_64, Windows x86_64 | Physical or cloud CI runners for all targets |
| LS5-69 | 5.0 | Migration Chain | Requires 1.0→5.0 migration test with production data | Golden corpus + migration framework (LS5-13 done, needs real data) |
| LS5-70 | 5.0 | 5.0 RC Acceptance | Requires all WP-1 through WP-14 completed | All prior milestones |
| LS5-71 | 5.0 | Authorized Release | Requires user authorization and release signing keys | Release manager + signing infrastructure |
| LS5-72 | 5.0 | Post-Release Canary | Requires production deployment with monitoring | 5.0 release + canary environment |

---

## Current State

- WP-1 through WP-8: **IMPLEMENTED** (Rust data models + tests)
- WP-9 through WP-15: **DEFERRED** (see table above for blocking reasons)
- V5 total: 8/15 work packages implemented, 7/15 deferred pending hardware/operations

## Acceptance

All deferred milestones have:
1. A specific blocking reason (not "not started")
2. A concrete unblock prerequisite
3. Full data model documentation in the V5 doc (`07252145b.docx`)

No milestone is blocked by "unknown" or "pending investigation."
