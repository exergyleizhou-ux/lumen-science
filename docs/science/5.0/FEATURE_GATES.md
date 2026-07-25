# Lumen Science 5.0 Feature Gates

**Status**: SPEC v1
**Date**: 2026-07-25
**Milestone**: LS5-5

## Design

All V2-V5 features are behind explicit feature gates. Default stable users
cannot accidentally access incomplete or dangerous capabilities.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScienceFeature {
    // V2: Research Project & Evidence Graph
    ResearchProject,
    EvidenceGraph,
    ClaimLifecycle,
    EvidenceQuery,

    // V3: Workflow & Reproducible Compute
    WorkflowDag,
    ComputeEnvironment,
    MultiKernel,
    WorkflowPackage,
    ReproductionReport,

    // V3: Collaboration
    Collaboration,
    ReviewPackage,

    // V3: Remote Compute
    RemoteCompute,
    HpcScheduler,
    DataMovement,

    // V4: Digital Twin & Dummy Lab
    DummyLab,
    DigitalTwin,
    DeviceDescriptor,
    SafetyGuard,

    // V5: Real Device Control
    DeviceCommand,
    HardwareInLoop,
    RealDevice,

    // Infrastructure
    MigrationChain,
    ScaleTest,
    FaultInjection,
}
```

## Gate States

```text
Disabled        → completely unavailable, no code paths accessible
Preview         → available with explicit opt-in, warnings displayed
Beta            → available, may have known limitations
Stable          → fully tested, documented, default behavior
```

## Default Configuration

```json
{
  "science_features": {
    "research_project": "preview",
    "evidence_graph": "preview",
    "claim_lifecycle": "preview",
    "evidence_query": "preview",
    "workflow_dag": "preview",
    "compute_environment": "preview",
    "multi_kernel": "preview",
    "workflow_package": "preview",
    "reproduction_report": "preview",
    "collaboration": "preview",
    "review_package": "preview",
    "remote_compute": "preview",
    "hpc_scheduler": "preview",
    "data_movement": "preview",
    "dummy_lab": "preview",
    "digital_twin": "preview",
    "device_descriptor": "preview",
    "safety_guard": "preview",
    "device_command": "disabled",
    "hardware_in_loop": "disabled",
    "real_device": "disabled",
    "migration_chain": "preview",
    "scale_test": "preview",
    "fault_injection": "preview"
  }
}
```

## Gate Enforcement

- SessionActor checks feature gate before routing to any V2+ code path
- Disabled features return `ScienceError::FeatureDisabled`
- Preview features log a warning on first use per session
- Device features (command, HIL, real) require additional operator confirmation
  beyond the feature gate
