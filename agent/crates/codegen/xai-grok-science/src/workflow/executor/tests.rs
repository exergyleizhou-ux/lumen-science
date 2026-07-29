//! Executor tests. Every one of these runs on a [`ManualClock`]: no test
//! here waits on wall time, so a retry schedule is asserted rather than slept
//! through.

use super::*;
use crate::workflow::{
    AcceptanceCondition, AcceptanceRule, NetworkPolicy, ResourceLimits, RetryPolicy,
};
use std::sync::{Barrier, atomic::AtomicU32};
use tempfile::{TempDir, tempdir};

// ── Fixtures ──────────────────────────────────────────────────────

fn environment() -> ComputeEnvironment {
    ComputeEnvironment {
        environment_id: "env-test".into(),
        os: "test-os".into(),
        architecture: "test-arch".into(),
        lumen_binary_hash: "lumen:test".into(),
        rust_lock_hash: None,
        python_hash: None,
        r_hash: None,
        julia_hash: None,
        dependency_lock_hash: "deps:test".into(),
        locale: "C".into(),
        timezone: "UTC".into(),
        environment_allowlist: vec![],
        cpu_identity: None,
        gpu_identity: None,
        deterministic_flags: vec![],
        network_policy: NetworkPolicy::None,
        container_digest: None,
    }
}

fn step(step_id: &str, kind: StepKind, inputs: &[&str]) -> WorkflowStep {
    WorkflowStep {
        step_id: step_id.into(),
        kind,
        connector_id: matches!(kind, StepKind::ConnectorFetch).then(|| "pubmed".to_string()),
        notebook_cell: matches!(kind, StepKind::NotebookCell).then(|| "cell-1".to_string()),
        inputs: inputs.iter().map(|s| (*s).to_string()).collect(),
        parameters: BTreeMap::new(),
        timeout_secs: 60,
        retry_policy: None,
        cache_policy: CachePolicy::NoCache,
        acceptance_rules: vec![],
    }
}

fn spec(steps: Vec<WorkflowStep>) -> WorkflowSpec {
    WorkflowSpec {
        workflow_id: "wf-exec".into(),
        project_id: ProjectId("proj-exec".into()),
        name: "executor test".into(),
        steps,
        parameters: BTreeMap::new(),
        permissions: vec![],
        resources: ResourceLimits {
            max_concurrent_steps: 1,
            max_total_duration_secs: 3600,
            max_memory_mb: 1024,
            max_disk_mb: 1024,
        },
        schema_version: 1,
    }
}

fn request(operation_id: &str, spec: WorkflowSpec) -> WorkflowExecutionRequest {
    WorkflowExecutionRequest {
        operation_id: operation_id.into(),
        session_id: "session-1".into(),
        owner_id: "owner-1".into(),
        spec,
    }
}

/// A runner whose behaviour per attempt the test dictates.
#[derive(Debug)]
struct ScriptedRunner {
    calls: AtomicU32,
    /// Outcome for attempt N (1-based); the last entry repeats.
    script: Vec<std::result::Result<StepOutput, StepFailure>>,
    /// Trip this (via the executor's cancel token) from inside `run`.
    cancel_on_call: Option<(u32, Arc<AtomicBool>)>,
}

impl ScriptedRunner {
    fn new(script: Vec<std::result::Result<StepOutput, StepFailure>>) -> Self {
        Self {
            calls: AtomicU32::new(0),
            script,
            cancel_on_call: None,
        }
    }
    fn always_ok() -> Self {
        Self::new(vec![Ok(output(&[("out.json", "aa")]))])
    }
    fn calls(&self) -> u32 {
        self.calls.load(Ordering::SeqCst)
    }
}

impl StepRunner for ScriptedRunner {
    fn run(&self, _plan: &StepPlan) -> std::result::Result<StepOutput, StepFailure> {
        let n = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        if let Some((at, flag)) = &self.cancel_on_call
            && n >= *at
        {
            flag.store(true, Ordering::SeqCst);
        }
        let index = (n as usize - 1).min(self.script.len().saturating_sub(1));
        match self.script.get(index) {
            Some(Ok(out)) => Ok(out.clone()),
            Some(Err(failure)) => Err(failure.clone()),
            None => Err(StepFailure::permanent(ErrorClass::RunnerError, "no script")),
        }
    }
}

#[derive(Debug)]
struct BarrierRunner {
    calls: AtomicU32,
    barrier: Barrier,
}

impl BarrierRunner {
    fn new(parties: usize) -> Self {
        Self {
            calls: AtomicU32::new(0),
            barrier: Barrier::new(parties),
        }
    }
}

impl StepRunner for BarrierRunner {
    fn run(&self, _plan: &StepPlan) -> std::result::Result<StepOutput, StepFailure> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.barrier.wait();
        Ok(output(&[("out.json", "same accepted bytes")]))
    }
}

fn output(artifacts: &[(&str, &str)]) -> StepOutput {
    StepOutput {
        artifacts: artifacts
            .iter()
            .map(|(k, v)| ((*k).to_string(), v.as_bytes().to_vec()))
            .collect(),
        exit_code: Some(0),
        bytes_produced: artifacts.iter().map(|(_, value)| value.len() as u64).sum(),
    }
}

struct Harness {
    _dir: TempDir,
    root: PathBuf,
    clock: ManualClock,
}

impl Harness {
    fn new() -> Self {
        let dir = tempdir().unwrap();
        let root = dir.path().to_path_buf();
        Self {
            _dir: dir,
            root,
            clock: ManualClock::at_origin(),
        }
    }

    fn executor(&self, runner: Arc<dyn StepRunner>) -> WorkflowExecutor {
        WorkflowExecutor::new(&self.root, environment())
            .with_clock(Arc::new(self.clock.clone()))
            .with_runner(runner)
    }
}

fn temp_files_under(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(temp_files_under(&path));
        } else if path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with(ProjectStore::temp_prefix()))
        {
            found.push(path);
        }
    }
    found
}

// ── The seam ──────────────────────────────────────────────────────

/// With nothing bound, the engine refuses rather than reporting empty
/// success. There is no default that quietly "runs" a step.
#[test]
fn an_unbound_runner_refuses_every_step() {
    let harness = Harness::new();
    let executor = WorkflowExecutor::new(&harness.root, environment())
        .with_clock(Arc::new(harness.clock.clone()));
    let report = executor
        .execute(&request(
            "op-unbound-001",
            spec(vec![step("fetch", StepKind::ConnectorFetch, &[])]),
        ))
        .unwrap();
    assert_eq!(report.state(), WorkflowState::Failed);
    let attempt = &report.attempts_for("fetch")[0];
    assert_eq!(attempt.error_class, Some(ErrorClass::NoStepRunnerBound));
}

// ── Ordering ──────────────────────────────────────────────────────

#[test]
fn steps_run_in_topological_order() {
    #[derive(Debug, Default)]
    struct OrderRunner(Mutex<Vec<String>>);
    impl StepRunner for OrderRunner {
        fn run(&self, plan: &StepPlan) -> std::result::Result<StepOutput, StepFailure> {
            self.0.lock().unwrap().push(plan.step_id.clone());
            Ok(output(&[("out", "aa")]))
        }
    }
    let harness = Harness::new();
    let runner = Arc::new(OrderRunner::default());
    let executor = harness.executor(runner.clone());
    // Declared out of order on purpose.
    let report = executor
        .execute(&request(
            "op-order-0001",
            spec(vec![
                step("render", StepKind::Renderer, &["transform"]),
                step("transform", StepKind::ArtifactTransform, &["fetch"]),
                step("fetch", StepKind::ConnectorFetch, &[]),
            ]),
        ))
        .unwrap();
    assert_eq!(report.state(), WorkflowState::Succeeded);
    assert_eq!(
        *runner.0.lock().unwrap(),
        vec!["fetch".to_string(), "transform".into(), "render".into()]
    );
    assert_eq!(report.run.step_order, vec!["fetch", "transform", "render"]);
}

#[test]
fn a_cyclic_spec_fails_before_any_step_runs() {
    let harness = Harness::new();
    let runner = Arc::new(ScriptedRunner::always_ok());
    let executor = harness.executor(runner.clone());
    let mut cyclic = spec(vec![
        step("a", StepKind::ConnectorFetch, &["b"]),
        step("b", StepKind::Renderer, &["a"]),
    ]);
    cyclic.workflow_id = "wf-cycle".into();
    let report = executor.execute(&request("op-cycle-0001", cyclic)).unwrap();
    assert_eq!(report.state(), WorkflowState::Failed);
    assert_eq!(runner.calls(), 0, "a cyclic spec must not run anything");
    assert!(report.run.failure.as_deref().unwrap().contains("cycle"));
}

#[test]
fn a_step_id_that_is_not_a_safe_file_name_is_refused() {
    let harness = Harness::new();
    let runner = Arc::new(ScriptedRunner::always_ok());
    let executor = harness.executor(runner.clone());
    let report = executor
        .execute(&request(
            "op-badid-0001",
            spec(vec![step("../escape", StepKind::ConnectorFetch, &[])]),
        ))
        .unwrap();
    assert_eq!(report.state(), WorkflowState::Failed);
    assert_eq!(runner.calls(), 0);
}

// ── Allowlist ─────────────────────────────────────────────────────

/// NotebookCell is not in the default allowlist: running arbitrary code needs
/// an explicit decision.
#[test]
fn a_step_kind_outside_the_allowlist_is_refused() {
    let harness = Harness::new();
    let runner = Arc::new(ScriptedRunner::always_ok());
    let executor = harness.executor(runner.clone());
    let report = executor
        .execute(&request(
            "op-allow-0001",
            spec(vec![step("notebook", StepKind::NotebookCell, &[])]),
        ))
        .unwrap();

    assert_eq!(report.state(), WorkflowState::Failed);
    assert_eq!(
        runner.calls(),
        0,
        "a refused kind must never reach a runner"
    );
    assert_eq!(report.run.refused_steps.len(), 1);
    assert_eq!(
        report.run.refused_steps[0].error_class,
        ErrorClass::StepKindNotAllowed
    );
    // Refused before the run was ever queued.
    let states: Vec<WorkflowState> = report.run.state_history.iter().map(|t| t.state).collect();
    assert!(!states.contains(&WorkflowState::Queued), "{states:?}");
    assert!(!states.contains(&WorkflowState::Running), "{states:?}");
}

/// Allowlisting the kind is not enough: without an admitted kernel the step is
/// still refused.
#[test]
fn a_kernel_step_without_an_admitted_kernel_is_refused() {
    let harness = Harness::new();
    let runner = Arc::new(ScriptedRunner::always_ok());
    let executor = harness
        .executor(runner.clone())
        .with_policy(ExecutionPolicy::default().allowing_kernel_steps());
    let report = executor
        .execute(&request(
            "op-kernel-001",
            spec(vec![step("notebook", StepKind::NotebookCell, &[])]),
        ))
        .unwrap();
    assert_eq!(report.state(), WorkflowState::Failed);
    assert_eq!(runner.calls(), 0);
    assert_eq!(
        report.run.refused_steps[0].error_class,
        ErrorClass::KernelNotAdmitted
    );
}

/// A rejected admission record cannot be handed to an executor at all.
#[test]
fn an_executor_refuses_a_rejected_kernel() {
    let harness = Harness::new();
    let rejected = crate::workflow::probe_kernel(
        &crate::workflow::KernelAdmissionRequest::new(
            "ghost",
            KernelKind::Python,
            "relative-python",
        )
        .with_admitted_by("test"),
    )
    .unwrap();
    let error = WorkflowExecutor::new(&harness.root, environment())
        .with_kernels(KernelManifest {
            kernels: vec![rejected],
            default_python: None,
            default_r: None,
            default_julia: None,
        })
        .unwrap_err();
    assert!(
        matches!(&error, ScienceError::Invalid(m) if m.contains("not admitted")),
        "unexpected: {error}"
    );
}

/// An admitted kernel produces an argv plan and never a command line.
#[cfg(unix)]
#[test]
fn a_kernel_step_plan_is_argv_never_a_shell_line() {
    use std::os::unix::fs::PermissionsExt;

    #[derive(Debug, Default)]
    struct CapturingRunner(Mutex<Option<StepPlan>>);
    impl StepRunner for CapturingRunner {
        fn run(&self, plan: &StepPlan) -> std::result::Result<StepOutput, StepFailure> {
            *self.0.lock().unwrap() = Some(plan.clone());
            Ok(output(&[("out", "aa")]))
        }
    }

    let harness = Harness::new();
    let exe = harness.root.join("python3");
    fs::create_dir_all(&harness.root).unwrap();
    fs::write(&exe, "#!/bin/sh\necho 'Python 3.12.0'\n").unwrap();
    fs::set_permissions(&exe, fs::Permissions::from_mode(0o755)).unwrap();
    let kernel = crate::workflow::probe_kernel(
        &crate::workflow::KernelAdmissionRequest::new("py", KernelKind::Python, &exe)
            .with_admitted_by("test"),
    )
    .unwrap();
    assert!(kernel.is_safe());

    let runner = Arc::new(CapturingRunner::default());
    let executor = harness
        .executor(runner.clone())
        .with_policy(ExecutionPolicy::default().allowing_kernel_steps())
        .with_kernels(KernelManifest {
            default_python: Some(kernel.kernel_id.clone()),
            kernels: vec![kernel],
            default_r: None,
            default_julia: None,
        })
        .unwrap();

    let report = executor
        .execute(&request(
            "op-argv-00001",
            spec(vec![step("notebook", StepKind::NotebookCell, &[])]),
        ))
        .unwrap();
    assert_eq!(report.state(), WorkflowState::Succeeded);

    let plan = runner.0.lock().unwrap().clone().unwrap();
    let StepOperation::KernelCell { invocation, .. } = plan.operation else {
        panic!("expected a kernel cell operation");
    };
    assert!(invocation.interpreter_path.ends_with("python3"));
    // Empty: the RUNNER decides how to drive the interpreter, because only it
    // knows which driver it uses (PythonLoopRunner passes its exec-loop script).
    // A `-` here would arrive as a stray argument to that script. The invariant
    // this test guards is that argv is a LIST, never a shell line — asserted
    // below on the whole vector rather than by its contents.
    assert!(invocation.argv.is_empty(), "argv: {:?}", invocation.argv);
    assert!(
        !invocation
            .argv
            .iter()
            .any(|a| a.contains(';') || a.contains('|') || a.contains('&')),
        "argv must never carry shell metacharacters"
    );
    assert!(!invocation.network_allowed);
    assert!(invocation.process_isolation_required);
    // The cell body is addressed by digest, not interpolated into a string.
    assert_eq!(invocation.cell_source_sha256.len(), 64);
}

// ── At-least-once execution, exactly-once commit ──────────────────

/// Rejected output is evidence of an attempt, never a published artifact.
#[test]
fn a_retried_but_never_accepted_step_produces_no_commit() {
    let harness = Harness::new();
    let runner = Arc::new(ScriptedRunner::always_ok());
    let executor = harness.executor(runner.clone());

    let mut retrying = step("fetch", StepKind::ConnectorFetch, &[]);
    retrying.retry_policy = Some(RetryPolicy {
        max_attempts: 2,
        base_delay_ms: 100,
        backoff_multiplier: 2.0,
    });
    // The runner always yields one artifact; demanding two makes acceptance
    // fail identically on every attempt.
    retrying.acceptance_rules = vec![AcceptanceRule {
        condition: AcceptanceCondition::ArtifactCount(2),
        on_fail: FailAction::Retry,
    }];

    let report = executor
        .execute(&request("op-atleast-01", spec(vec![retrying])))
        .unwrap();

    assert_eq!(
        runner.calls(),
        2,
        "the step must run at least once, twice here"
    );
    let attempts = report.attempts_for("fetch");
    assert_eq!(attempts.len(), 2, "both attempts must be recorded");
    assert!(attempts.iter().all(|a| !a.in_flight()));

    let commits = executor.list_commits().unwrap();
    assert_eq!(
        commits.len(),
        0,
        "acceptance-rejected output became a durable commit: {commits:#?}"
    );
    assert!(attempts.iter().all(|attempt| attempt.commit_key.is_none()));
    assert!(
        attempts
            .iter()
            .all(|attempt| attempt.output_manifest_hash.is_none())
    );
    assert_eq!(report.state(), WorkflowState::Failed);
}

/// A transient failure followed by a success also commits once.
#[test]
fn a_transient_failure_is_retried_and_then_succeeds() {
    let harness = Harness::new();
    let runner = Arc::new(ScriptedRunner::new(vec![
        Err(StepFailure::transient(ErrorClass::RunnerError, "flaky")),
        Ok(output(&[("out.json", "aa")])),
    ]));
    let executor = harness.executor(runner.clone());

    let mut flaky = step("fetch", StepKind::ConnectorFetch, &[]);
    flaky.retry_policy = Some(RetryPolicy {
        max_attempts: 3,
        base_delay_ms: 100,
        backoff_multiplier: 2.0,
    });

    let report = executor
        .execute(&request("op-transient-1", spec(vec![flaky])))
        .unwrap();
    assert_eq!(report.state(), WorkflowState::Succeeded);
    assert_eq!(runner.calls(), 2);
    assert_eq!(executor.list_commits().unwrap().len(), 1);

    let attempts = report.attempts_for("fetch");
    assert_eq!(attempts[0].terminal_state, Some(AttemptState::Failed));
    assert_eq!(attempts[0].error_class, Some(ErrorClass::RunnerError));
    assert_eq!(attempts[1].terminal_state, Some(AttemptState::Succeeded));
}

/// A permanent failure is not retried at all.
#[test]
fn a_permanent_failure_is_not_retried() {
    let harness = Harness::new();
    let runner = Arc::new(ScriptedRunner::new(vec![Err(StepFailure::permanent(
        ErrorClass::PolicyViolation,
        "nope",
    ))]));
    let executor = harness.executor(runner.clone());
    let mut retrying = step("fetch", StepKind::ConnectorFetch, &[]);
    retrying.retry_policy = Some(RetryPolicy {
        max_attempts: 5,
        base_delay_ms: 10,
        backoff_multiplier: 1.0,
    });
    let report = executor
        .execute(&request("op-permanent-1", spec(vec![retrying])))
        .unwrap();
    assert_eq!(report.state(), WorkflowState::Failed);
    assert_eq!(runner.calls(), 1);
}

/// A second run of the same step, with the same inputs and environment, finds
/// the existing commit and does not run the step again.
#[test]
fn deterministic_reuse_skips_a_step_whose_commit_already_exists() {
    let harness = Harness::new();
    let runner = Arc::new(ScriptedRunner::always_ok());
    let executor = harness.executor(runner.clone());

    let mut reusable = step("fetch", StepKind::ConnectorFetch, &[]);
    reusable.cache_policy = CachePolicy::DeterministicReuse;

    let first = executor
        .execute(&request("op-reuse-0001", spec(vec![reusable.clone()])))
        .unwrap();
    assert_eq!(first.state(), WorkflowState::Succeeded);
    assert_eq!(runner.calls(), 1);
    assert_eq!(first.steps_reused, 0);

    // A different operation id: a genuinely new run, same work.
    let second = executor
        .execute(&request("op-reuse-0002", spec(vec![reusable])))
        .unwrap();
    assert_eq!(second.state(), WorkflowState::Succeeded);
    assert!(!second.replayed, "this is a new run, not a replay");
    assert_eq!(runner.calls(), 1, "the reused step must not run again");
    assert_eq!(second.steps_reused, 1);
    assert_eq!(
        second.attempts_for("fetch")[0].terminal_state,
        Some(AttemptState::Reused)
    );
    assert_eq!(executor.list_commits().unwrap().len(), 1);
}

#[test]
fn executor_hashes_exact_runner_bytes_into_the_store_owned_cas() {
    let harness = Harness::new();
    let runner = Arc::new(ScriptedRunner::new(vec![Ok(output(&[(
        "result.bin",
        "exact artifact bytes",
    )]))]));
    let executor = harness.executor(runner);

    let report = executor
        .execute(&request(
            "op-cas-bytes",
            spec(vec![step("fetch", StepKind::ConnectorFetch, &[])]),
        ))
        .unwrap();
    let commit = &report.commits[0];
    let expected_bytes = b"exact artifact bytes";
    let expected_digest = hex_sha256(expected_bytes);
    assert_eq!(commit.output_manifest["result.bin"], expected_digest);
    assert_eq!(
        fs::read(harness.root.join(executor.artifact_path(&expected_digest))).unwrap(),
        expected_bytes
    );
}

#[test]
fn committed_artifact_bytes_reopens_verified_cas() {
    let harness = Harness::new();
    let runner = Arc::new(ScriptedRunner::new(vec![Ok(output(&[
        ("result.json", "{\"mean\":1.5}"),
        ("stdout.txt", "computed\n"),
    ]))]));
    let executor = harness.executor(runner);
    let report = executor
        .execute(&request(
            "op-authority-cas-read",
            spec(vec![step("fetch", StepKind::ConnectorFetch, &[])]),
        ))
        .unwrap();
    let commit = &report.commits[0];

    assert_eq!(
        executor
            .committed_artifact_bytes(&commit.commit_key, "result.json")
            .unwrap(),
        b"{\"mean\":1.5}"
    );
    assert_eq!(
        executor
            .committed_artifact_bytes(&commit.commit_key, "stdout.txt")
            .unwrap(),
        b"computed\n"
    );
    assert!(
        executor
            .committed_artifact_bytes(&commit.commit_key, "missing.txt")
            .is_err(),
        "an artifact absent from the canonical manifest was served"
    );
    assert!(
        executor
            .committed_artifact_bytes(&"0".repeat(64), "result.json")
            .is_err(),
        "an unknown commit key was accepted"
    );
}

#[test]
fn committed_artifact_bytes_refuses_missing_or_tampered_cas() {
    for (label, replacement) in [("missing", None), ("tampered", Some(b"forged".as_slice()))] {
        let harness = Harness::new();
        let runner = Arc::new(ScriptedRunner::always_ok());
        let executor = harness.executor(runner);
        let report = executor
            .execute(&request(
                &format!("op-authority-cas-{label}"),
                spec(vec![step("fetch", StepKind::ConnectorFetch, &[])]),
            ))
            .unwrap();
        let commit = &report.commits[0];
        let digest = &commit.output_manifest["out.json"];
        let cas_path = harness.root.join(executor.artifact_path(digest));
        match replacement {
            Some(bytes) => fs::write(cas_path, bytes).unwrap(),
            None => fs::remove_file(cas_path).unwrap(),
        }

        assert!(
            executor
                .committed_artifact_bytes(&commit.commit_key, "out.json")
                .is_err(),
            "{label} CAS bytes were served"
        );
    }
}

#[test]
fn workflow_run_id_for_operation_is_validated_and_deterministic() {
    let first = run_id_for_operation("op-authority-run-id").unwrap();
    let replay = run_id_for_operation("op-authority-run-id").unwrap();
    let other = run_id_for_operation("op-authority-run-id-other").unwrap();

    assert_eq!(first, replay);
    assert_ne!(first, other);
    assert_eq!(first.len(), 32);
    assert!(first.bytes().all(|byte| byte.is_ascii_hexdigit()));
    for invalid in ["", "../escape", "slash/name", " leading-space"] {
        assert!(
            run_id_for_operation(invalid).is_err(),
            "invalid operation id {invalid:?} received a run id"
        );
    }
}

#[test]
fn runner_byte_count_claim_is_recomputed_before_cas_publication() {
    let harness = Harness::new();
    let runner = Arc::new(ScriptedRunner::new(vec![Ok(StepOutput {
        artifacts: BTreeMap::from([("result.bin".into(), b"real bytes".to_vec())]),
        exit_code: Some(0),
        bytes_produced: 1,
    })]));
    let executor = harness.executor(runner);

    let report = executor
        .execute(&request(
            "op-cas-byte-count-lie",
            spec(vec![step("fetch", StepKind::ConnectorFetch, &[])]),
        ))
        .unwrap();
    assert_eq!(report.state(), WorkflowState::Failed);
    assert!(report.commits.is_empty());
    assert_eq!(
        report.attempts_for("fetch")[0].error_class,
        Some(ErrorClass::PolicyViolation)
    );
    assert!(
        !harness.root.join("workflow-artifacts").exists(),
        "unverified bytes reached the CAS"
    );
}

#[test]
fn unsafe_runner_artifact_name_is_rejected_before_cas_publication() {
    let harness = Harness::new();
    let runner = Arc::new(ScriptedRunner::new(vec![Ok(StepOutput {
        artifacts: BTreeMap::from([("../outside".into(), b"payload".to_vec())]),
        exit_code: Some(0),
        bytes_produced: 7,
    })]));
    let executor = harness.executor(runner);

    let report = executor
        .execute(&request(
            "op-cas-unsafe-name",
            spec(vec![step("fetch", StepKind::ConnectorFetch, &[])]),
        ))
        .unwrap();
    assert_eq!(report.state(), WorkflowState::Failed);
    assert!(report.commits.is_empty());
    assert!(
        report
            .run
            .failure
            .as_deref()
            .is_some_and(|failure| failure.contains("unsafe workflow artifact name"))
    );
    assert!(
        !harness.root.join("workflow-artifacts").exists(),
        "unsafe artifact name reached the CAS"
    );
}

#[test]
fn deterministic_reuse_fails_durably_when_a_cas_blob_is_tampered() {
    let harness = Harness::new();
    let runner = Arc::new(ScriptedRunner::always_ok());
    let executor = harness.executor(runner.clone());
    let mut reusable = step("fetch", StepKind::ConnectorFetch, &[]);
    reusable.cache_policy = CachePolicy::DeterministicReuse;

    let first = executor
        .execute(&request("op-cas-tamper-1", spec(vec![reusable.clone()])))
        .unwrap();
    let digest = first.commits[0].output_manifest["out.json"].clone();
    fs::write(
        harness.root.join(executor.artifact_path(&digest)),
        b"tampered",
    )
    .unwrap();

    let second = executor
        .execute(&request("op-cas-tamper-2", spec(vec![reusable])))
        .unwrap();
    assert_eq!(second.state(), WorkflowState::Failed);
    assert_eq!(runner.calls(), 1, "corrupt CAS must never run or reuse");
    let attempt = second.attempts_for("fetch")[0];
    assert_eq!(attempt.terminal_state, Some(AttemptState::Failed));
    assert_eq!(attempt.error_class, Some(ErrorClass::PolicyViolation));
    assert!(
        second
            .run
            .failure
            .as_deref()
            .is_some_and(|failure| failure.contains("stored bytes hash")),
        "failure was not durable: {:?}",
        second.run.failure
    );
}

#[test]
fn deterministic_reuse_fails_durably_when_a_cas_blob_is_deleted() {
    let harness = Harness::new();
    let runner = Arc::new(ScriptedRunner::always_ok());
    let executor = harness.executor(runner.clone());
    let mut reusable = step("fetch", StepKind::ConnectorFetch, &[]);
    reusable.cache_policy = CachePolicy::DeterministicReuse;

    let first = executor
        .execute(&request("op-cas-delete-1", spec(vec![reusable.clone()])))
        .unwrap();
    let digest = first.commits[0].output_manifest["out.json"].clone();
    fs::remove_file(harness.root.join(executor.artifact_path(&digest))).unwrap();

    let second = executor
        .execute(&request("op-cas-delete-2", spec(vec![reusable])))
        .unwrap();
    assert_eq!(second.state(), WorkflowState::Failed);
    assert_eq!(runner.calls(), 1, "missing CAS must never run or reuse");
    let attempt = second.attempts_for("fetch")[0];
    assert_eq!(attempt.terminal_state, Some(AttemptState::Failed));
    assert_eq!(attempt.error_class, Some(ErrorClass::PolicyViolation));
    assert!(
        second
            .run
            .failure
            .as_deref()
            .is_some_and(|failure| failure.contains("is missing")),
        "failure was not durable: {:?}",
        second.run.failure
    );
}

#[test]
fn deterministic_reuse_refuses_a_commit_whose_origin_attempt_is_not_succeeded() {
    let harness = Harness::new();
    let runner = Arc::new(ScriptedRunner::always_ok());
    let executor = harness.executor(runner.clone());
    let mut reusable = step("fetch", StepKind::ConnectorFetch, &[]);
    reusable.cache_policy = CachePolicy::DeterministicReuse;

    let first = executor
        .execute(&request(
            "op-origin-attempt-1",
            spec(vec![reusable.clone()]),
        ))
        .unwrap();
    let origin = &first.attempts[0];
    let path = harness
        .root
        .join(executor.attempt_path(&first.run.run_id, &origin.attempt_id));
    let mut forged: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    forged["terminal_state"] = serde_json::json!("failed");
    fs::write(&path, serde_json::to_vec_pretty(&forged).unwrap()).unwrap();

    let second = executor
        .execute(&request("op-origin-attempt-2", spec(vec![reusable])))
        .unwrap();
    assert_eq!(second.state(), WorkflowState::Failed);
    assert_eq!(
        runner.calls(),
        1,
        "commit with a forged origin attempt was reused or re-executed"
    );
    assert_eq!(
        second.attempts_for("fetch")[0].error_class,
        Some(ErrorClass::PolicyViolation)
    );
}

#[test]
fn prepared_commit_is_recovered_to_one_succeeded_origin_and_committed_state() {
    let harness = Harness::new();
    let executor = harness.executor(Arc::new(ScriptedRunner::always_ok()));
    let mut reusable = step("fetch", StepKind::ConnectorFetch, &[]);
    reusable.cache_policy = CachePolicy::DeterministicReuse;
    let first = executor
        .execute(&request("op-prepared-recovery", spec(vec![reusable])))
        .unwrap();
    let mut commit = first.commits[0].clone();
    commit.state = ArtifactCommitState::Prepared;
    executor
        .write_record(&executor.commit_path(&commit.commit_key), &commit)
        .unwrap();

    let mut origin = first.attempts[0].clone();
    origin.commit_key = None;
    origin.output_manifest_hash = None;
    origin.terminal_state = None;
    origin.finished_at = None;
    executor
        .write_record(
            &executor.attempt_path(&origin.run_id, &origin.attempt_id),
            &origin,
        )
        .unwrap();

    let recovered = executor
        .load_commit(&commit.commit_key)
        .unwrap()
        .expect("prepared commit");
    assert_eq!(recovered.state, ArtifactCommitState::Committed);
    let recovered_origin = executor.load_attempts(&origin.run_id).unwrap().remove(0);
    assert_eq!(
        recovered_origin.terminal_state,
        Some(AttemptState::Succeeded)
    );
    assert_eq!(
        recovered_origin.commit_key.as_deref(),
        Some(commit.commit_key.as_str())
    );
}

#[test]
fn concurrent_deterministic_publish_never_exposes_a_half_committed_origin() {
    let harness = Harness::new();
    let runner = Arc::new(BarrierRunner::new(2));
    let executor = harness.executor(runner.clone());
    let mut reusable = step("fetch", StepKind::ConnectorFetch, &[]);
    reusable.cache_policy = CachePolicy::DeterministicReuse;
    let workflow = spec(vec![reusable]);

    let left_executor = executor.clone();
    let left_workflow = workflow.clone();
    let left = std::thread::spawn(move || {
        left_executor.execute(&request("op-concurrent-publish-left", left_workflow))
    });
    let right_executor = executor.clone();
    let right = std::thread::spawn(move || {
        right_executor.execute(&request("op-concurrent-publish-right", workflow))
    });
    let left = left.join().unwrap().unwrap();
    let right = right.join().unwrap().unwrap();

    assert_eq!(runner.calls.load(Ordering::SeqCst), 2);
    assert_eq!(left.state(), WorkflowState::Succeeded);
    assert_eq!(right.state(), WorkflowState::Succeeded);
    assert_eq!(
        left.attempts[0].terminal_state,
        Some(AttemptState::Succeeded)
    );
    assert_eq!(
        right.attempts[0].terminal_state,
        Some(AttemptState::Succeeded)
    );
    assert_eq!(left.commits[0].commit_key, right.commits[0].commit_key);
    assert_eq!(executor.list_commits().unwrap().len(), 1);
}

#[test]
fn durable_v1_run_and_operation_records_fail_closed_without_rewrite() {
    let harness = Harness::new();
    let executor = harness.executor(Arc::new(ScriptedRunner::always_ok()));
    let workflow = spec(vec![step("fetch", StepKind::ConnectorFetch, &[])]);
    let report = executor
        .execute(&request("op-schema-v1", workflow.clone()))
        .unwrap();
    let run_path = harness.root.join(executor.run_path(&report.run.run_id));
    let operation_path = harness.root.join(executor.operation_path("op-schema-v1"));

    for path in [&run_path, &operation_path] {
        let mut record: serde_json::Value =
            serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        record["schema_version"] = serde_json::json!(1);
        fs::write(path, serde_json::to_vec_pretty(&record).unwrap()).unwrap();
    }
    let run_before = fs::read(&run_path).unwrap();
    let operation_before = fs::read(&operation_path).unwrap();

    assert!(executor.load_run(&report.run.run_id).is_err());
    assert!(executor.lookup_operation("op-schema-v1").is_err());
    assert!(
        executor
            .execute(&request("op-schema-v1", workflow))
            .is_err()
    );
    assert_eq!(fs::read(&run_path).unwrap(), run_before);
    assert_eq!(fs::read(&operation_path).unwrap(), operation_before);
}

#[test]
fn no_cache_runs_publish_distinct_commits_and_reports_are_exact() {
    let harness = Harness::new();
    let runner = Arc::new(ScriptedRunner::always_ok());
    let executor = harness.executor(runner.clone());
    let workflow = spec(vec![step("fetch", StepKind::ConnectorFetch, &[])]);

    let first = executor
        .execute(&request("op-no-cache-1", workflow.clone()))
        .unwrap();
    let second = executor
        .execute(&request("op-no-cache-2", workflow.clone()))
        .unwrap();
    assert_eq!(runner.calls(), 2);
    assert_eq!(first.commits.len(), 1);
    assert_eq!(second.commits.len(), 1);
    assert_ne!(first.commits[0].commit_key, second.commits[0].commit_key);
    assert_ne!(first.commits[0].run_id, second.commits[0].run_id);
    assert_eq!(executor.list_commits().unwrap().len(), 2);

    let replayed_first = executor
        .execute(&request("op-no-cache-1", workflow))
        .unwrap();
    assert!(replayed_first.replayed);
    assert_eq!(replayed_first.commits.len(), 1);
    assert_eq!(
        replayed_first.commits[0].commit_key, first.commits[0].commit_key,
        "report selected another run's equal-manifest commit"
    );
}

#[test]
fn deterministic_reuse_is_scoped_to_owner_project_and_session() {
    let harness = Harness::new();
    let runner = Arc::new(ScriptedRunner::always_ok());
    let executor = harness.executor(runner.clone());
    let mut reusable = step("fetch", StepKind::ConnectorFetch, &[]);
    reusable.cache_policy = CachePolicy::DeterministicReuse;
    let base = spec(vec![reusable]);

    let first = executor
        .execute(&request("op-scope-base", base.clone()))
        .unwrap();

    let mut other_project = base.clone();
    other_project.project_id = ProjectId("proj-other".into());
    let project_report = executor
        .execute(&request("op-scope-project", other_project))
        .unwrap();

    let mut session_request = request("op-scope-session", base);
    session_request.session_id = "session-2".into();
    let session_report = executor.execute(&session_request).unwrap();

    let mut owner_request = request("op-scope-owner", session_request.spec.clone());
    owner_request.owner_id = "owner-2".into();
    let owner_report = executor.execute(&owner_request).unwrap();

    assert_eq!(runner.calls(), 4, "authority scopes must not share commits");
    assert_eq!(first.steps_reused, 0);
    assert_eq!(project_report.steps_reused, 0);
    assert_eq!(session_report.steps_reused, 0);
    assert_eq!(owner_report.steps_reused, 0);
    assert_ne!(
        first.commits[0].commit_key,
        project_report.commits[0].commit_key
    );
    assert_ne!(
        first.commits[0].commit_key,
        session_report.commits[0].commit_key
    );
    assert_ne!(
        first.commits[0].commit_key,
        owner_report.commits[0].commit_key
    );
    assert_eq!(project_report.commits[0].project_id.0, "proj-other");
    assert_eq!(session_report.commits[0].session_id, "session-2");
    assert_eq!(owner_report.commits[0].owner_id, "owner-2");
}

#[test]
fn local_reuse_is_refused_before_the_runner_or_commit_store() {
    let harness = Harness::new();
    let runner = Arc::new(ScriptedRunner::always_ok());
    let executor = harness.executor(runner.clone());
    let mut local = step("fetch", StepKind::ConnectorFetch, &[]);
    local.cache_policy = CachePolicy::LocalReuse;

    let report = executor
        .execute(&request("op-local-reuse", spec(vec![local])))
        .unwrap();
    assert_eq!(report.state(), WorkflowState::Failed);
    assert_eq!(runner.calls(), 0);
    assert!(report.attempts.is_empty());
    assert!(report.commits.is_empty());
    assert_eq!(
        report.run.refused_steps[0].error_class,
        ErrorClass::PolicyViolation
    );
}

/// Reuse is keyed on content: change a parameter and the step runs again.
#[test]
fn changing_a_parameter_defeats_reuse() {
    let harness = Harness::new();
    let runner = Arc::new(ScriptedRunner::always_ok());
    let executor = harness.executor(runner.clone());

    let mut base = step("fetch", StepKind::ConnectorFetch, &[]);
    base.cache_policy = CachePolicy::DeterministicReuse;
    executor
        .execute(&request("op-param-0001", spec(vec![base.clone()])))
        .unwrap();
    assert_eq!(runner.calls(), 1);

    let mut changed = base;
    changed
        .parameters
        .insert("query".into(), "different".into());
    let second = executor
        .execute(&request("op-param-0002", spec(vec![changed])))
        .unwrap();
    assert_eq!(second.steps_reused, 0);
    assert_eq!(runner.calls(), 2, "a different parameter is different work");
    assert_eq!(executor.list_commits().unwrap().len(), 2);
}

#[test]
fn changing_acceptance_contract_defeats_reuse() {
    let harness = Harness::new();
    let runner = Arc::new(ScriptedRunner::always_ok());
    let executor = harness.executor(runner.clone());

    let mut original = step("fetch", StepKind::ConnectorFetch, &[]);
    original.cache_policy = CachePolicy::DeterministicReuse;
    executor
        .execute(&request(
            "op-acceptance-contract-1",
            spec(vec![original.clone()]),
        ))
        .unwrap();

    let mut stricter = original;
    stricter.acceptance_rules.push(AcceptanceRule {
        condition: AcceptanceCondition::ArtifactCount(1),
        on_fail: FailAction::Abort,
    });
    let second = executor
        .execute(&request("op-acceptance-contract-2", spec(vec![stricter])))
        .unwrap();
    assert_eq!(second.state(), WorkflowState::Succeeded);
    assert_eq!(second.steps_reused, 0);
    assert_eq!(
        runner.calls(),
        2,
        "a changed acceptance contract reused output accepted under older rules"
    );
}

// ── Operation-id deduplication ────────────────────────────────────

#[test]
fn re_executing_one_operation_id_replays_instead_of_running_again() {
    let harness = Harness::new();
    let runner = Arc::new(ScriptedRunner::always_ok());
    let executor = harness.executor(runner.clone());
    let workflow = spec(vec![step("fetch", StepKind::ConnectorFetch, &[])]);

    let first = executor
        .execute(&request("op-dedupe-001", workflow.clone()))
        .unwrap();
    assert!(!first.replayed);
    assert_eq!(runner.calls(), 1);

    let second = executor
        .execute(&request("op-dedupe-001", workflow))
        .unwrap();
    assert!(second.replayed, "a repeated operation id must replay");
    assert_eq!(runner.calls(), 1, "the workflow ran a second time");
    assert_eq!(second.run.run_id, first.run.run_id);
    assert_eq!(second.state(), first.state());
}

#[test]
fn a_replay_is_refused_to_another_session_or_owner() {
    let harness = Harness::new();
    let executor = harness.executor(Arc::new(ScriptedRunner::always_ok()));
    let workflow = spec(vec![step("fetch", StepKind::ConnectorFetch, &[])]);
    executor
        .execute(&request("op-owner-0001", workflow.clone()))
        .unwrap();

    let mut other_session = request("op-owner-0001", workflow.clone());
    other_session.session_id = "session-2".into();
    assert!(matches!(
        executor.execute(&other_session),
        Err(ScienceError::Ownership)
    ));

    let mut other_owner = request("op-owner-0001", workflow);
    other_owner.owner_id = "owner-2".into();
    assert!(matches!(
        executor.execute(&other_owner),
        Err(ScienceError::Ownership)
    ));
}

#[test]
fn a_replay_binds_project_spec_environment_and_policy() {
    let harness = Harness::new();
    let workflow = spec(vec![step("fetch", StepKind::ConnectorFetch, &[])]);
    harness
        .executor(Arc::new(ScriptedRunner::always_ok()))
        .execute(&request("op-authority-bind", workflow.clone()))
        .unwrap();

    let mut another_project = workflow.clone();
    another_project.project_id = ProjectId("proj-not-approved".into());
    assert!(matches!(
        harness
            .executor(Arc::new(ScriptedRunner::always_ok()))
            .execute(&request("op-authority-bind", another_project)),
        Err(ScienceError::Invalid(message))
            if message.contains("does not match its original authority request")
    ));

    let mut changed_spec = workflow.clone();
    changed_spec.name = "a different workflow payload".into();
    assert!(matches!(
        harness
            .executor(Arc::new(ScriptedRunner::always_ok()))
            .execute(&request("op-authority-bind", changed_spec)),
        Err(ScienceError::Invalid(message))
            if message.contains("does not match its original authority request")
    ));

    let mut changed_environment = environment();
    changed_environment.environment_allowlist = vec!["another-interpreter".into()];
    let environment_executor = WorkflowExecutor::new(&harness.root, changed_environment)
        .with_clock(Arc::new(harness.clock.clone()))
        .with_runner(Arc::new(ScriptedRunner::always_ok()));
    assert!(matches!(
        environment_executor.execute(&request("op-authority-bind", workflow.clone())),
        Err(ScienceError::Invalid(message))
            if message.contains("does not match its original authority request")
    ));

    let policy_executor = harness
        .executor(Arc::new(ScriptedRunner::always_ok()))
        .with_policy(ExecutionPolicy::default().allowing_kernel_steps());
    assert!(matches!(
        policy_executor.execute(&request("op-authority-bind", workflow)),
        Err(ScienceError::Invalid(message))
            if message.contains("does not match its original authority request")
    ));
}

#[test]
fn operation_id_shape_is_validated() {
    let harness = Harness::new();
    let executor = harness.executor(Arc::new(ScriptedRunner::always_ok()));
    let workflow = spec(vec![step("fetch", StepKind::ConnectorFetch, &[])]);
    for bad in ["", "short", "../../escape", "has space", &"x".repeat(129)] {
        assert!(
            executor.execute(&request(bad, workflow.clone())).is_err(),
            "operation id {bad:?} was accepted"
        );
    }
}

// ── Deterministic clock ───────────────────────────────────────────

/// Backoff comes from the injected clock, so the schedule is an assertion and
/// the test costs no wall time.
#[test]
fn retry_backoff_uses_the_injected_clock() {
    let harness = Harness::new();
    let runner = Arc::new(ScriptedRunner::new(vec![Err(StepFailure::transient(
        ErrorClass::RunnerError,
        "flaky",
    ))]));
    let executor = harness.executor(runner.clone());

    let mut retrying = step("fetch", StepKind::ConnectorFetch, &[]);
    retrying.retry_policy = Some(RetryPolicy {
        max_attempts: 4,
        base_delay_ms: 100,
        backoff_multiplier: 2.0,
    });

    let started = harness.clock.now();
    let report = executor
        .execute(&request("op-backoff-01", spec(vec![retrying])))
        .unwrap();

    assert_eq!(runner.calls(), 4);
    assert_eq!(report.state(), WorkflowState::Failed);
    // 100ms, 200ms, 400ms — then the attempts are spent, so no fourth sleep.
    assert_eq!(
        harness.clock.slept(),
        vec![
            Duration::from_millis(100),
            Duration::from_millis(200),
            Duration::from_millis(400),
        ]
    );
    assert_eq!(
        harness.clock.now() - started,
        chrono::Duration::milliseconds(700)
    );
}

#[test]
fn backoff_is_capped_by_policy() {
    let harness = Harness::new();
    let runner = Arc::new(ScriptedRunner::new(vec![Err(StepFailure::transient(
        ErrorClass::RunnerError,
        "flaky",
    ))]));
    let policy = ExecutionPolicy {
        max_retry_delay: Duration::from_millis(150),
        ..Default::default()
    };
    let executor = harness.executor(runner).with_policy(policy);

    let mut retrying = step("fetch", StepKind::ConnectorFetch, &[]);
    retrying.retry_policy = Some(RetryPolicy {
        max_attempts: 3,
        base_delay_ms: 1_000,
        backoff_multiplier: 10.0,
    });
    executor
        .execute(&request("op-cap-000001", spec(vec![retrying])))
        .unwrap();
    assert_eq!(
        harness.clock.slept(),
        vec![Duration::from_millis(150), Duration::from_millis(150)]
    );
}

/// The policy ceiling wins over a step's own retry policy.
#[test]
fn the_policy_ceiling_bounds_a_steps_own_retry_count() {
    let harness = Harness::new();
    let runner = Arc::new(ScriptedRunner::new(vec![Err(StepFailure::transient(
        ErrorClass::RunnerError,
        "flaky",
    ))]));
    let policy = ExecutionPolicy {
        max_attempts_ceiling: 2,
        ..Default::default()
    };
    let executor = harness.executor(runner.clone()).with_policy(policy);

    let mut greedy = step("fetch", StepKind::ConnectorFetch, &[]);
    greedy.retry_policy = Some(RetryPolicy {
        max_attempts: 100,
        base_delay_ms: 1,
        backoff_multiplier: 1.0,
    });
    executor
        .execute(&request("op-ceiling-01", spec(vec![greedy])))
        .unwrap();
    assert_eq!(runner.calls(), 2);
}

#[test]
fn the_total_duration_budget_is_enforced_on_the_injected_clock() {
    #[derive(Debug)]
    struct SlowRunner(ManualClock);
    impl StepRunner for SlowRunner {
        fn run(&self, _plan: &StepPlan) -> std::result::Result<StepOutput, StepFailure> {
            // Simulate a step that took ten minutes, without taking any.
            self.0.advance(Duration::from_secs(600));
            Ok(output(&[("out", "aa")]))
        }
    }
    let harness = Harness::new();
    let executor = harness.executor(Arc::new(SlowRunner(harness.clock.clone())));
    let mut workflow = spec(vec![
        step("a", StepKind::ConnectorFetch, &[]),
        step("b", StepKind::Renderer, &["a"]),
    ]);
    workflow.resources.max_total_duration_secs = 60;
    let report = executor
        .execute(&request("op-budget-001", workflow))
        .unwrap();
    assert_eq!(report.state(), WorkflowState::Failed);
    assert!(report.run.failure.as_deref().unwrap().contains("budget"));
    // The first step ran; the second was never started.
    assert_eq!(report.attempts_for("a").len(), 1);
    assert!(report.attempts_for("b").is_empty());
}

// ── Cancellation ──────────────────────────────────────────────────

#[test]
fn cancellation_stops_the_run_before_the_next_step() {
    let harness = Harness::new();
    let executor = harness.executor(Arc::new(UnboundStepRunner));
    // Cancel from inside the first step's runner call. `with_runner` keeps the
    // same cancel token, so the runner trips the flag this executor reads.
    let cancelling = Arc::new(ScriptedRunner {
        calls: AtomicU32::new(0),
        script: vec![Ok(output(&[("out", "aa")]))],
        cancel_on_call: Some((1, executor.cancel_token())),
    });
    let executor = executor.with_runner(cancelling.clone());

    let report = executor
        .execute(&request(
            "op-cancel-001",
            spec(vec![
                step("a", StepKind::ConnectorFetch, &[]),
                step("b", StepKind::Renderer, &["a"]),
            ]),
        ))
        .unwrap();

    assert_eq!(report.state(), WorkflowState::Cancelled);
    assert_eq!(cancelling.calls(), 1, "the second step must not have run");
    assert_eq!(
        report.attempts_for("a")[0].terminal_state,
        Some(AttemptState::Cancelled)
    );
    assert!(report.attempts_for("b").is_empty());
}

#[test]
fn a_run_cancelled_before_it_starts_never_reaches_running() {
    let harness = Harness::new();
    let runner = Arc::new(ScriptedRunner::always_ok());
    let executor = harness.executor(runner.clone());
    executor.cancel();
    let report = executor
        .execute(&request(
            "op-precancel-1",
            spec(vec![step("fetch", StepKind::ConnectorFetch, &[])]),
        ))
        .unwrap();
    assert_eq!(report.state(), WorkflowState::Cancelled);
    assert_eq!(runner.calls(), 0);
    let states: Vec<WorkflowState> = report.run.state_history.iter().map(|t| t.state).collect();
    assert!(states.contains(&WorkflowState::Queued));
    assert!(!states.contains(&WorkflowState::Running), "{states:?}");
}

// ── State machine ─────────────────────────────────────────────────

#[test]
fn a_successful_run_walks_the_whole_state_machine() {
    let harness = Harness::new();
    let executor = harness.executor(Arc::new(ScriptedRunner::always_ok()));
    let report = executor
        .execute(&request(
            "op-states-001",
            spec(vec![step("fetch", StepKind::ConnectorFetch, &[])]),
        ))
        .unwrap();
    let states: Vec<WorkflowState> = report.run.state_history.iter().map(|t| t.state).collect();
    assert_eq!(
        states,
        vec![
            WorkflowState::Draft,
            WorkflowState::Validated,
            WorkflowState::Admitted,
            WorkflowState::Queued,
            WorkflowState::Running,
            WorkflowState::Succeeded,
        ]
    );
    assert!(report.run.finished_at.is_some());
}

#[test]
fn every_attempt_records_the_required_identity_fields() {
    let harness = Harness::new();
    let executor = harness.executor(Arc::new(ScriptedRunner::always_ok()));
    let report = executor
        .execute(&request(
            "op-fields-001",
            spec(vec![
                step("fetch", StepKind::ConnectorFetch, &[]),
                step("render", StepKind::Renderer, &["fetch"]),
            ]),
        ))
        .unwrap();
    assert_eq!(report.attempts.len(), 2);
    for attempt in &report.attempts {
        assert_eq!(attempt.workflow_id, "wf-exec");
        assert!(!attempt.run_id.is_empty());
        assert!(!attempt.step_id.is_empty());
        assert_eq!(attempt.input_manifest_hash.len(), 64);
        assert_eq!(attempt.parameter_hash.len(), 64);
        assert_eq!(attempt.implementation_hash.len(), 64);
        assert_eq!(attempt.environment_hash.len(), 64);
        assert_eq!(attempt.policy_hash.len(), 64);
        assert_eq!(attempt.terminal_state, Some(AttemptState::Succeeded));
        assert_eq!(attempt.output_manifest_hash.as_ref().unwrap().len(), 64);
        assert!(attempt.error_class.is_none());
        assert!(attempt.finished_at.is_some());
    }
    // The downstream step's input manifest names its upstream's output.
    let render = report.attempts_for("render")[0];
    let fetch = report.attempts_for("fetch")[0];
    assert_ne!(render.input_manifest_hash, fetch.input_manifest_hash);
}

// ── Crash recovery ────────────────────────────────────────────────

/// Simulates a process that died mid-step: the attempt record on disk is
/// non-terminal and the run never reached a terminal state.
fn wedge_a_run_in_flight(executor: &WorkflowExecutor, run_id: &str) {
    let mut attempts = executor.load_attempts(run_id).unwrap();
    let attempt = attempts.last_mut().unwrap();
    attempt.terminal_state = None;
    attempt.finished_at = None;
    attempt.output_manifest_hash = None;
    attempt.commit_key = None;
    executor
        .write_record(&executor.attempt_path(run_id, &attempt.attempt_id), attempt)
        .unwrap();

    let mut run = executor.load_run(run_id).unwrap();
    run.state = WorkflowState::Running;
    run.finished_at = None;
    run.failure = None;
    run.state_history.push(WorkflowStateTransition {
        state: WorkflowState::Running,
        at: executor.clock.now(),
        note: Some("test crash fixture".into()),
    });
    executor
        .write_record(&executor.run_path(run_id), &run)
        .unwrap();
}

#[test]
fn recovery_marks_in_flight_attempts_interrupted_without_re_running_them() {
    let harness = Harness::new();
    let runner = Arc::new(ScriptedRunner::always_ok());
    let executor = harness.executor(runner.clone());
    let report = executor
        .execute(&request(
            "op-recover-01",
            spec(vec![step("fetch", StepKind::ConnectorFetch, &[])]),
        ))
        .unwrap();
    let run_id = report.run.run_id.clone();
    assert_eq!(runner.calls(), 1);

    wedge_a_run_in_flight(&executor, &run_id);

    let recovery = executor.recover_run(&run_id).unwrap();
    assert!(recovery.repaired());
    assert_eq!(recovery.interrupted_attempts.len(), 1);
    assert_eq!(recovery.run_state_before, Some(WorkflowState::Running));
    assert_eq!(recovery.run_state_after, Some(WorkflowState::Interrupted));
    assert_eq!(runner.calls(), 1, "recovery must not re-run the step");

    let attempts = executor.load_attempts(&run_id).unwrap();
    assert_eq!(attempts[0].terminal_state, Some(AttemptState::Interrupted));
    assert_eq!(attempts[0].error_class, Some(ErrorClass::Interrupted));
}

/// Re-executing an interrupted operation id closes it out; it does not
/// silently start the workflow over.
#[test]
fn re_executing_an_interrupted_run_recovers_it_rather_than_re_running() {
    let harness = Harness::new();
    let runner = Arc::new(ScriptedRunner::always_ok());
    let executor = harness.executor(runner.clone());
    let workflow = spec(vec![step("fetch", StepKind::ConnectorFetch, &[])]);
    let report = executor
        .execute(&request("op-reexec-001", workflow.clone()))
        .unwrap();
    wedge_a_run_in_flight(&executor, &report.run.run_id);

    let again = executor
        .execute(&request("op-reexec-001", workflow))
        .unwrap();
    assert!(again.replayed);
    assert!(again.recovered);
    assert_eq!(again.state(), WorkflowState::Interrupted);
    assert_eq!(runner.calls(), 1, "the interrupted step was re-executed");
}

#[test]
fn recovery_is_idempotent_and_sweeps_stale_temp_files() {
    let harness = Harness::new();
    let executor = harness.executor(Arc::new(ScriptedRunner::always_ok()));
    let report = executor
        .execute(&request(
            "op-sweep-0001",
            spec(vec![step("fetch", StepKind::ConnectorFetch, &[])]),
        ))
        .unwrap();
    let run_id = report.run.run_id.clone();

    let litter = executor
        .attempts_dir(&run_id)
        .join(format!("{}dead.tmp", ProjectStore::temp_prefix()));
    fs::write(&litter, b"half a record").unwrap();
    assert_eq!(temp_files_under(&harness.root).len(), 1);

    let first = executor.recover_run(&run_id).unwrap();
    assert_eq!(first.stale_temp_files_removed, 1);
    assert!(temp_files_under(&harness.root).is_empty());

    let second = executor.recover_run(&run_id).unwrap();
    assert!(
        !second.repaired(),
        "recovery must be idempotent: {second:?}"
    );
}

#[test]
fn recovering_an_unknown_run_is_an_error() {
    let harness = Harness::new();
    let executor = harness.executor(Arc::new(ScriptedRunner::always_ok()));
    assert!(executor.recover_run("no-such-run").is_err());
}

// ── Acceptance rules ──────────────────────────────────────────────

#[test]
fn an_unevaluable_custom_rule_fails_closed() {
    let harness = Harness::new();
    let executor = harness.executor(Arc::new(ScriptedRunner::always_ok()));
    let mut custom = step("fetch", StepKind::ConnectorFetch, &[]);
    custom.acceptance_rules = vec![AcceptanceRule {
        condition: AcceptanceCondition::CustomRule("p<0.05".into()),
        on_fail: FailAction::Abort,
    }];
    let report = executor
        .execute(&request("op-custom-001", spec(vec![custom])))
        .unwrap();
    assert_eq!(report.state(), WorkflowState::Failed);
    assert!(
        report
            .run
            .failure
            .as_deref()
            .unwrap()
            .contains("cannot be evaluated"),
        "an unevaluable rule must not be reported as passing"
    );
    assert!(report.commits.is_empty());
    assert!(executor.list_commits().unwrap().is_empty());
    assert!(
        fs::read_dir(harness.root.join("workflow-artifacts/sha256"))
            .map(|mut entries| entries.next().is_none())
            .unwrap_or(true),
        "acceptance-rejected bytes were published into the CAS"
    );
}

#[test]
fn an_empty_output_fails_an_output_not_empty_rule() {
    let harness = Harness::new();
    let executor = harness.executor(Arc::new(ScriptedRunner::new(vec![Ok(StepOutput {
        artifacts: BTreeMap::new(),
        exit_code: Some(0),
        bytes_produced: 0,
    })])));
    let mut strict = step("fetch", StepKind::ConnectorFetch, &[]);
    strict.acceptance_rules = vec![AcceptanceRule {
        condition: AcceptanceCondition::OutputNotEmpty,
        on_fail: FailAction::Abort,
    }];
    let report = executor
        .execute(&request("op-empty-0001", spec(vec![strict])))
        .unwrap();
    assert_eq!(report.state(), WorkflowState::Failed);
    assert_eq!(
        report.attempts_for("fetch")[0].error_class,
        Some(ErrorClass::AcceptanceFailed)
    );
}

#[test]
fn a_missing_exit_code_cannot_satisfy_exit_code_zero() {
    let harness = Harness::new();
    let executor = harness.executor(Arc::new(ScriptedRunner::new(vec![Ok(StepOutput {
        artifacts: BTreeMap::from([("out".into(), b"aa".to_vec())]),
        exit_code: None,
        bytes_produced: 1,
    })])));
    let mut strict = step("fetch", StepKind::ConnectorFetch, &[]);
    strict.acceptance_rules = vec![AcceptanceRule {
        condition: AcceptanceCondition::ExitCodeZero,
        on_fail: FailAction::Abort,
    }];
    let report = executor
        .execute(&request("op-noexit-001", spec(vec![strict])))
        .unwrap();
    assert_eq!(report.state(), WorkflowState::Failed);
}

#[test]
fn a_downstream_step_does_not_run_after_an_upstream_failure() {
    let harness = Harness::new();
    let runner = Arc::new(ScriptedRunner::new(vec![Err(StepFailure::permanent(
        ErrorClass::RunnerError,
        "boom",
    ))]));
    let executor = harness.executor(runner.clone());
    let report = executor
        .execute(&request(
            "op-downstream-1",
            spec(vec![
                step("a", StepKind::ConnectorFetch, &[]),
                step("b", StepKind::Renderer, &["a"]),
            ]),
        ))
        .unwrap();
    assert_eq!(report.state(), WorkflowState::Failed);
    assert_eq!(runner.calls(), 1);
    assert!(report.attempts_for("b").is_empty());
}

// ── Durability ────────────────────────────────────────────────────

#[test]
fn a_completed_run_leaves_no_temp_files_and_reloads_from_disk() {
    let harness = Harness::new();
    let executor = harness.executor(Arc::new(ScriptedRunner::always_ok()));
    let report = executor
        .execute(&request(
            "op-durable-001",
            spec(vec![
                step("fetch", StepKind::ConnectorFetch, &[]),
                step("render", StepKind::Renderer, &["fetch"]),
            ]),
        ))
        .unwrap();
    assert!(
        temp_files_under(&harness.root).is_empty(),
        "durable writes left litter: {:?}",
        temp_files_under(&harness.root)
    );

    // A fresh executor over the same root sees the same records.
    let reopened = WorkflowExecutor::new(&harness.root, environment());
    assert_eq!(reopened.load_run(&report.run.run_id).unwrap(), report.run);
    assert_eq!(
        reopened.load_attempts(&report.run.run_id).unwrap(),
        report.attempts
    );
}

/// Four threads race on one operation id. Exactly one executes; the losers
/// either replay the finished run or are told it is in flight. None of them
/// runs the workflow a second time, and none of them marks the live run
/// `Interrupted`.
#[test]
fn concurrent_executions_of_one_operation_id_produce_one_run() {
    let harness = Harness::new();
    let runner = Arc::new(ScriptedRunner::always_ok());
    let executor = harness.executor(runner.clone());
    let workflow = spec(vec![step("fetch", StepKind::ConnectorFetch, &[])]);

    let outcomes: Vec<Result<WorkflowRunReport>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..4)
            .map(|_| {
                let executor = executor.clone();
                let workflow = workflow.clone();
                scope.spawn(move || executor.execute(&request("op-race-000001", workflow)))
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });

    assert_eq!(runner.calls(), 1, "the workflow ran more than once");
    assert_eq!(executor.list_commits().unwrap().len(), 1);

    let mut executed = 0;
    let mut run_ids = BTreeSet::new();
    for outcome in &outcomes {
        match outcome {
            Ok(report) => {
                run_ids.insert(report.run.run_id.clone());
                if !report.replayed {
                    executed += 1;
                }
                assert!(
                    !report.recovered,
                    "a live run was mistaken for a crashed one"
                );
                assert_ne!(report.state(), WorkflowState::Interrupted);
            }
            Err(ScienceError::Invalid(message)) => {
                assert!(message.contains("already in flight"), "{message}");
            }
            Err(other) => panic!("unexpected error: {other}"),
        }
    }
    assert_eq!(executed, 1, "exactly one thread may execute");
    assert_eq!(run_ids.len(), 1);
    assert_eq!(
        executor
            .load_run(&run_ids.iter().next().unwrap().clone())
            .unwrap()
            .state,
        WorkflowState::Succeeded
    );
}

// ── Reuse key ─────────────────────────────────────────────────────

/// `compute_hash` used to ignore parameters, policy and connector version,
/// which would have collided two different units of work onto one commit key.
#[test]
fn reuse_key_hash_covers_every_field_that_matches_compares() {
    let base = ReuseKey {
        input_artifact_hashes: BTreeMap::from([("a".into(), "sha:1".into())]),
        step_implementation_version: "v1".into(),
        parameters: BTreeMap::from([("k".into(), "v".into())]),
        compute_environment_hash: "env:1".into(),
        policy_version: "p1".into(),
        connector_version: Some("pubmed@1".into()),
        renderer_build_id: Some("r1".into()),
    };
    assert_eq!(base.compute_hash(), base.clone().compute_hash());

    let mut different_parameter = base.clone();
    different_parameter.parameters = BTreeMap::from([("k".into(), "other".into())]);
    assert_ne!(base.compute_hash(), different_parameter.compute_hash());
    assert!(!base.matches(&different_parameter));

    let mut different_policy = base.clone();
    different_policy.policy_version = "p2".into();
    assert_ne!(base.compute_hash(), different_policy.compute_hash());
    assert!(!base.matches(&different_policy));

    let mut different_connector = base.clone();
    different_connector.connector_version = Some("pubmed@2".into());
    assert_ne!(base.compute_hash(), different_connector.compute_hash());
    assert!(!base.matches(&different_connector));

    let mut different_renderer = base.clone();
    different_renderer.renderer_build_id = Some("r2".into());
    assert_ne!(base.compute_hash(), different_renderer.compute_hash());
    assert!(!base.matches(&different_renderer));
}

#[test]
fn topological_order_is_deterministic_and_rejects_a_cycle() {
    let workflow = spec(vec![
        step("z", StepKind::Renderer, &["m"]),
        step("m", StepKind::ArtifactTransform, &["a"]),
        step("a", StepKind::ConnectorFetch, &[]),
        step("b", StepKind::ConnectorFetch, &[]),
    ]);
    // 'a' and 'b' are both ready first; lexicographic tie-break makes the
    // order stable across runs.
    assert_eq!(
        workflow.topological_order().unwrap(),
        vec!["a", "b", "m", "z"]
    );
    assert_eq!(
        workflow.topological_order().unwrap(),
        workflow.topological_order().unwrap()
    );

    let cyclic = spec(vec![
        step("a", StepKind::ConnectorFetch, &["b"]),
        step("b", StepKind::Renderer, &["a"]),
    ]);
    assert!(cyclic.topological_order().unwrap_err().contains("cycle"));

    let dangling = spec(vec![step("a", StepKind::ConnectorFetch, &["ghost"])]);
    assert!(dangling.topological_order().is_err());
}
