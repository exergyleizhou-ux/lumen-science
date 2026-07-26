//! WP-4/5 product path: workflow validate + dry-run + kernels through store.
use super::store::ProjectStore;
use crate::features::ScienceFeature;
use crate::workflow::{
    KernelAdmission, KernelAdmissionRequest, KernelKind, KernelManifest, StepKind, WorkflowSpec,
    probe_kernel,
};
use serde::{Deserialize, Serialize};

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
        let has_unknown = spec.steps.iter().any(|s| !matches!(
            s.kind,
            StepKind::ConnectorFetch
                | StepKind::ArtifactTransform
                | StepKind::NotebookCell
                | StepKind::Renderer
                | StepKind::Reviewer
                | StepKind::HumanApproval
                | StepKind::Export
        ));
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

    /// Dry-run: what would be allowed.
    ///
    /// DEFECT FIXED (LS5-K1): the kernel branch used to be
    /// `.all(|_| true)` with the comment "assume kernel would pass", so a
    /// workflow full of `NotebookCell` steps reported `kernelPasses: true`
    /// with no kernel anywhere. `kernels` is now the manifest the run would
    /// actually use; `None` means no kernel is available, and a workflow with
    /// a kernel step is blocked.
    pub fn workflow_dry_run(
        &self,
        spec: &WorkflowSpec,
        kernels: Option<&KernelManifest>,
    ) -> crate::Result<DryRunReport> {
        self.gates().require(ScienceFeature::WorkflowDag)?;
        let mut blocked = vec![];
        let mut allowed = vec![];
        let rejected = vec![];
        if let Err(e) = spec.validate_dag() {
            blocked.push(format!("dag invalid: {e}"));
        }
        if let Err(e) = spec.validate_references() {
            blocked.push(format!("references invalid: {e}"));
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
        // Kernel check, against the manifest that would really be used.
        self.gates().require(ScienceFeature::MultiKernel)?;
        let kernel_steps: Vec<&str> = spec
            .steps
            .iter()
            .filter(|s| matches!(s.kind, StepKind::NotebookCell))
            .map(|s| s.step_id.as_str())
            .collect();
        let kernel_passes = if kernel_steps.is_empty() {
            true
        } else {
            match kernels.and_then(|m| m.find_admitted(KernelKind::Python)) {
                Some(_) => true,
                None => {
                    blocked.push(format!(
                        "no admitted kernel for notebook steps: {}",
                        kernel_steps.join(", ")
                    ));
                    false
                }
            }
        };
        Ok(DryRunReport {
            workflow_id: spec.workflow_id.clone(),
            can_run: blocked.is_empty(),
            blocked_reasons: blocked,
            allowed_steps: allowed,
            rejected_unknown: rejected,
            kernel_passes,
            notes: vec![
                "Dry-run only; no step was executed. Use WorkflowExecutor to run the spec.".into(),
            ],
        })
    }

    /// Admit a kernel by probing it.
    ///
    /// DEFECT FIXED (LS5-K1): this used to fabricate the whole record. It
    /// never touched an interpreter — it wrote `exact_version: "unknown"`,
    /// hardcoded `admitted_at`/`admitted_by`, copied the caller's
    /// `exec_hash`/`lock_hash` into the record as if verified, and returned
    /// `AdmissionStatus::Admitted` with no path to a rejection. It is reachable
    /// over ACP, so the product answered "admitted" for kernels that do not
    /// exist. It now delegates to [`probe_kernel`], which resolves and hashes
    /// the executable, runs it under a timeout to read its real version, and
    /// rejects with a reason.
    pub fn check_kernel_admission(
        &self,
        request: &KernelAdmissionRequest,
    ) -> crate::Result<KernelAdmission> {
        self.gates().require(ScienceFeature::MultiKernel)?;
        probe_kernel(request)
    }

    /// Build a manifest of admitted kernels.
    ///
    /// Rejected records are refused rather than filed: a manifest is the list
    /// of kernels a workflow may use, so a rejection has no place in it.
    pub fn build_kernel_manifest(
        &self,
        kernels: Vec<KernelAdmission>,
    ) -> crate::Result<KernelManifest> {
        self.gates().require(ScienceFeature::MultiKernel)?;
        for kernel in &kernels {
            if !kernel.is_safe() {
                return Err(crate::ScienceError::Invalid(format!(
                    "kernel '{}' is not admitted: {}",
                    kernel.kernel_id,
                    kernel
                        .rejection_reason
                        .as_ref()
                        .map(|reason| reason.to_string())
                        .unwrap_or_else(|| format!("status {:?}", kernel.admission_status))
                )));
            }
        }
        let default_for = |kind: KernelKind| {
            kernels
                .iter()
                .find(|kernel| kernel.kind == kind)
                .map(|kernel| kernel.kernel_id.clone())
        };
        Ok(KernelManifest {
            default_python: default_for(KernelKind::Python),
            default_r: default_for(KernelKind::R),
            default_julia: default_for(KernelKind::Julia),
            kernels,
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
        let dry = store.workflow_dry_run(&wf, None).unwrap();
        assert!(dry.can_run);
        assert!(!dry.allowed_steps.is_empty());
    }

    /// The old dry-run hardcoded `kernel_passes = true`. A notebook step with
    /// no kernel manifest must block.
    #[test]
    fn dry_run_blocks_a_notebook_step_with_no_admitted_kernel() {
        use crate::workflow::{CachePolicy, WorkflowStep};
        let dir = tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        let p = store.create_project("o", "t", "q").unwrap();
        let mut wf = sample_workflow(p.project_id);
        wf.steps.push(WorkflowStep {
            step_id: "notebook".into(),
            kind: StepKind::NotebookCell,
            connector_id: None,
            notebook_cell: Some("cell-1".into()),
            inputs: vec!["pubmed".into()],
            parameters: BTreeMap::new(),
            timeout_secs: 60,
            retry_policy: None,
            cache_policy: CachePolicy::NoCache,
            acceptance_rules: vec![],
        });
        let dry = store.workflow_dry_run(&wf, None).unwrap();
        assert!(!dry.kernel_passes);
        assert!(!dry.can_run);
        assert!(
            dry.blocked_reasons
                .iter()
                .any(|r| r.contains("no admitted kernel")),
            "{:?}",
            dry.blocked_reasons
        );
    }

    /// A kernel that does not exist is rejected, not admitted. The old
    /// implementation returned `Admitted` here without looking at anything.
    #[test]
    fn kernel_admission_rejects_a_kernel_that_is_not_there() {
        let dir = tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        let admission = store
            .check_kernel_admission(
                &KernelAdmissionRequest::new(
                    "py-3.11",
                    KernelKind::Python,
                    dir.path().join("nope/python3"),
                )
                .with_admitted_by("test"),
            )
            .unwrap();
        assert!(!admission.is_safe());
        assert_eq!(
            admission.admission_status,
            crate::workflow::AdmissionStatus::Rejected
        );
        assert!(admission.rejection_reason.is_some());
        assert!(admission.admitted_at.is_none());
    }

    #[cfg(unix)]
    #[test]
    fn kernel_admission_admits_a_real_interpreter_and_builds_a_manifest() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        let exe = dir.path().join("python3");
        std::fs::write(&exe, "#!/bin/sh\necho 'Python 3.11.9'\n").unwrap();
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();

        let admission = store
            .check_kernel_admission(
                &KernelAdmissionRequest::new("py-3.11", KernelKind::Python, &exe)
                    .with_admitted_by("test"),
            )
            .unwrap();
        assert!(admission.is_safe());
        assert_eq!(admission.exact_version, "Python 3.11.9");

        let manifest = store.build_kernel_manifest(vec![admission]).unwrap();
        assert_eq!(manifest.default_python.as_deref(), Some("py-3.11"));
        assert!(manifest.find_admitted(KernelKind::Python).is_some());
    }

    /// A rejected kernel must never reach a manifest a workflow could run.
    #[test]
    fn build_kernel_manifest_refuses_a_rejected_kernel() {
        let dir = tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        let rejected = store
            .check_kernel_admission(
                &KernelAdmissionRequest::new("ghost", KernelKind::Python, "python3")
                    .with_admitted_by("test"),
            )
            .unwrap();
        let error = store.build_kernel_manifest(vec![rejected]).unwrap_err();
        assert!(
            matches!(&error, crate::ScienceError::Invalid(m) if m.contains("not admitted")),
            "unexpected: {error}"
        );
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
