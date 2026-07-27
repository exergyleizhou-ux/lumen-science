//! Science feature gates (LS5-5). V2–V5 capabilities require explicit gate state.
//!
//! Default: research/evidence/workflow features are Preview; real device control
//! is Disabled. SessionActor (or CLI) must check gates before product paths.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::ScienceError;

/// Feature identifiers for gated science capabilities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScienceFeature {
    ResearchProject,
    EvidenceGraph,
    ClaimLifecycle,
    EvidenceQuery,
    WorkflowDag,
    ComputeEnvironment,
    MultiKernel,
    WorkflowPackage,
    ReproductionReport,
    Collaboration,
    ReviewPackage,
    RemoteCompute,
    HpcScheduler,
    DataMovement,
    DummyLab,
    DigitalTwin,
    DeviceDescriptor,
    SafetyGuard,
    DeviceCommand,
    HardwareInLoop,
    RealDevice,
    MigrationChain,
    ScaleTest,
    FaultInjection,
}

impl ScienceFeature {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ResearchProject => "research_project",
            Self::EvidenceGraph => "evidence_graph",
            Self::ClaimLifecycle => "claim_lifecycle",
            Self::EvidenceQuery => "evidence_query",
            Self::WorkflowDag => "workflow_dag",
            Self::ComputeEnvironment => "compute_environment",
            Self::MultiKernel => "multi_kernel",
            Self::WorkflowPackage => "workflow_package",
            Self::ReproductionReport => "reproduction_report",
            Self::Collaboration => "collaboration",
            Self::ReviewPackage => "review_package",
            Self::RemoteCompute => "remote_compute",
            Self::HpcScheduler => "hpc_scheduler",
            Self::DataMovement => "data_movement",
            Self::DummyLab => "dummy_lab",
            Self::DigitalTwin => "digital_twin",
            Self::DeviceDescriptor => "device_descriptor",
            Self::SafetyGuard => "safety_guard",
            Self::DeviceCommand => "device_command",
            Self::HardwareInLoop => "hardware_in_loop",
            Self::RealDevice => "real_device",
            Self::MigrationChain => "migration_chain",
            Self::ScaleTest => "scale_test",
            Self::FaultInjection => "fault_injection",
        }
    }

    pub fn all() -> &'static [ScienceFeature] {
        &[
            Self::ResearchProject,
            Self::EvidenceGraph,
            Self::ClaimLifecycle,
            Self::EvidenceQuery,
            Self::WorkflowDag,
            Self::ComputeEnvironment,
            Self::MultiKernel,
            Self::WorkflowPackage,
            Self::ReproductionReport,
            Self::Collaboration,
            Self::ReviewPackage,
            Self::RemoteCompute,
            Self::HpcScheduler,
            Self::DataMovement,
            Self::DummyLab,
            Self::DigitalTwin,
            Self::DeviceDescriptor,
            Self::SafetyGuard,
            Self::DeviceCommand,
            Self::HardwareInLoop,
            Self::RealDevice,
            Self::MigrationChain,
            Self::ScaleTest,
            Self::FaultInjection,
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateState {
    Disabled,
    Preview,
    Beta,
    Stable,
}

impl GateState {
    pub fn allows_use(self) -> bool {
        matches!(self, Self::Preview | Self::Beta | Self::Stable)
    }

    pub fn is_preview(self) -> bool {
        matches!(self, Self::Preview)
    }
}

/// Runtime feature gate configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureGates {
    pub gates: BTreeMap<String, GateState>,
}

impl Default for FeatureGates {
    fn default() -> Self {
        Self::v2_preview_defaults()
    }
}

impl FeatureGates {
    /// Defaults from FEATURE_GATES.md: V2 research preview; devices disabled.
    pub fn v2_preview_defaults() -> Self {
        let mut gates = BTreeMap::new();
        for f in ScienceFeature::all() {
            let state = match f {
                ScienceFeature::DeviceCommand
                | ScienceFeature::HardwareInLoop
                | ScienceFeature::RealDevice => GateState::Disabled,
                ScienceFeature::DummyLab
                | ScienceFeature::DigitalTwin
                | ScienceFeature::DeviceDescriptor
                | ScienceFeature::SafetyGuard => GateState::Preview,
                _ => GateState::Preview,
            };
            gates.insert(f.as_str().to_string(), state);
        }
        Self { gates }
    }

    pub fn get(&self, feature: ScienceFeature) -> GateState {
        self.gates
            .get(feature.as_str())
            .copied()
            .unwrap_or(GateState::Disabled)
    }

    pub fn set(&mut self, feature: ScienceFeature, state: GateState) {
        self.gates.insert(feature.as_str().to_string(), state);
    }

    /// Build one immutable session snapshot from operator overrides.
    ///
    /// Unknown keys are rejected instead of being silently ignored. The
    /// resulting map is complete: omitted features retain the compiled safe
    /// defaults, while explicit operator states replace them.
    pub fn from_overrides(overrides: &BTreeMap<String, GateState>) -> Result<Self, ScienceError> {
        let mut gates = Self::default();
        for (name, state) in overrides {
            let feature = ScienceFeature::all()
                .iter()
                .copied()
                .find(|feature| feature.as_str() == name)
                .ok_or_else(|| {
                    ScienceError::Invalid(format!("unknown science feature gate: {name}"))
                })?;
            gates.set(feature, *state);
        }
        Ok(gates)
    }

    /// Fail closed if feature is Disabled.
    pub fn require(&self, feature: ScienceFeature) -> Result<GateState, ScienceError> {
        let state = self.get(feature);
        if !state.allows_use() {
            return Err(ScienceError::FeatureDisabled(feature.as_str().to_string()));
        }
        Ok(state)
    }

    /// Require every capability a compound operation depends on.
    pub fn require_all(&self, features: &[ScienceFeature]) -> Result<Vec<GateState>, ScienceError> {
        features
            .iter()
            .copied()
            .map(|feature| self.require(feature))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_disable_real_devices() {
        let g = FeatureGates::default();
        assert_eq!(g.get(ScienceFeature::RealDevice), GateState::Disabled);
        assert_eq!(g.get(ScienceFeature::DeviceCommand), GateState::Disabled);
        assert!(g.require(ScienceFeature::RealDevice).is_err());
    }

    #[test]
    fn research_project_preview_allowed() {
        let g = FeatureGates::default();
        let s = g.require(ScienceFeature::ResearchProject).unwrap();
        assert!(s.is_preview());
    }

    #[test]
    fn can_enable_beta() {
        let mut g = FeatureGates::default();
        g.set(ScienceFeature::WorkflowDag, GateState::Beta);
        assert_eq!(g.get(ScienceFeature::WorkflowDag), GateState::Beta);
        assert!(g.require(ScienceFeature::WorkflowDag).unwrap().allows_use());
    }

    #[test]
    fn operator_overrides_are_complete_and_fail_closed_on_unknown_keys() {
        let overrides = BTreeMap::from([
            ("research_project".to_string(), GateState::Disabled),
            ("workflow_dag".to_string(), GateState::Stable),
        ]);
        let gates = FeatureGates::from_overrides(&overrides).unwrap();
        assert_eq!(
            gates.get(ScienceFeature::ResearchProject),
            GateState::Disabled
        );
        assert_eq!(gates.get(ScienceFeature::WorkflowDag), GateState::Stable);
        assert_eq!(gates.get(ScienceFeature::EvidenceGraph), GateState::Preview);

        let error = FeatureGates::from_overrides(&BTreeMap::from([(
            "research_projet".to_string(),
            GateState::Stable,
        )]))
        .unwrap_err();
        assert!(error.to_string().contains("unknown science feature gate"));
    }
}
