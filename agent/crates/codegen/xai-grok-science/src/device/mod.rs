//! BOS bridge, Dummy Lab, Device Descriptors, Digital Twin, Safety.
//! Seam: LS5-44~LS5-59. Pure data models — real HW integration needs physical lab.

use serde::{Deserialize, Serialize};

// ── Device Descriptor (LS5-45) ────────────────────────────────────
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceDescriptor {
    pub device_id: String,
    pub device_class: DeviceClass,
    pub manufacturer: String,
    pub model: String,
    pub firmware_version: String,
    pub driver_hash: String,
    pub capabilities: Vec<String>,
    pub command_schema: String,
    pub sensor_schema: String,
    pub calibration_required: bool,
    pub emergency_stop_semantics: EmergencyStop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeviceClass {
    Incubator,
    LiquidHandler,
    PlateReader,
    Microscope,
    EnvironmentalSensor,
    GenericActuator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EmergencyStop {
    ImmediatePowerOff,
    SafeStateThenOff,
    ManualOnly,
    NotApplicable,
}

// ── Experiment Session (LS5-44) ───────────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TargetMode {
    Dummy,
    DigitalTwin,
    HardwareInLoop,
    Real,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbodiedExperimentSession {
    pub experiment_id: String,
    pub project_id: String,
    pub owner_id: String,
    pub protocol_version: String,
    pub batch_identities: Vec<String>,
    pub sample_identities: Vec<String>,
    pub devices: Vec<String>,
    pub goals: Vec<String>,
    pub safety_constraints: Vec<SafetyConstraint>,
    pub planned_actions: Vec<PlannedAction>,
    pub acceptance_rules: Vec<String>,
    pub required_reviewers: Vec<String>,
    pub target_mode: TargetMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyConstraint {
    pub constraint_id: String,
    pub rule: String,
    pub severity: ConstraintSeverity,
    pub on_violation: ViolationAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConstraintSeverity { Warning, Critical, Fatal }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViolationAction { Log, Pause, EmergencyStop }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannedAction {
    pub action_id: String,
    pub device_id: String,
    pub command: String,
    pub parameters: std::collections::BTreeMap<String, String>,
    pub preconditions: Vec<String>,
    pub expected_observations: Vec<String>,
}

// ── Preflight + SafetyGuard (LS5-46) ──────────────────────────────
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreflightChecklist {
    pub checks: Vec<PreflightItem>,
    pub all_clear: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreflightItem {
    pub check_id: String,
    pub category: PreflightCategory,
    pub passed: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PreflightCategory {
    DeviceIdentity,
    CalibrationValid,
    SampleIdentity,
    CommandPlanHash,
    ParameterBounds,
    Interlock,
    EmergencyStop,
    OperatorPresence,
    RequiredReviewers,
    WorkspaceEgress,
    EstimatedDuration,
    WasteConsumableConstraints,
    TargetMode,
}

impl EmbodiedExperimentSession {
    /// Run preflight: all checks must pass before execution.
    pub fn preflight(&self, devices: &[DeviceDescriptor]) -> PreflightChecklist {
        let mut checks = Vec::new();
        for device_id in &self.devices {
            let found = devices.iter().any(|d| &d.device_id == device_id);
            checks.push(PreflightItem {
                check_id: format!("device-identity-{}", device_id),
                category: PreflightCategory::DeviceIdentity,
                passed: found,
                detail: if found { "device found".into() } else { "device not found".into() },
            });
        }
        if self.target_mode == TargetMode::Real && self.required_reviewers.is_empty() {
            checks.push(PreflightItem {
                check_id: "reviewers".into(),
                category: PreflightCategory::RequiredReviewers,
                passed: false,
                detail: "real target mode requires reviewers".into(),
            });
        }
        for constraint in &self.safety_constraints {
            checks.push(PreflightItem {
                check_id: format!("constraint-{}", constraint.constraint_id),
                category: PreflightCategory::Interlock,
                passed: !matches!(constraint.severity, ConstraintSeverity::Fatal),
                detail: format!("constraint: {}", constraint.rule),
            });
        }
        let all_clear = checks.iter().all(|c| c.passed);
        PreflightChecklist { checks, all_clear }
    }
}

// ── Dummy Lab (LS5-47) ────────────────────────────────────────────
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DummyLabConfig {
    pub enabled_devices: Vec<String>,
    pub fault_injection: FaultInjectionProfile,
    pub deterministic_seed: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaultInjectionProfile {
    pub sensor_drift_probability: f64,
    pub dropped_packet_probability: f64,
    pub stuck_actuator_probability: f64,
    pub delayed_response_probability: f64,
    pub calibration_expiry_probability: f64,
    pub emergency_stop_probability: f64,
    pub sample_mismatch_probability: f64,
    pub power_loss_probability: f64,
    pub partial_batch_probability: f64,
}

// ── Digital Twin (LS5-48) ─────────────────────────────────────────
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DigitalTwin {
    pub twin_id: String,
    pub model_identity: String,
    pub model_version: String,
    pub model_hash: String,
    pub initial_state: std::collections::BTreeMap<String, f64>,
    pub parameters: std::collections::BTreeMap<String, f64>,
    pub assumptions: Vec<String>,
    pub simulation_clock: f64,
    pub random_seed: u64,
    pub prediction_interval: Option<(f64, f64)>,
    pub known_limitations: Vec<String>,
}

// ── Control Policy Sandbox (LS5-49) ───────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ControlPhase {
    Observe,
    Recommend,
    Simulate,
    ProposeCommand,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlPolicy {
    pub allowed_phase: ControlPhase,
    pub requires_human_approval: bool,
    pub max_commands_per_session: u32,
    pub cooldown_ms: u64,
}

// ── CommandPlan (LS5-53) ──────────────────────────────────────────
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandPlan {
    pub plan_id: String,
    pub commands: Vec<DeviceCommand>,
    pub ordering: Vec<String>,
    pub timing: Vec<u64>,
    pub preconditions: Vec<String>,
    pub expected_observations: Vec<String>,
    pub abort_conditions: Vec<String>,
    pub safe_state: String,
    pub plan_sha256: String,
}

impl CommandPlan {
    pub fn compute_hash(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        for cmd in &self.commands { h.update(cmd.command.as_bytes()); }
        format!("{:x}", h.finalize())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCommand {
    pub command_id: String,
    pub device_id: String,
    pub command: String,
    pub parameters: std::collections::BTreeMap<String, f64>,
}

// ── Sensor Trust Chain (LS5-56) ───────────────────────────────────
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorObservation {
    pub observation_id: String,
    pub device_id: String,
    pub sensor_id: String,
    pub calibration_record: String,
    pub timestamp_source: String,
    pub sequence_number: u64,
    pub raw_payload_hash: String,
    pub parser_version: String,
    pub quality_flags: Vec<String>,
    pub value: f64,
    pub unit: String,
}

impl SensorObservation {
    pub fn trust_score(&self) -> f64 {
        let mut score: f64 = 1.0;
        if self.calibration_record.is_empty() { score -= 0.3; }
        if self.quality_flags.is_empty() { score -= 0.1; }
        if self.raw_payload_hash.len() < 8 { score -= 0.2; }
        score.max(0.0_f64)
    }
}

// ── Two-Phase Execution (LS5-54) ──────────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionPhase { Prepare, Commit, Aborted }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwoPhaseExecution {
    pub plan: CommandPlan,
    pub phase: ExecutionPhase,
    pub operator_present: bool,
    pub approval_valid: bool,
    pub plan_hash_unchanged: bool,
}

impl TwoPhaseExecution {
    pub fn can_commit(&self) -> bool {
        self.phase == ExecutionPhase::Prepare
            && self.operator_present
            && self.approval_valid
            && self.plan_hash_unchanged
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preflight_rejects_real_without_reviewers() {
        let session = EmbodiedExperimentSession {
            experiment_id: "e1".into(), project_id: "p1".into(), owner_id: "o1".into(),
            protocol_version: "1.0".into(), batch_identities: vec![], sample_identities: vec![],
            devices: vec![],
            goals: vec!["test".into()],
            safety_constraints: vec![],
            planned_actions: vec![],
            acceptance_rules: vec![],
            required_reviewers: vec![],
            target_mode: TargetMode::Real,
        };
        let checklist = session.preflight(&[]);
        assert!(!checklist.all_clear);
    }

    #[test]
    fn preflight_passes_dummy_mode() {
        let session = EmbodiedExperimentSession {
            experiment_id: "e1".into(), project_id: "p1".into(), owner_id: "o1".into(),
            protocol_version: "1.0".into(), batch_identities: vec![], sample_identities: vec![],
            devices: vec![],
            goals: vec!["test".into()],
            safety_constraints: vec![],
            planned_actions: vec![],
            acceptance_rules: vec![],
            required_reviewers: vec![],
            target_mode: TargetMode::Dummy,
        };
        let checklist = session.preflight(&[]);
        assert!(checklist.all_clear);
    }

    #[test]
    fn command_plan_hash_deterministic() {
        let plan = CommandPlan {
            plan_id: "p1".into(),
            commands: vec![DeviceCommand {
                command_id: "c1".into(), device_id: "d1".into(),
                command: "move".into(), parameters: Default::default(),
            }],
            ordering: vec!["c1".into()], timing: vec![100],
            preconditions: vec![], expected_observations: vec![],
            abort_conditions: vec![],
            safe_state: "home".into(), plan_sha256: String::new(),
        };
        let h1 = plan.compute_hash();
        let h2 = plan.compute_hash();
        assert_eq!(h1, h2);
    }

    #[test]
    fn sensor_trust_score_decays_without_calibration() {
        let obs = SensorObservation {
            observation_id: "o1".into(), device_id: "d1".into(), sensor_id: "s1".into(),
            calibration_record: String::new(), timestamp_source: "ntp".into(),
            sequence_number: 1, raw_payload_hash: "abc".into(),
            parser_version: "v1".into(), quality_flags: vec![],
            value: 25.0, unit: "C".into(),
        };
        assert!(obs.trust_score() < 1.0);
        assert!(obs.trust_score() >= 0.0);
    }

    #[test]
    fn two_phase_cannot_commit_without_operator() {
        let plan = CommandPlan {
            plan_id: "p1".into(), commands: vec![],
            ordering: vec![], timing: vec![],
            preconditions: vec![], expected_observations: vec![],
            abort_conditions: vec![], safe_state: "".into(),
            plan_sha256: String::new(),
        };
        let exec = TwoPhaseExecution {
            plan, phase: ExecutionPhase::Prepare,
            operator_present: false, approval_valid: true, plan_hash_unchanged: true,
        };
        assert!(!exec.can_commit());
    }

    #[test]
    fn fault_injection_profile_defaults() {
        let profile = FaultInjectionProfile {
            sensor_drift_probability: 0.01, dropped_packet_probability: 0.001,
            stuck_actuator_probability: 0.001, delayed_response_probability: 0.01,
            calibration_expiry_probability: 0.0, emergency_stop_probability: 0.0,
            sample_mismatch_probability: 0.001, power_loss_probability: 0.0001,
            partial_batch_probability: 0.01,
        };
        assert!(profile.sensor_drift_probability > 0.0);
        assert!(profile.power_loss_probability < 0.01);
    }
}
