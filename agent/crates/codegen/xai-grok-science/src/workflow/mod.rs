//! Declarative workflow specification and execution engine.
//! Seam contracts: LS5-15, LS5-16, LS5-17, LS5-18, LS5-19, LS5-20, LS5-21.

pub mod admission;
pub mod executor;
mod io;
pub mod kernel;
mod kernel_admission_protocol;
pub mod package;
mod pinned_executable;
pub mod python_runner;

#[cfg(test)]
mod e2e_tests;

pub use admission::{
    KernelAdmissionRequest, KernelPolicy, RejectionReason, default_resource_cap, probe_kernel,
    probe_pinned_kernel,
};
pub use executor::{
    ArtifactCommit, ArtifactCommitState, AttemptState, Clock, ErrorClass, ExecutionPolicy,
    KernelInvocation, ManualClock, RefusedStep, StepAttempt, StepFailure, StepOperation,
    StepOutput, StepPlan, StepRunner, SystemClock, UnboundStepRunner, WorkflowExecutionRequest,
    WorkflowExecutor, WorkflowOperationRecord, WorkflowRecoveryReport, WorkflowRunRecord,
    WorkflowRunReport, WorkflowState, run_id_for_operation,
};
pub use io::{
    AttemptOutputCapability, RetainedOutputDirectory, WorkflowChildPaths, WorkflowIoCapability,
    WorkflowOutputSnapshot,
};
pub use kernel::{
    AdmissionStatus, KernelAdmission, KernelKind, KernelManifest, ReproductionAttempt,
    ReproductionLevel, ReproductionResult, ResourceCap,
};
pub use kernel_admission_protocol::{
    KernelAdmissionResult, begin_kernel_admission, finish_kernel_admission,
};
pub use package::{ArtifactManifest, InputManifest, WorkflowPackage};
pub use pinned_executable::PinnedExecutable;
pub use python_runner::PythonLoopRunner;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::project::model::ProjectId;

// ── WorkflowSpec (LS5-15) ──────────────────────────────────────────

/// Step types allowed in a workflow. No arbitrary shell steps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum StepKind {
    ConnectorFetch,
    ArtifactTransform,
    NotebookCell,
    Renderer,
    Reviewer,
    HumanApproval,
    Export,
}

/// A single workflow step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStep {
    pub step_id: String,
    pub kind: StepKind,
    pub connector_id: Option<String>,
    pub notebook_cell: Option<String>,
    pub inputs: Vec<String>, // step_id references
    pub parameters: BTreeMap<String, String>,
    pub timeout_secs: u64,
    pub retry_policy: Option<RetryPolicy>,
    pub cache_policy: CachePolicy,
    pub acceptance_rules: Vec<AcceptanceRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub base_delay_ms: u64,
    pub backoff_multiplier: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CachePolicy {
    NoCache,
    LocalReuse,
    DeterministicReuse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceptanceRule {
    pub condition: AcceptanceCondition,
    pub on_fail: FailAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AcceptanceCondition {
    ExitCodeZero,
    OutputNotEmpty,
    ArtifactCount(usize),
    CustomRule(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FailAction {
    Abort,
    Retry,
    Skip,
    PauseForApproval,
}

/// Top-level workflow specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowSpec {
    pub workflow_id: String,
    pub project_id: ProjectId,
    pub name: String,
    pub steps: Vec<WorkflowStep>,
    pub parameters: BTreeMap<String, String>,
    pub permissions: Vec<String>,
    pub resources: ResourceLimits,
    pub schema_version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    pub max_concurrent_steps: u32,
    pub max_total_duration_secs: u64,
    pub max_memory_mb: u64,
    pub max_disk_mb: u64,
}

impl WorkflowSpec {
    pub const CURRENT_SCHEMA_VERSION: u32 = 1;

    /// Validate the workflow DAG is acyclic.
    pub fn validate_dag(&self) -> Result<(), String> {
        // Build adjacency
        let mut graph: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for step in &self.steps {
            graph.entry(&step.step_id).or_default();
            for input in &step.inputs {
                graph.entry(input).or_default().push(&step.step_id);
            }
        }

        // Simple DFS cycle detection
        let mut visited: BTreeMap<&str, u8> = BTreeMap::new(); // 0=white, 1=gray, 2=black
        for node in graph.keys() {
            if !visited.contains_key(node) {
                self.dfs_cycle_check(node, &graph, &mut visited)?;
            }
        }
        Ok(())
    }

    fn dfs_cycle_check<'a>(
        &self,
        node: &'a str,
        graph: &BTreeMap<&'a str, Vec<&'a str>>,
        visited: &mut BTreeMap<&'a str, u8>,
    ) -> Result<(), String> {
        visited.insert(node, 1); // gray
        if let Some(neighbors) = graph.get(node) {
            for neighbor in neighbors {
                match visited.get(neighbor).copied().unwrap_or(0) {
                    1 => return Err(format!("cycle detected at step: {node} → {neighbor}")),
                    2 => continue,
                    _ => self.dfs_cycle_check(neighbor, graph, visited)?,
                }
            }
        }
        visited.insert(node, 2); // black
        Ok(())
    }

    /// Step ids in a deterministic dependency-respecting order.
    ///
    /// Kahn's algorithm with a lexicographic tie-break, so two runs of the
    /// same spec visit steps in the same order and the attempt log of one run
    /// can be compared against another's. Fails on a duplicate step id, a
    /// dangling reference, or a cycle — a workflow that cannot be ordered
    /// cannot be executed.
    pub fn topological_order(&self) -> Result<Vec<String>, String> {
        self.validate_references()?;
        let mut pending: BTreeMap<&str, std::collections::BTreeSet<&str>> = BTreeMap::new();
        for step in &self.steps {
            if pending
                .insert(
                    step.step_id.as_str(),
                    step.inputs.iter().map(String::as_str).collect(),
                )
                .is_some()
            {
                return Err(format!("duplicate step id: {}", step.step_id));
            }
        }

        let mut order = Vec::with_capacity(self.steps.len());
        while !pending.is_empty() {
            // BTreeMap iteration is sorted, so the first ready step is always
            // the lexicographically smallest one.
            let Some(ready) = pending
                .iter()
                .find(|(_, inputs)| inputs.is_empty())
                .map(|(id, _)| *id)
            else {
                let stuck: Vec<&str> = pending.keys().copied().collect();
                return Err(format!("cycle detected among steps: {}", stuck.join(", ")));
            };
            pending.remove(ready);
            for inputs in pending.values_mut() {
                inputs.remove(ready);
            }
            order.push(ready.to_string());
        }
        Ok(order)
    }

    /// Verify all input references point to existing steps.
    pub fn validate_references(&self) -> Result<(), String> {
        let ids: Vec<&str> = self.steps.iter().map(|s| s.step_id.as_str()).collect();
        for step in &self.steps {
            for input in &step.inputs {
                if !ids.contains(&input.as_str()) {
                    return Err(format!(
                        "step '{}' references unknown input '{}'",
                        step.step_id, input
                    ));
                }
            }
        }
        Ok(())
    }
}

// ── Content-addressed reuse (LS5-17) ───────────────────────────────

/// Key for deterministic workflow step reuse.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReuseKey {
    pub input_artifact_hashes: BTreeMap<String, String>,
    pub step_implementation_version: String,
    pub parameters: BTreeMap<String, String>,
    pub compute_environment_hash: String,
    pub policy_version: String,
    pub connector_version: Option<String>,
    pub renderer_build_id: Option<String>,
}

impl ReuseKey {
    /// Compute a deterministic reuse key hash.
    ///
    /// DEFECT FIXED (LS5-K1): this used to hash only the input hashes, the
    /// implementation version and the environment hash — it ignored
    /// `parameters`, `policy_version`, `connector_version` and
    /// `renderer_build_id`, all of which [`ReuseKey::matches`] does compare.
    /// Two steps differing only in their parameters therefore produced the
    /// same key, and anything using this hash as a cache or commit address
    /// would have served one step's artifact for another's. Every field is
    /// now covered, length-prefixed so no two field boundaries can collide.
    pub fn compute_hash(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        let mut feed = |bytes: &[u8]| {
            hasher.update(bytes.len().to_le_bytes());
            hasher.update(bytes);
        };
        feed(b"reuse-key-v2");
        for (k, v) in &self.input_artifact_hashes {
            feed(k.as_bytes());
            feed(v.as_bytes());
        }
        feed(self.step_implementation_version.as_bytes());
        for (k, v) in &self.parameters {
            feed(k.as_bytes());
            feed(v.as_bytes());
        }
        feed(self.compute_environment_hash.as_bytes());
        feed(self.policy_version.as_bytes());
        feed(self.connector_version.as_deref().unwrap_or("").as_bytes());
        feed(self.renderer_build_id.as_deref().unwrap_or("").as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Check if a cached result can be reused. Must verify:
    /// 1. All input artifact hashes match
    /// 2. Step implementation version matches
    /// 3. Compute environment hash matches
    /// 4. Parameters are identical
    pub fn matches(&self, other: &ReuseKey) -> bool {
        self.input_artifact_hashes == other.input_artifact_hashes
            && self.step_implementation_version == other.step_implementation_version
            && self.compute_environment_hash == other.compute_environment_hash
            && self.parameters == other.parameters
            && self.policy_version == other.policy_version
            && self.connector_version == other.connector_version
            && self.renderer_build_id == other.renderer_build_id
    }
}

// ── ComputeEnvironment (LS5-18) ────────────────────────────────────

/// Immutable compute environment manifest for reproducibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputeEnvironment {
    pub environment_id: String,
    pub os: String,
    pub architecture: String,
    pub lumen_binary_hash: String,
    pub rust_lock_hash: Option<String>,
    pub python_hash: Option<String>,
    pub r_hash: Option<String>,
    pub julia_hash: Option<String>,
    pub dependency_lock_hash: String,
    pub locale: String,
    pub timezone: String,
    pub environment_allowlist: Vec<String>,
    pub cpu_identity: Option<String>,
    pub gpu_identity: Option<String>,
    pub deterministic_flags: Vec<String>,
    pub network_policy: NetworkPolicy,
    pub container_digest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NetworkPolicy {
    None,
    Allowlisted(Vec<String>),
    FullAccess,
}

impl ComputeEnvironment {
    /// Compute environment identity hash.
    pub fn identity_hash(&self) -> String {
        use sha2::{Digest, Sha256};
        // This hash is an authority and replay boundary, not a display label.
        // Bind every serialized field so a caller cannot reuse one operation
        // id after changing an interpreter, dependency, locale, deterministic
        // flag, hardware identity or network policy.
        let bytes = serde_json::to_vec(self)
            .expect("ComputeEnvironment contains only infallibly serializable fields");
        format!("{:x}", Sha256::digest(bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_spec() -> WorkflowSpec {
        WorkflowSpec {
            workflow_id: "wf-test".into(),
            project_id: ProjectId("proj".into()),
            name: "Test Workflow".into(),
            steps: vec![
                WorkflowStep {
                    step_id: "fetch".into(),
                    kind: StepKind::ConnectorFetch,
                    connector_id: Some("pubmed".into()),
                    notebook_cell: None,
                    inputs: vec![],
                    parameters: BTreeMap::new(),
                    timeout_secs: 60,
                    retry_policy: None,
                    cache_policy: CachePolicy::DeterministicReuse,
                    acceptance_rules: vec![],
                },
                WorkflowStep {
                    step_id: "analyze".into(),
                    kind: StepKind::NotebookCell,
                    connector_id: None,
                    notebook_cell: Some("cell-1".into()),
                    inputs: vec!["fetch".into()],
                    parameters: BTreeMap::new(),
                    timeout_secs: 120,
                    retry_policy: None,
                    cache_policy: CachePolicy::NoCache,
                    acceptance_rules: vec![],
                },
            ],
            parameters: BTreeMap::new(),
            permissions: vec![],
            resources: ResourceLimits {
                max_concurrent_steps: 2,
                max_total_duration_secs: 3600,
                max_memory_mb: 4096,
                max_disk_mb: 10240,
            },
            schema_version: 1,
        }
    }

    #[test]
    fn valid_dag_passes_validation() {
        let spec = sample_spec();
        assert!(spec.validate_dag().is_ok());
    }

    #[test]
    fn invalid_input_reference_fails() {
        let mut spec = sample_spec();
        spec.steps[1].inputs = vec!["nonexistent".into()];
        assert!(spec.validate_references().is_err());
    }

    #[test]
    fn cycle_detection() {
        let mut spec = sample_spec();
        spec.steps[0].inputs = vec!["analyze".into()];
        assert!(spec.validate_dag().is_err());
    }

    #[test]
    fn reuse_key_matching() {
        let mut input_hashes = BTreeMap::new();
        input_hashes.insert("a".to_string(), "sha:1".to_string());
        let k1 = ReuseKey {
            input_artifact_hashes: input_hashes,
            step_implementation_version: "v1".into(),
            parameters: BTreeMap::new(),
            compute_environment_hash: "env:1".into(),
            policy_version: "p1".into(),
            connector_version: None,
            renderer_build_id: None,
        };
        let k2 = k1.clone();
        assert!(k1.matches(&k2));
        assert_eq!(k1.compute_hash(), k2.compute_hash());
    }

    #[test]
    fn compute_environment_identity() {
        let env = ComputeEnvironment {
            environment_id: "env-1".into(),
            os: "windows".into(),
            architecture: "x86_64".into(),
            lumen_binary_hash: "lumen:1".into(),
            rust_lock_hash: None,
            python_hash: Some("py:3.12".into()),
            r_hash: None,
            julia_hash: None,
            dependency_lock_hash: "deps:1".into(),
            locale: "en-US".into(),
            timezone: "UTC".into(),
            environment_allowlist: vec![],
            cpu_identity: None,
            gpu_identity: None,
            deterministic_flags: vec![],
            network_policy: NetworkPolicy::None,
            container_digest: None,
        };
        let h = env.identity_hash();
        assert!(!h.is_empty());

        let mut changed = env.clone();
        changed
            .environment_allowlist
            .push("different-kernel".into());
        assert_ne!(h, changed.identity_hash());
    }
}
