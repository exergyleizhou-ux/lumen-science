# Lumen Science 5.0 Feature Gates

**Status**: IMPLEMENTED v1
**Date**: 2026-07-25
**Milestone**: LS5-5

## Design

All V2-V5 features have typed feature gates. Preview capabilities retain their
compiled preview defaults for compatibility; device execution remains disabled.
An operator can override any state in Lumen's primary `config.toml`.

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

## Operator Configuration

Lumen reads the primary product config from `$LUMEN_HOME/config.toml` (or
`~/.lumen/config.toml`):

```toml
[science_features]
research_project = "disabled"
workflow_dag = "beta"
real_device = "disabled"
```

Unknown feature names and invalid states reject the config instead of being
ignored. Omitted entries inherit the compiled defaults above.

## Gate Enforcement

- The composition root resolves one complete gate snapshot when a main session
  is created.
- A subagent inherits its parent session's snapshot unchanged.
- Read-only Science ACP routes and the SessionActor use the same snapshot.
- Project mutations and workflow execution are re-checked by SessionActor
  before a durable run or permission request is created.
- Editing `config.toml` does not widen an existing session; the next session
  receives the new validated snapshot.
- Disabled features return `ScienceError::FeatureDisabled`.
