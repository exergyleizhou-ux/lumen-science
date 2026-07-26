//! Workflow built-binary e2e test (offline fixture-only).
//! Seam contract: LS5-22.
//!
//! Validates the complete workflow lifecycle using deterministic fixtures:
//! DAG validation → step reference check → reuse key computation →
//! compute environment identity → package integrity.
//!
//! Built-binary proof deferred: this module validates the data-model
//! contract that a built Lumen binary would exercise through SessionActor.

use std::collections::BTreeMap;

use crate::project::model::ProjectId;
use crate::workflow::{
    AcceptanceCondition, AcceptanceRule, CachePolicy, ComputeEnvironment,
    FailAction, NetworkPolicy, ReuseKey, ResourceLimits, StepKind,
    WorkflowSpec, WorkflowStep, WorkflowPackage, InputManifest,
    ArtifactManifest, KernelAdmission, KernelKind, KernelManifest,
    ResourceCap, AdmissionStatus, ReproductionLevel,
};

/// Build a representative 3-step pubmed→notebook→review workflow.
fn build_workflow_spec() -> WorkflowSpec {
    WorkflowSpec {
        workflow_id: "wf-e2e-test".into(),
        project_id: ProjectId("e2e-proj".into()),
        name: "E2E PubMed Analysis".into(),
        steps: vec![
            WorkflowStep {
                step_id: "pubmed_fetch".into(),
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
                step_id: "notebook_analysis".into(),
                kind: StepKind::NotebookCell,
                connector_id: None,
                notebook_cell: Some("cell-1".into()),
                inputs: vec!["pubmed_fetch".into()],
                parameters: {
                    let mut p = BTreeMap::new();
                    p.insert("significance".into(), "0.05".into());
                    p
                },
                timeout_secs: 120,
                retry_policy: None,
                cache_policy: CachePolicy::NoCache,
                acceptance_rules: vec![AcceptanceRule {
                    condition: AcceptanceCondition::ExitCodeZero,
                    on_fail: FailAction::PauseForApproval,
                }],
            },
            WorkflowStep {
                step_id: "reviewer_check".into(),
                kind: StepKind::Reviewer,
                connector_id: None,
                notebook_cell: None,
                inputs: vec!["notebook_analysis".into()],
                parameters: BTreeMap::new(),
                timeout_secs: 60,
                retry_policy: None,
                cache_policy: CachePolicy::NoCache,
                acceptance_rules: vec![],
            },
        ],
        parameters: BTreeMap::new(),
        permissions: vec!["pubmed_search".into(), "notebook_execute".into()],
        resources: ResourceLimits {
            max_concurrent_steps: 2,
            max_total_duration_secs: 600,
            max_memory_mb: 4096,
            max_disk_mb: 1024,
        },
        schema_version: 1,
    }
}

fn build_workflow_package() -> WorkflowPackage {
    let mut inputs = BTreeMap::new();
    inputs.insert("pubmed_results.json".into(), "sha256:abc123".into());
    inputs.insert("notebook_output.csv".into(), "sha256:def456".into());

    let mut outputs = BTreeMap::new();
    outputs.insert("pubmed_results.json".into(), "sha256:abc123".into());
    outputs.insert("notebook_output.csv".into(), "sha256:def456".into());
    outputs.insert("review_verdict.json".into(), "sha256:ghi789".into());

    WorkflowPackage {
        package_id: "pkg-e2e".into(),
        workflow_id: "wf-e2e-test".into(),
        created_at: "2026-07-25".into(),
        schema_version: 1,
        workflow_json: serde_json::to_string(&build_workflow_spec()).unwrap(),
        inputs_manifest: InputManifest { files: inputs, total_bytes: 1024 },
        artifacts_manifest: ArtifactManifest { files: outputs, total_artifacts: 3 },
        evidence_graph_json: None,
        events_jsonl: "[]".into(),
        licenses: vec!["MIT".into()],
        signature: None,
        environment_hash: "env:e2e".into(),
    }
}

fn build_kernel_manifest() -> KernelManifest {
    KernelManifest {
        kernels: vec![
            KernelAdmission {
                kernel_id: "py-3.12-e2e".into(),
                kind: KernelKind::Python,
                exact_version: "3.12.4".into(),
                executable_hash: "sha256:py312".into(),
                package_lock_hash: "sha256:pkg".into(),
                interpreter_path: "/usr/bin/python3.12".into(),
                os: "linux".into(),
                architecture: "x86_64".into(),
                default_no_network: true,
                process_isolation: true,
                resource_cap: ResourceCap {
                    max_memory_mb: 4096,
                    max_cpu_seconds: 300,
                    max_output_bytes: 10_485_760,
                    max_file_descriptors: 64,
                },
                artifact_only_io: true,
                admission_status: AdmissionStatus::Admitted,
                rejection_reason: None,
                admitted_at: Some("2026-07-25".into()),
                admitted_by: Some("e2e-test".into()),
            },
        ],
        default_python: Some("py-3.12-e2e".into()),
        default_r: None,
        default_julia: None,
    }
}

// ── E2E-level validations ──────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// LS5-22: Validate a complete 3-step workflow DAG end-to-end.
    #[test]
    fn e2e_workflow_dag_is_valid() {
        let spec = build_workflow_spec();
        assert_eq!(spec.steps.len(), 3);
        spec.validate_dag().expect("DAG must be acyclic");
        spec.validate_references().expect("all refs must exist");
    }

    /// LS5-22: Verify that step references form the correct chain.
    #[test]
    fn e2e_step_reference_chain() {
        let spec = build_workflow_spec();
        assert!(spec.steps[0].inputs.is_empty(), "pubmed_fetch has no inputs");
        assert_eq!(spec.steps[1].inputs[0], "pubmed_fetch");
        assert_eq!(spec.steps[2].inputs[0], "notebook_analysis");
    }

    /// LS5-22: Reuse key must be deterministic for same inputs.
    #[test]
    fn e2e_reuse_key_deterministic() {
        let mut hashes = BTreeMap::new();
        hashes.insert("a".into(), "sha:1".into());
        let k1 = ReuseKey {
            input_artifact_hashes: hashes.clone(),
            step_implementation_version: "v1".into(),
            parameters: BTreeMap::new(),
            compute_environment_hash: "env:1".into(),
            policy_version: "p1".into(),
            connector_version: None,
            renderer_build_id: None,
        };
        assert_eq!(k1.compute_hash(), k1.compute_hash());
    }

    /// LS5-22: Workflow package integrity across all artifacts.
    #[test]
    fn e2e_package_integrity() {
        let pkg = build_workflow_package();
        pkg.verify_integrity().expect("package hashes must match");
    }

    /// LS5-22: Kernel manifest guarantees safe execution.
    #[test]
    fn e2e_kernel_safety() {
        let manifest = build_kernel_manifest();
        let py = manifest.find_admitted(KernelKind::Python).expect("python kernel must be admitted");
        assert!(py.is_safe());
        assert!(py.default_no_network);
        assert!(py.artifact_only_io);
    }

    /// LS5-22: Reproduction levels never allow live providers.
    #[test]
    fn e2e_reproduction_safety() {
        for level in &[ReproductionLevel::R1ReplayOnly, ReproductionLevel::R2Deterministic, ReproductionLevel::R3Independent] {
            assert!(!level.allows_live_providers(), "{:?} must never allow live providers", level);
        }
    }

    /// LS5-22: Cycle detection catches a 2-node loop.
    #[test]
    fn e2e_cycle_detection() {
        let mut spec = build_workflow_spec();
        spec.steps[0].inputs = vec!["reviewer_check".into()];
        assert!(spec.validate_dag().is_err(), "cycle must be rejected");
    }

    /// LS5-22: Compute environment identity is stable.
    #[test]
    fn e2e_compute_environment_stable() {
        let env1 = ComputeEnvironment {
            environment_id: "e2e".into(),
            os: "windows".into(),
            architecture: "x86_64".into(),
            lumen_binary_hash: "lumen:abc".into(),
            rust_lock_hash: None,
            python_hash: Some("py:3.12".into()),
            r_hash: None,
            julia_hash: None,
            dependency_lock_hash: "deps:1".into(),
            locale: "en-US".into(),
            timezone: "UTC".into(),
            environment_allowlist: vec!["pubmed".into()],
            cpu_identity: None,
            gpu_identity: None,
            deterministic_flags: vec![],
            network_policy: NetworkPolicy::Allowlisted(vec!["eutils.ncbi.nlm.nih.gov".into()]),
            container_digest: None,
        };
        let h1 = env1.identity_hash();
        let h2 = env1.identity_hash();
        assert_eq!(h1, h2);
        assert!(!h1.is_empty());
    }
}
