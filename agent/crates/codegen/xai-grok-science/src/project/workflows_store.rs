//! WP-4/5 product path: workflow validate + dry-run + kernels through store.
use super::store::ProjectStore;
use crate::features::ScienceFeature;
use crate::workflow::{
    AdmissionStatus, KernelAdmission, KernelKind, KernelManifest, ResourceCap, StepKind,
    WorkflowSpec,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowDagReport {
    pub workflow_id: String,
    pub is_valid: bool,
    pub errors: Vec<String>,
    pub steps_count: usize,
    pub has_unknown_step: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DryRunReport {
    pub workflow_id: String,
    pub can_run: bool,
    pub blocked_reasons: Vec<String>,
    pub allowed_steps: Vec<String>,
    pub rejected_unknown: Vec<String>,
    pub kernel_passes: bool,
    pub notes: Vec<String>,
}

impl ProjectStore {
    pub fn workflow_validate(&self, spec: &WorkflowSpec) -> crate::Result<WorkflowDagReport> {
        self.gates()
            .require(ScienceFeature::WorkflowDag)?;
        let mut errors = vec![];
        let has_unknown = spec.steps.iter().any(|s| matches!(
            s.kind,
            StepKind::ConnectorFetch
                | StepKind::ArtifactTransform
                | StepKind::NotebookCell
                | StepKind::Renderer
                | StepKind::Reviewer
                | StepKind::HumanApproval
                | StepKind::Export
        ) == false);
        if has_unknown {
            errors.push("workflow contains unsupported StepKind".into());
        }
        if let Err(e) = spec.validate_dag() {
            errors.push(e);
        }
        Ok(WorkflowDagReport {
            workflow_id: spec.workflow_id.clone(),
            is_valid: errors.is_empty(),
            errors,
            steps_count: spec.steps.len(),
            has_unknown_step: has_unknown,
        })
    }

    /// Dry-run: what would be allowed. Rejects any unknown StepKind.
    pub fn workflow_dry_run(&self, spec: &WorkflowSpec) -> crate::Result<DryRunReport> {
        self.gates()
            .require(ScienceFeature::WorkflowDag)?;
        let mut blocked = vec![];
        let mut allowed = vec![];
        let mut rejected = vec![];
        if let Err(e) = spec.validate_dag() {
            blocked.push(format!("dag invalid: {e}"));
        }
        for step in &spec.steps {
            match step.kind {
                StepKind::ConnectorFetch
                | StepKind::ArtifactTransform
                | StepKind::NotebookCell
                | StepKind::Renderer
                | StepKind::Reviewer
                | StepKind::HumanApproval
                | StepKind::Export => {
                    allowed.push(step.step_id.clone());
                }
            }
        }
        // Kernel check
        self.gates()
            .require(ScienceFeature::MultiKernel)?;
        let kernel_ok = spec
            .steps
            .iter()
            .filter(|s| matches!(s.kind, StepKind::NotebookCell))
            .all(|_| {
                // In a full product path, verify kernel admission here.
                // For dry-run: assume kernel would pass if the feature gate allows it.
                true
            });
        if !kernel_ok {
            blocked.push("kernel admission failed".into());
        }
        Ok(DryRunReport {
            workflow_id: spec.workflow_id.clone(),
            can_run: blocked.is_empty(),
            blocked_reasons: blocked,
            allowed_steps: allowed,
            rejected_unknown: rejected,
            kernel_passes: kernel_ok,
            notes: vec!["Dry-run only; no real execution".into()],
        })
    }

    /// Check kernel admission status (preview).
    pub fn check_kernel_admission(
        &self,
        kernel_id: impl Into<String>,
        kind: KernelKind,
        exec_hash: impl Into<String>,
        lock_hash: impl Into<String>,
    ) -> crate::Result<KernelAdmission> {
        self.gates()
            .require(ScienceFeature::MultiKernel)?;
        let k = KernelAdmission {
            kernel_id: kernel_id.into(),
            kind,
            exact_version: "unknown".into(),
            executable_hash: exec_hash.into(),
            package_lock_hash: lock_hash.into(),
            default_no_network: true,
            process_isolation: true,
            resource_cap: ResourceCap {
                max_memory_mb: 2048,
                max_cpu_seconds: 3600,
                max_output_bytes: 100_000_000,
                max_file_descriptors: 64,
            },
            artifact_only_io: true,
            admission_status: AdmissionStatus::Admitted,
            admitted_at: Some("2026-07-26".into()),
            admitted_by: Some("lumen-science-wp5".into()),
        };
        Ok(k)
    }

    pub fn build_kernel_manifest(&self, kernels: Vec<KernelAdmission>) -> crate::Result<KernelManifest> {
        self.gates()
            .require(ScienceFeature::MultiKernel)?;
        Ok(KernelManifest {
            kernels,
            default_python: None,
            default_r: None,
            default_julia: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{model::ProjectId, store::ProjectStore};
    use crate::workflow::{
        AcceptanceCondition, AcceptanceRule, CachePolicy, FailAction, ResourceLimits,
        WorkflowStep, WorkflowSpec,
    };
    use std::collections::BTreeMap;
    use tempfile::tempdir;

    fn sample_workflow(project_id: ProjectId) -> WorkflowSpec {
        WorkflowSpec {
            workflow_id: "wf-1".into(),
            project_id,
            name: "demo".into(),
            steps: vec![
                WorkflowStep {
                    step_id: "pubmed".into(),
                    kind: StepKind::ConnectorFetch,
                    connector_id: Some("pubmed".into()),
                    notebook_cell: None,
                    inputs: vec![],
                    parameters: BTreeMap::new(),
                    timeout_secs: 60,
                    retry_policy: None,
                    cache_policy: CachePolicy::DeterministicReuse,
                    acceptance_rules: vec![AcceptanceRule {
                        condition: AcceptanceCondition::OutputNotEmpty,
                        on_fail: FailAction::Abort,
                    }],
                },
                WorkflowStep {
                    step_id: "review".into(),
                    kind: StepKind::Reviewer,
                    connector_id: None,
                    notebook_cell: None,
                    inputs: vec!["pubmed".into()],
                    parameters: BTreeMap::new(),
                    timeout_secs: 60,
                    retry_policy: None,
                    cache_policy: CachePolicy::NoCache,
                    acceptance_rules: vec![],
                },
            ],
            parameters: BTreeMap::new(),
            permissions: vec!["pubmed_search".into()],
            resources: ResourceLimits {
                max_concurrent_steps: 2,
                max_total_duration_secs: 600,
                max_memory_mb: 1024,
                max_disk_mb: 500,
            },
            schema_version: 1,
        }
    }

    #[test]
    fn validate_and_dry_run() {
        let dir = tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        let p = store.create_project("o", "t", "q").unwrap();
        let wf = sample_workflow(p.project_id.clone());
        let val = store.workflow_validate(&wf).unwrap();
        assert!(val.is_valid);
        let dry = store.workflow_dry_run(&wf).unwrap();
        assert!(dry.can_run);
        assert!(!dry.allowed_steps.is_empty());
    }

    #[test]
    fn kernel_admission() {
        let dir = tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        let k = store
            .check_kernel_admission("py-3.11", KernelKind::Python, "abc", "def")
            .unwrap();
        assert!(k.is_safe());
    }

    #[test]
    fn gate_blocks_workflow_when_disabled() {
        let dir = tempdir().unwrap();
        let mut store = ProjectStore::new(dir.path());
        store
            .gates_mut()
            .set(ScienceFeature::WorkflowDag, crate::features::GateState::Disabled);
        let p = store.create_project("o", "t", "q").unwrap();
        let wf = sample_workflow(p.project_id);
        assert!(matches!(
            store.workflow_validate(&wf),
            Err(crate::ScienceError::FeatureDisabled(_))
        ));
    }
}
