//! Resource budgets, scale testing, fault injection, data governance, model governance.
//! Seam: LS5-60~LS5-66. Pure specs — production deployment needed for real scale tests.

use serde::{Deserialize, Serialize};

// ── Resource Budgets (LS5-60) ─────────────────────────────────────
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceBudget {
    pub budget_id: String,
    pub thresholds: std::collections::BTreeMap<String, BudgetThreshold>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetThreshold {
    pub metric: String,
    pub warn_at: f64,
    pub error_at: f64,
    pub unit: String,
}

impl ResourceBudget {
    pub fn check(&self, metric: &str, value: f64) -> BudgetStatus {
        if let Some(threshold) = self.thresholds.get(metric) {
            if value > threshold.error_at { return BudgetStatus::Exceeded; }
            if value > threshold.warn_at { return BudgetStatus::Warning; }
        }
        BudgetStatus::Ok
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BudgetStatus { Ok, Warning, Exceeded }

// ── Scale Test Spec (LS5-61) ──────────────────────────────────────
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScaleTestSpec {
    pub projects: u32,
    pub runs: u32,
    pub artifacts: u32,
    pub evidence_edges: u32,
    pub concurrent_workflows: u32,
    pub remote_jobs: u32,
    pub device_sessions: u32,
    pub non_device_soak_days: u32,
    pub supervised_device_soak_hours: u32,
}

// ── Fault Injection (LS5-62) ──────────────────────────────────────
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaultInjectionScenario {
    pub scenario_id: String,
    pub fault_type: FaultType,
    pub target_component: String,
    pub injection_point: String,
    pub expected_behavior: ExpectedBehavior,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FaultType {
    DiskFull,
    EventLogCorruption,
    PowerLoss,
    ClockJump,
    PermissionServiceUnavailable,
    ReviewerTimeout,
    KernelCrashLoop,
    SchedulerLostJob,
    SensorFlood,
    DuplicateCommandAck,
    DeviceDisconnect,
    EmergencyStopRace,
    PartialMigration,
    ExpiredSigningKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExpectedBehavior { FailClosed, GracefulDegradation, AutoRecovery, ManualIntervention }

// ── Data Governance (LS5-63) ──────────────────────────────────────
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataClassification {
    pub level: ClassificationLevel,
    pub retention_days: u32,
    pub export_allowed: bool,
    pub requires_redaction: bool,
    pub pii_phi_warning: bool,
    pub legal_hold: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ClassificationLevel { Public, Internal, Confidential, Restricted }

impl DataClassification {
    pub fn can_export(&self) -> bool { self.export_allowed && self.level <= ClassificationLevel::Confidential }
    pub fn requires_audit(&self) -> bool { self.level >= ClassificationLevel::Internal }
}

// ── Model Governance (LS5-64) ─────────────────────────────────────
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCallRecord {
    pub call_id: String,
    pub provider: String,
    pub model: String,
    pub request_policy: String,
    pub prompt_template_hash: String,
    pub input_artifact_refs: Vec<String>,
    pub redaction_result: RedactionResult,
    pub tool_permissions: Vec<String>,
    pub usage_tokens: u64,
    pub provider_cache_truth: Option<bool>,
    pub response_hash: String,
    pub review_status: ModelReviewStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RedactionResult { Clean, PiiDetected, PhiDetected, CredentialLeak }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelReviewStatus { Pending, Reviewed, Escalated, BaselineReset }

impl ModelCallRecord {
    pub fn is_safe(&self) -> bool {
        self.redaction_result == RedactionResult::Clean
            && !self.input_artifact_refs.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_check_warn_and_exceed() {
        let mut thresholds = std::collections::BTreeMap::new();
        thresholds.insert("latency_ms".into(), BudgetThreshold {
            metric: "latency_ms".into(), warn_at: 100.0, error_at: 500.0, unit: "ms".into() });
        let budget = ResourceBudget { budget_id: "b1".into(), thresholds };
        assert_eq!(budget.check("latency_ms", 50.0), BudgetStatus::Ok);
        assert_eq!(budget.check("latency_ms", 200.0), BudgetStatus::Warning);
        assert_eq!(budget.check("latency_ms", 600.0), BudgetStatus::Exceeded);
    }

    #[test]
    fn data_classification_export_rules() {
        let pub_data = DataClassification { level: ClassificationLevel::Public, retention_days: 365, export_allowed: true, requires_redaction: false, pii_phi_warning: false, legal_hold: false };
        assert!(pub_data.can_export());
        let restricted = DataClassification { level: ClassificationLevel::Restricted, retention_days: 90, export_allowed: false, requires_redaction: true, pii_phi_warning: true, legal_hold: false };
        assert!(!restricted.can_export());
        assert!(restricted.requires_audit());
    }

    #[test]
    fn model_call_safety_check() {
        let call = ModelCallRecord {
            call_id: "c1".into(), provider: "deepseek".into(), model: "v4".into(),
            request_policy: "default".into(), prompt_template_hash: "abc".into(),
            input_artifact_refs: vec!["a1".into()], redaction_result: RedactionResult::Clean,
            tool_permissions: vec!["pubmed".into()], usage_tokens: 1000,
            provider_cache_truth: None, response_hash: "resp:1".into(),
            review_status: ModelReviewStatus::Pending,
        };
        assert!(call.is_safe());
    }

    #[test]
    fn model_call_unsafe_with_credential_leak() {
        let call = ModelCallRecord {
            call_id: "c1".into(), provider: "deepseek".into(), model: "v4".into(),
            request_policy: "default".into(), prompt_template_hash: "abc".into(),
            input_artifact_refs: vec![], redaction_result: RedactionResult::CredentialLeak,
            tool_permissions: vec![], usage_tokens: 1000,
            provider_cache_truth: None, response_hash: "resp:1".into(),
            review_status: ModelReviewStatus::Pending,
        };
        assert!(!call.is_safe());
    }
}
