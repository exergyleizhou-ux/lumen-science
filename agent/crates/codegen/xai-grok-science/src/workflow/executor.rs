//! Workflow execution engine (LS5-K1).
//!
//! ## What was missing
//!
//! `WorkflowSpec` had `validate_dag`, `validate_references` and `ReuseKey`,
//! and `workflow_dry_run` said so honestly — `notes: ["Dry-run only; no real
//! execution"]`. Nothing ran a spec. This module is the thing that runs one.
//!
//! ## Guarantees
//!
//! - **At-least-once step execution, exactly-once accepted-output commit.** An
//!   attempt record is written *before* cache verification or work and
//!   finalised after, so a crash leaves a non-terminal attempt rather than a
//!   silent gap. The executor hashes the runner's exact bytes into its own
//!   immutable CAS. Only accepted output may publish a commit. Deterministic
//!   reuse is scoped to owner, session, project, workflow, step, environment
//!   and policy; NoCache additionally scopes each commit to its run.
//! - **Operation-id deduplication.** Re-executing one operation id replays the
//!   recorded run instead of running the workflow again.
//! - **Bounded retry on an injected clock.** Backoff never reads the wall
//!   clock; [`ManualClock`] makes a retry schedule assertable.
//! - **Cancellation** is checked before every step and every attempt.
//! - **Crash recovery** turns non-terminal attempts into `Interrupted`. It
//!   does not re-run them: an interrupted step may have had a side effect
//!   nobody recorded, and re-running it silently is how you get two of
//!   something.
//! - **Allowlisted step kinds only.** A kind outside the policy allowlist is
//!   refused before the run reaches `Queued`.
//!
//! ## What this module deliberately does not do
//!
//! It never spawns a process and it never builds a shell command line. A step
//! is a typed [`StepPlan`]; kernel work is described by a [`KernelInvocation`]
//! that is argv and nothing else. Turning a plan into a running process is the
//! job of a [`StepRunner`], which is the seam. Without one bound, every step
//! is refused — see [`UnboundStepRunner`].

use super::kernel::{KernelAdmission, KernelKind, KernelManifest, ResourceCap};
use super::{
    AcceptanceCondition, CachePolicy, ComputeEnvironment, FailAction, ReuseKey, StepKind,
    WorkflowSpec, WorkflowStep,
};
use crate::project::capability::PinnedDirectory;
use crate::project::model::ProjectId;
#[cfg(test)]
use crate::project::store::ProjectStore;
use crate::project::store::{HeldProjectRootWriteGuard, write_lock_for};
use crate::{Result, ScienceError};
use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
#[cfg(test)]
use std::fs;
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

pub const WORKFLOW_RUN_SCHEMA_VERSION: u32 = 2;

thread_local! {
    /// Root-lock identities proven held by a typed project-store guard on this
    /// synchronous thread. Workflow execution is synchronous; this scope never
    /// crosses an await point or moves between threads.
    static EXTERNALLY_HELD_ROOT_WRITES: RefCell<Vec<usize>> =
        const { RefCell::new(Vec::new()) };
}

struct ExternallyHeldRootWriteScope {
    lock_identity: usize,
}

impl ExternallyHeldRootWriteScope {
    fn enter(writes: &Arc<Mutex<()>>, held: &HeldProjectRootWriteGuard<'_>) -> Result<Self> {
        if !held.authorizes(writes) {
            return Err(ScienceError::Ownership);
        }
        let lock_identity = Arc::as_ptr(writes) as usize;
        EXTERNALLY_HELD_ROOT_WRITES.with(|held| held.borrow_mut().push(lock_identity));
        Ok(Self { lock_identity })
    }
}

impl Drop for ExternallyHeldRootWriteScope {
    fn drop(&mut self) {
        EXTERNALLY_HELD_ROOT_WRITES.with(|held| {
            let removed = held.borrow_mut().pop();
            debug_assert_eq!(removed, Some(self.lock_identity));
        });
    }
}

fn root_write_is_externally_held(writes: &Arc<Mutex<()>>) -> bool {
    let lock_identity = Arc::as_ptr(writes) as usize;
    EXTERNALLY_HELD_ROOT_WRITES.with(|held| held.borrow().contains(&lock_identity))
}

/// Step ids become directory and file name components, so they are kept to a
/// boring alphabet rather than sanitised after the fact.
fn validate_step_id(step_id: &str) -> std::result::Result<(), String> {
    if step_id.is_empty() || step_id.len() > 64 {
        return Err(format!("step id '{step_id}' must be 1..=64 characters"));
    }
    if !step_id
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
    {
        return Err(format!("step id '{step_id}' must be [A-Za-z0-9._-] only"));
    }
    if step_id == "." || step_id == ".." {
        return Err(format!("step id '{step_id}' is reserved"));
    }
    Ok(())
}

fn validate_run_id(run_id: &str) -> Result<()> {
    if run_id.len() != 32
        || !run_id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(ScienceError::Invalid(
            "workflow run id must be exactly 32 lowercase hex characters".into(),
        ));
    }
    Ok(())
}

fn validate_commit_key(commit_key: &str) -> Result<()> {
    if commit_key.len() != 64
        || !commit_key
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(ScienceError::Invalid(
            "workflow commit key must be exactly 64 lowercase hex characters".into(),
        ));
    }
    Ok(())
}

fn validate_workflow_record_stem(stem: &str, field: &str) -> Result<()> {
    if stem.is_empty()
        || stem.len() > 128
        || stem == "."
        || stem == ".."
        || !stem.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_' || byte == b'.'
        })
    {
        return Err(ScienceError::Invalid(format!(
            "{field} must be 1..=128 [A-Za-z0-9._-] characters and not dot traversal"
        )));
    }
    Ok(())
}

// ── Clock seam ────────────────────────────────────────────────────

/// Time source for the executor.
///
/// Retry backoff and every recorded timestamp go through this, so a test can
/// assert the exact retry schedule instead of sleeping through it.
pub trait Clock: Send + Sync + std::fmt::Debug {
    fn now(&self) -> DateTime<Utc>;
    /// Wait for `duration`. A deterministic clock advances instead of blocking.
    fn sleep(&self, duration: Duration);
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
    fn sleep(&self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

/// A clock that never reads wall time and never blocks.
///
/// `sleep` advances the clock by exactly the requested duration and records
/// it, which is what makes "retried twice with 100ms then 200ms of backoff"
/// something a test can assert rather than infer.
#[derive(Debug, Clone)]
pub struct ManualClock {
    state: Arc<Mutex<ManualClockState>>,
}

#[derive(Debug)]
struct ManualClockState {
    now: DateTime<Utc>,
    slept: Vec<Duration>,
}

impl ManualClock {
    pub fn new(start: DateTime<Utc>) -> Self {
        Self {
            state: Arc::new(Mutex::new(ManualClockState {
                now: start,
                slept: Vec::new(),
            })),
        }
    }

    /// A fixed, arbitrary origin so recorded timestamps are reproducible.
    pub fn at_origin() -> Self {
        Self::new(Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap())
    }

    pub fn advance(&self, duration: Duration) {
        let mut state = self.lock();
        state.now += chrono::Duration::from_std(duration).unwrap_or_default();
    }

    /// Every `sleep` this clock was asked for, in order.
    pub fn slept(&self) -> Vec<Duration> {
        self.lock().slept.clone()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, ManualClockState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl Clock for ManualClock {
    fn now(&self) -> DateTime<Utc> {
        self.lock().now
    }
    fn sleep(&self, duration: Duration) {
        let mut state = self.lock();
        state.slept.push(duration);
        state.now += chrono::Duration::from_std(duration).unwrap_or_default();
    }
}

// ── Step seam ─────────────────────────────────────────────────────

/// The typed work of one step. There is no shell string in this type and
/// there must never be one: a workflow describes operations, not commands.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum StepOperation {
    ConnectorFetch {
        connector_id: String,
    },
    ArtifactTransform {
        transform_id: String,
    },
    /// Kernel work. The invocation is argv against an admitted interpreter.
    /// This crate never spawns it — [`StepRunner`] is the seam that does.
    KernelCell {
        kernel: Box<KernelAdmission>,
        invocation: KernelInvocation,
    },
    Renderer {
        renderer_id: String,
    },
    Reviewer {
        reviewer_id: String,
    },
    HumanApproval {
        approval_key: String,
    },
    Export {
        target: String,
    },
}

/// A kernel process described as argv, never as a command line.
///
/// `argv[0]` is implicit: the process is `interpreter_path` with these
/// arguments. Nothing here is ever concatenated into a string for a shell.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KernelInvocation {
    /// Absolute path recorded by kernel admission, symlinks already resolved.
    ///
    /// This is evidence/display metadata. A production runner must execute the
    /// actor-retained capability whose digest is `executable_sha256`; it must
    /// never reopen this path.
    pub interpreter_path: String,
    /// Digest of the exact executable bytes admitted for this invocation.
    pub executable_sha256: String,
    pub argv: Vec<String>,
    /// Cell body the runner is to feed the kernel, addressed by digest so the
    /// plan stays small and the source stays verifiable.
    pub cell_source_sha256: String,
    pub working_dir: Option<String>,
    /// Exact environment the kernel may see. An empty map means an empty
    /// environment, not an inherited one.
    pub environment: BTreeMap<String, String>,
    /// Required by the admission policy; a runner that cannot honour these
    /// must fail the step rather than run without them.
    pub network_allowed: bool,
    pub process_isolation_required: bool,
    pub resource_cap: ResourceCap,
}

/// Everything the runner is told about one attempt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepPlan {
    pub workflow_id: String,
    pub run_id: String,
    pub step_id: String,
    pub attempt_id: String,
    pub attempt_number: u32,
    pub kind: StepKind,
    pub operation: StepOperation,
    /// Upstream step id → that step's output manifest hash.
    pub inputs: BTreeMap<String, String>,
    pub parameters: BTreeMap<String, String>,
    pub timeout: Duration,
    pub reuse_key_hash: String,
    pub environment_hash: String,
    pub policy_hash: String,
}

/// What a step produced.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StepOutput {
    /// Artifact path → exact bytes.
    ///
    /// A runner is not trusted to supply digests. The executor hashes these
    /// bytes itself and publishes them into its store-owned immutable CAS only
    /// after every acceptance rule has passed.
    pub artifacts: BTreeMap<String, Vec<u8>>,
    pub exit_code: Option<i32>,
    pub bytes_produced: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepFailure {
    pub class: ErrorClass,
    pub detail: String,
    /// Whether the executor may spend another attempt on this.
    pub retryable: bool,
}

impl StepFailure {
    pub fn permanent(class: ErrorClass, detail: impl Into<String>) -> Self {
        Self {
            class,
            detail: detail.into(),
            retryable: false,
        }
    }
    pub fn transient(class: ErrorClass, detail: impl Into<String>) -> Self {
        Self {
            class,
            detail: detail.into(),
            retryable: true,
        }
    }
}

/// The seam between the executor and anything that actually does work.
///
/// Implementations own process spawning, network calls and timeouts. The
/// executor owns ordering, attempts, retry, commit and state.
pub trait StepRunner: std::fmt::Debug + Send + Sync {
    fn run(&self, plan: &StepPlan) -> std::result::Result<StepOutput, StepFailure>;
}

/// The default runner: refuses everything.
///
/// A workflow engine with no runner bound has done no work, and this says so
/// instead of reporting empty success.
#[derive(Debug, Default, Clone, Copy)]
pub struct UnboundStepRunner;

impl StepRunner for UnboundStepRunner {
    fn run(&self, plan: &StepPlan) -> std::result::Result<StepOutput, StepFailure> {
        Err(StepFailure::permanent(
            ErrorClass::NoStepRunnerBound,
            format!(
                "no StepRunner is bound, so step '{}' ({:?}) cannot be executed",
                plan.step_id, plan.kind
            ),
        ))
    }
}

// ── States and classes ────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowState {
    Draft,
    Validated,
    Admitted,
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    Interrupted,
}

impl WorkflowState {
    pub fn terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::Cancelled | Self::Interrupted
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptState {
    Succeeded,
    Failed,
    Cancelled,
    Interrupted,
    /// The step was not run because an equal commit already existed.
    Reused,
    /// The step was not run because its acceptance rule said to skip it.
    Skipped,
}

impl AttemptState {
    pub fn ran_the_step(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorClass {
    SpecInvalid,
    StepKindNotAllowed,
    KernelNotAdmitted,
    NoStepRunnerBound,
    DependencyFailed,
    AcceptanceFailed,
    ApprovalRequired,
    Timeout,
    Cancelled,
    Interrupted,
    RunnerError,
    RetriesExhausted,
    PolicyViolation,
}

// ── Policy ────────────────────────────────────────────────────────

/// What the executor is permitted to run, and how hard it may try.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionPolicy {
    pub policy_version: String,
    /// Kinds outside this set are refused before the run is queued.
    pub allowed_step_kinds: BTreeSet<StepKind>,
    /// Hard ceiling on attempts, whatever a step's own retry policy says.
    pub max_attempts_ceiling: u32,
    pub max_retry_delay: Duration,
    /// Whether a `NotebookCell` step needs an admitted kernel. Off only makes
    /// sense for a runner that is not a kernel at all.
    pub require_admitted_kernel: bool,
}

impl Default for ExecutionPolicy {
    /// `NotebookCell` is deliberately absent: running arbitrary code needs an
    /// explicit decision plus an admitted kernel, not a default.
    fn default() -> Self {
        Self {
            policy_version: "workflow-execution-v1".into(),
            allowed_step_kinds: BTreeSet::from([
                StepKind::ConnectorFetch,
                StepKind::ArtifactTransform,
                StepKind::Renderer,
                StepKind::Reviewer,
                StepKind::HumanApproval,
                StepKind::Export,
            ]),
            max_attempts_ceiling: 5,
            max_retry_delay: Duration::from_secs(60),
            require_admitted_kernel: true,
        }
    }
}

impl ExecutionPolicy {
    /// Allow kernel steps as well. Separate from `default` so that enabling
    /// code execution is a visible call site.
    pub fn allowing_kernel_steps(mut self) -> Self {
        self.allowed_step_kinds.insert(StepKind::NotebookCell);
        self
    }

    pub fn policy_hash(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.policy_version.as_bytes());
        for kind in &self.allowed_step_kinds {
            hasher.update(format!("{kind:?}").as_bytes());
        }
        hasher.update(self.max_attempts_ceiling.to_le_bytes());
        hasher.update(self.max_retry_delay.as_millis().to_le_bytes());
        hasher.update([u8::from(self.require_admitted_kernel)]);
        format!("{:x}", hasher.finalize())
    }
}

// ── Durable records ───────────────────────────────────────────────

/// One attempt at one step. Written before the step runs and finalised after.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepAttempt {
    pub schema_version: u32,
    pub attempt_id: String,
    pub workflow_id: String,
    pub run_id: String,
    pub step_id: String,
    pub attempt_number: u32,
    pub input_manifest_hash: String,
    pub parameter_hash: String,
    pub implementation_hash: String,
    pub environment_hash: String,
    pub policy_hash: String,
    pub reuse_key_hash: String,
    /// Exact durable commit this attempt published or reused. It stays `None`
    /// for rejected, failed, cancelled and interrupted attempts.
    #[serde(default)]
    pub commit_key: Option<String>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    /// `None` while the attempt is in flight. A `None` found on restart is
    /// exactly what crash recovery converts to `Interrupted`.
    pub terminal_state: Option<AttemptState>,
    pub output_manifest_hash: Option<String>,
    pub error_class: Option<ErrorClass>,
    pub error_detail: Option<String>,
}

impl StepAttempt {
    pub fn in_flight(&self) -> bool {
        self.terminal_state.is_none()
    }
}

/// The exactly-once ledger entry for a step's output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactCommitState {
    Prepared,
    Committed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArtifactCommit {
    pub schema_version: u32,
    pub state: ArtifactCommitState,
    pub commit_key: String,
    pub owner_id: String,
    pub session_id: String,
    pub project_id: ProjectId,
    /// The run that originally published this commit. Deterministic reuse may
    /// reference it from a later run, but NoCache keys include this run id.
    pub run_id: String,
    pub workflow_id: String,
    pub step_id: String,
    pub cache_policy: CachePolicy,
    pub reuse_key_hash: String,
    pub output_manifest: BTreeMap<String, String>,
    pub output_manifest_hash: String,
    pub committed_at: DateTime<Utc>,
    /// The attempt that won the commit. A later attempt with the same key
    /// reads this record rather than replacing it.
    pub committed_by_attempt: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowStateTransition {
    pub state: WorkflowState,
    pub at: DateTime<Utc>,
    pub note: Option<String>,
}

/// A step that was refused before the run was queued.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RefusedStep {
    pub step_id: String,
    pub kind: StepKind,
    pub error_class: ErrorClass,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowRunRecord {
    pub schema_version: u32,
    pub run_id: String,
    pub operation_id: String,
    pub session_id: String,
    pub owner_id: String,
    pub workflow_id: String,
    pub project_id: ProjectId,
    pub spec_hash: String,
    pub environment_hash: String,
    pub policy_hash: String,
    pub state: WorkflowState,
    pub state_history: Vec<WorkflowStateTransition>,
    pub step_order: Vec<String>,
    pub refused_steps: Vec<RefusedStep>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub failure: Option<String>,
}

/// The idempotency record for one execution request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowOperationRecord {
    pub schema_version: u32,
    pub operation_id: String,
    pub session_id: String,
    pub owner_id: String,
    pub run_id: String,
    pub workflow_id: String,
    pub reserved_at: DateTime<Utc>,
}

/// What a caller gets back from an execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowRunReport {
    pub run: WorkflowRunRecord,
    pub attempts: Vec<StepAttempt>,
    pub commits: Vec<ArtifactCommit>,
    /// Artifacts this execution committed for the first time.
    pub artifacts_committed: usize,
    /// Steps that found an existing commit and did not run.
    pub steps_reused: usize,
    /// True when the operation id had already been used and this is the
    /// recorded outcome rather than a fresh execution.
    pub replayed: bool,
    /// True when a non-terminal run was found and closed as `Interrupted`.
    pub recovered: bool,
}

impl WorkflowRunReport {
    pub fn state(&self) -> WorkflowState {
        self.run.state
    }
    pub fn attempts_for(&self, step_id: &str) -> Vec<&StepAttempt> {
        self.attempts
            .iter()
            .filter(|a| a.step_id == step_id)
            .collect()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WorkflowRecoveryReport {
    pub run_id: String,
    /// Attempts that were in flight and are now `Interrupted`.
    pub interrupted_attempts: Vec<String>,
    pub run_state_before: Option<WorkflowState>,
    pub run_state_after: Option<WorkflowState>,
    pub stale_temp_files_removed: usize,
}

impl WorkflowRecoveryReport {
    pub fn repaired(&self) -> bool {
        !self.interrupted_attempts.is_empty()
            || self.run_state_before != self.run_state_after
            || self.stale_temp_files_removed > 0
    }
}

/// One execution request.
#[derive(Debug, Clone)]
pub struct WorkflowExecutionRequest {
    /// Idempotency key. Re-using it replays instead of re-running.
    pub operation_id: String,
    pub session_id: String,
    pub owner_id: String,
    pub spec: WorkflowSpec,
}

// ── Executor ──────────────────────────────────────────────────────

/// Runs a [`WorkflowSpec`], durably.
///
/// Records live under the store root, written with the same unique-temp +
/// fsync + atomic-rename + per-root write lock discipline as `ProjectStore`.
#[derive(Debug, Clone)]
pub struct WorkflowExecutor {
    #[cfg(test)]
    root: PathBuf,
    /// Retained once at construction. Every durable workflow record is opened
    /// relative to this capability, never by reopening `root`.
    confined: std::result::Result<Arc<PinnedDirectory>, Arc<str>>,
    clock: Arc<dyn Clock>,
    runner: Arc<dyn StepRunner>,
    policy: ExecutionPolicy,
    environment: ComputeEnvironment,
    kernels: KernelManifest,
    cancel: Arc<AtomicBool>,
    writes: Arc<Mutex<()>>,
}

impl WorkflowExecutor {
    pub fn new(root: impl Into<PathBuf>, environment: ComputeEnvironment) -> Self {
        let root = root.into();
        let confined = PinnedDirectory::open_or_create(&root)
            .map(Arc::new)
            .map_err(|error| Arc::<str>::from(error.to_string()));
        Self::from_capability(root, environment, confined)
    }

    /// Construct a product executor whose retained store root is proven to be
    /// the same directory as a canonical path below `workspace`.
    pub fn new_confined(
        root: impl Into<PathBuf>,
        workspace: &Path,
        environment: ComputeEnvironment,
    ) -> Result<Self> {
        let root = root.into();
        let confined = Arc::new(PinnedDirectory::open_or_create_within(&root, workspace)?);
        Ok(Self::from_capability(root, environment, Ok(confined)))
    }

    /// Construct an executor from the already-retained workflow I/O root.
    ///
    /// `root` is used only as the in-process write-lock key (and by tests for
    /// inspection). Durable records are always opened through `io`'s retained
    /// directory capability, so an approval-time rename or symlink swap cannot
    /// redirect the ledger.
    pub fn from_io(
        root: impl Into<PathBuf>,
        io: &super::WorkflowIoCapability,
        environment: ComputeEnvironment,
    ) -> Self {
        let root = root.into();
        Self::from_capability(root, environment, Ok(io.shared_root()))
    }

    fn from_capability(
        root: PathBuf,
        environment: ComputeEnvironment,
        confined: std::result::Result<Arc<PinnedDirectory>, Arc<str>>,
    ) -> Self {
        let writes = write_lock_for(&root);
        Self {
            #[cfg(test)]
            root,
            confined,
            clock: Arc::new(SystemClock),
            runner: Arc::new(UnboundStepRunner),
            policy: ExecutionPolicy::default(),
            environment,
            kernels: KernelManifest {
                kernels: Vec::new(),
                default_python: None,
                default_r: None,
                default_julia: None,
            },
            cancel: Arc::new(AtomicBool::new(false)),
            writes,
        }
    }

    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.clock = clock;
        self
    }

    pub fn with_runner(mut self, runner: Arc<dyn StepRunner>) -> Self {
        self.runner = runner;
        self
    }

    pub fn with_policy(mut self, policy: ExecutionPolicy) -> Self {
        self.policy = policy;
        self
    }

    /// Kernels this executor may use. Rejected records are refused here so
    /// they cannot reach a step plan.
    pub fn with_kernels(mut self, kernels: KernelManifest) -> Result<Self> {
        for kernel in &kernels.kernels {
            if !kernel.is_safe() {
                return Err(ScienceError::Invalid(format!(
                    "kernel '{}' is not admitted and cannot be given to an executor",
                    kernel.kernel_id
                )));
            }
        }
        self.kernels = kernels;
        Ok(self)
    }

    /// The flag that stops this executor. Trip it from any thread.
    pub fn cancel_token(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancel)
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }

    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    // ── Paths ─────────────────────────────────────────────────────

    fn run_dir(&self, run_id: &str) -> PathBuf {
        PathBuf::from("workflow-runs").join(run_id)
    }
    fn run_path(&self, run_id: &str) -> PathBuf {
        self.run_dir(run_id).join("run.json")
    }
    fn attempts_relative(&self, run_id: &str) -> PathBuf {
        self.run_dir(run_id).join("attempts")
    }
    #[cfg(test)]
    fn attempts_dir(&self, run_id: &str) -> PathBuf {
        self.root.join(self.attempts_relative(run_id))
    }
    fn attempt_path(&self, run_id: &str, attempt_id: &str) -> PathBuf {
        self.attempts_relative(run_id)
            .join(format!("{attempt_id}.json"))
    }
    fn commit_path(&self, commit_key: &str) -> PathBuf {
        PathBuf::from("workflow-commits").join(format!("{commit_key}.json"))
    }
    fn artifact_path(&self, sha256: &str) -> PathBuf {
        PathBuf::from("workflow-artifacts")
            .join("sha256")
            .join(sha256)
    }
    fn operation_path(&self, operation_id: &str) -> PathBuf {
        PathBuf::from("workflow-operations").join(format!("{operation_id}.json"))
    }

    fn guard(&self) -> Result<Option<std::sync::MutexGuard<'_, ()>>> {
        if root_write_is_externally_held(&self.writes) {
            return Ok(None);
        }
        self.writes
            .lock()
            .map(Some)
            .map_err(|_| ScienceError::Invalid("workflow store write lock poisoned".into()))
    }

    fn confined(&self) -> Result<&PinnedDirectory> {
        self.confined.as_deref().map_err(|error| {
            ScienceError::Invalid(format!("workflow store capability is unavailable: {error}"))
        })
    }

    fn read_record<T: for<'de> Deserialize<'de>>(&self, path: &Path) -> Result<Option<T>> {
        self.confined()?
            .read_optional(path)?
            .map(|bytes| Ok(serde_json::from_slice(&bytes)?))
            .transpose()
    }

    fn require_record<T: for<'de> Deserialize<'de>>(
        &self,
        path: &Path,
        description: &str,
    ) -> Result<T> {
        self.read_record(path)?
            .ok_or_else(|| ScienceError::Invalid(format!("{description} not found")))
    }

    fn write_record<T: Serialize>(&self, path: &Path, value: &T) -> Result<()> {
        let _guard = self.guard()?;
        self.confined()?
            .replace_atomic(path, &serde_json::to_vec_pretty(value)?)
    }

    // ── Public API ────────────────────────────────────────────────

    /// Execute while a [`ProjectStore`](crate::project::ProjectStore) holds
    /// the write lock for this executor's exact retained root.
    ///
    /// `std::sync::Mutex` is not reentrant. The typed guard proves that
    /// project-owner/revision validation already holds the same lock, so the
    /// executor may use that outer critical section instead of deadlocking on
    /// its normal per-write acquisitions. A token from any other root is
    /// rejected and no workflow record is created.
    pub fn execute_with_project_guard(
        &self,
        held: &HeldProjectRootWriteGuard<'_>,
        request: &WorkflowExecutionRequest,
    ) -> Result<WorkflowRunReport> {
        self.with_project_guard(held, |executor| executor.execute(request))
    }

    /// Run a synchronous executor operation under an already-held project
    /// root guard.
    ///
    /// This wider seam is needed when execution is immediately followed by
    /// commit-ledger reads during an actor authority commit: those reads use
    /// the same root lock and must remain inside the typed external-lock scope
    /// as well.
    pub fn with_project_guard<T>(
        &self,
        held: &HeldProjectRootWriteGuard<'_>,
        operation: impl FnOnce(&Self) -> Result<T>,
    ) -> Result<T> {
        let _scope = ExternallyHeldRootWriteScope::enter(&self.writes, held)?;
        operation(self)
    }

    /// Execute a workflow, or replay an already-executed operation id.
    pub fn execute(&self, request: &WorkflowExecutionRequest) -> Result<WorkflowRunReport> {
        crate::project::mutation::validate_operation_id(&request.operation_id)?;
        if request.session_id.is_empty() || request.owner_id.is_empty() {
            return Err(ScienceError::Invalid(
                "workflow execution requires a session id and owner id".into(),
            ));
        }

        let run_id = derive_run_id(&request.operation_id);

        // Reserve and publish the Draft record under one guard. Without this,
        // a second caller could see the reservation file and then fail to find
        // the run record the winner had not written yet.
        let outcome = {
            let _guard = self.guard()?;
            match self.reserve_operation(request, &run_id)? {
                OperationReservation::Fresh => {
                    let run = self.new_draft(request, &run_id)?;
                    self.confined()?.replace_atomic(
                        &self.run_path(&run_id),
                        &serde_json::to_vec_pretty(&run)?,
                    )?;
                    OperationOutcome::Fresh(Box::new(run))
                }
                OperationReservation::Replay(record) => {
                    validate_run_id(&record.run_id)?;
                    let run: WorkflowRunRecord = self
                        .require_record(&self.run_path(&record.run_id), "reserved workflow run")?;
                    self.validate_run_record(&run, &record.run_id)?;
                    let requested_spec_hash = spec_hash(&request.spec)?;
                    let requested_environment_hash = self.environment.identity_hash();
                    let requested_policy_hash = self.policy.policy_hash();
                    if run.run_id != record.run_id
                        || run.operation_id != record.operation_id
                        || run.session_id != record.session_id
                        || run.owner_id != record.owner_id
                        || run.workflow_id != record.workflow_id
                    {
                        return Err(ScienceError::Ownership);
                    }
                    if record.session_id != request.session_id
                        || record.owner_id != request.owner_id
                    {
                        return Err(ScienceError::Ownership);
                    }
                    if record.workflow_id != request.spec.workflow_id
                        || run.workflow_id != request.spec.workflow_id
                        || run.project_id != request.spec.project_id
                        || run.spec_hash != requested_spec_hash
                        || run.environment_hash != requested_environment_hash
                        || run.policy_hash != requested_policy_hash
                    {
                        return Err(ScienceError::Invalid(format!(
                            "workflow operation {} replay does not match its original authority request",
                            request.operation_id
                        )));
                    }
                    OperationOutcome::Replay(Box::new(record), Box::new(run))
                }
            }
        };

        match outcome {
            OperationOutcome::Fresh(run) => {
                let lease = RunLease::acquire(&run_id);
                let report = self.execute_fresh(request, *run);
                drop(lease);
                report
            }
            OperationOutcome::Replay(record, run) => {
                if run.state.terminal() {
                    let mut report = self.build_report(&run)?;
                    report.replayed = true;
                    return Ok(report);
                }
                // Non-terminal. Either another execution in this process owns
                // it right now, or the process that owned it is gone.
                if RunLease::is_held(&record.run_id) {
                    return Err(ScienceError::Invalid(format!(
                        "workflow run {} is already in flight under operation {}",
                        record.run_id, record.operation_id
                    )));
                }
                // No live owner here, so this is a restart after a crash.
                // Close the run out; do not run the workflow again behind the
                // caller's back.
                //
                // CAVEAT: the lease is process-local. Two *processes* sharing
                // one store and one operation id would not see each other's
                // lease; a cross-process lease is not in this engine.
                self.recover_run(&record.run_id)?;
                let run = self.load_run(&record.run_id)?;
                let mut report = self.build_report(&run)?;
                report.replayed = true;
                report.recovered = true;
                Ok(report)
            }
        }
    }

    /// Close out a run that was interrupted mid-flight.
    ///
    /// In-flight attempts become `Interrupted` and are **not** re-executed:
    /// the step may already have had an effect that was never recorded.
    pub fn recover_run(&self, run_id: &str) -> Result<WorkflowRecoveryReport> {
        validate_run_id(run_id)?;
        let mut report = WorkflowRecoveryReport {
            run_id: run_id.to_string(),
            ..Default::default()
        };
        let run_path = self.run_path(run_id);
        let Some(mut run): Option<WorkflowRunRecord> = self.read_record(&run_path)? else {
            return Err(ScienceError::Invalid(format!(
                "workflow run not found: {run_id}"
            )));
        };
        self.validate_run_record(&run, run_id)?;

        for mut attempt in self.load_attempts(run_id)? {
            if attempt.in_flight() {
                attempt.terminal_state = Some(AttemptState::Interrupted);
                attempt.error_class = Some(ErrorClass::Interrupted);
                attempt.error_detail =
                    Some("attempt was in flight when the process stopped; not re-executed".into());
                attempt.finished_at = Some(self.clock.now());
                let path = self.attempt_path(run_id, &attempt.attempt_id);
                self.write_record(&path, &attempt)?;
                report.interrupted_attempts.push(attempt.attempt_id.clone());
            }
        }

        report.run_state_before = Some(run.state);
        if !run.state.terminal() {
            let at = self.clock.now();
            run.state = WorkflowState::Interrupted;
            run.finished_at = Some(at);
            run.failure = Some("run was interrupted; steps were not re-executed".into());
            run.state_history.push(WorkflowStateTransition {
                state: WorkflowState::Interrupted,
                at,
                note: Some("crash recovery".into()),
            });
            self.write_record(&run_path, &run)?;
        }
        report.run_state_after = Some(run.state);
        report.stale_temp_files_removed = self.sweep_temp_files(&self.run_dir(run_id))?
            + self.sweep_temp_files(&self.attempts_relative(run_id))?;
        Ok(report)
    }

    pub fn load_run(&self, run_id: &str) -> Result<WorkflowRunRecord> {
        validate_run_id(run_id)?;
        let path = self.run_path(run_id);
        let run: WorkflowRunRecord = self.require_record(&path, "workflow run")?;
        self.validate_run_record(&run, run_id)?;
        Ok(run)
    }

    pub fn load_attempts(&self, run_id: &str) -> Result<Vec<StepAttempt>> {
        validate_run_id(run_id)?;
        let dir = self.attempts_relative(run_id);
        let mut attempts = Vec::new();
        for name in self.confined()?.list_names(&dir)? {
            let Some(name_text) = name.to_str() else {
                return Err(ScienceError::Invalid(
                    "workflow attempt file name must be UTF-8".into(),
                ));
            };
            let Some(stem) = name_text.strip_suffix(".json") else {
                continue;
            };
            validate_workflow_record_stem(stem, "workflow attempt id")?;
            let path = dir.join(&name);
            let attempt: StepAttempt =
                self.require_record(&path, "listed workflow attempt record")?;
            self.validate_attempt_record(&attempt, run_id, stem)?;
            attempts.push(attempt);
        }
        attempts.sort_by(|a, b| {
            (a.step_id.as_str(), a.attempt_number).cmp(&(b.step_id.as_str(), b.attempt_number))
        });
        Ok(attempts)
    }

    pub fn load_commit(&self, commit_key: &str) -> Result<Option<ArtifactCommit>> {
        validate_commit_key(commit_key)?;
        let _guard = self.guard()?;
        let path = self.commit_path(commit_key);
        let Some(commit): Option<ArtifactCommit> = self.read_record(&path)? else {
            return Ok(None);
        };
        Ok(Some(self.resolve_commit_locked(commit, commit_key)?))
    }

    /// Reopen one immutable artifact through the retained workflow-store
    /// capability and verify that its physical bytes still match the commit.
    ///
    /// SessionActor uses this seam to register workflow outputs in the
    /// ScienceStore authority run. Returning bytes rather than a store path
    /// prevents protocol adapters from bypassing the confined CAS.
    pub fn committed_artifact_bytes(
        &self,
        commit_key: &str,
        artifact_name: &str,
    ) -> Result<Vec<u8>> {
        let commit = self
            .load_commit(commit_key)?
            .ok_or_else(|| ScienceError::Invalid("workflow artifact commit not found".into()))?;
        let digest = commit.output_manifest.get(artifact_name).ok_or_else(|| {
            ScienceError::Invalid(format!(
                "workflow artifact '{artifact_name}' is not in commit {commit_key}"
            ))
        })?;
        let bytes = self
            .confined()?
            .read_optional(&self.artifact_path(digest))?
            .ok_or_else(|| {
                ScienceError::Invalid(format!(
                    "workflow artifact '{artifact_name}' ({digest}) is missing from the immutable store"
                ))
            })?;
        let actual = hex_sha256(&bytes);
        if actual != *digest {
            return Err(ScienceError::Invalid(format!(
                "workflow artifact '{artifact_name}' expected {digest}, but stored bytes hash to {actual}"
            )));
        }
        Ok(bytes)
    }

    /// The reservation an operation id already holds, if any.
    ///
    /// Read-only: it reserves nothing and executes nothing. A caller that must
    /// decide *before* running whether a request is a retry — an approval gate,
    /// say, which should not prompt a second time for work already done — needs
    /// to ask that question without also committing to the run. `execute` is
    /// still the authority on what a replay returns; this only says whether one
    /// would happen.
    pub fn lookup_operation(&self, operation_id: &str) -> Result<Option<WorkflowOperationRecord>> {
        // The id names a path component, so it is validated here exactly as
        // `execute` validates it. A public lookup that skipped this would turn
        // an operation id into a directory traversal.
        crate::project::mutation::validate_operation_id(operation_id)?;
        let path = self.operation_path(operation_id);
        let operation: Option<WorkflowOperationRecord> = self.read_record(&path)?;
        if let Some(record) = &operation {
            self.validate_operation_record(record, operation_id)?;
        }
        Ok(operation)
    }

    /// Every artifact commit under this root, oldest key first.
    pub fn list_commits(&self) -> Result<Vec<ArtifactCommit>> {
        let dir = PathBuf::from("workflow-commits");
        let mut commits = Vec::new();
        for name in self.confined()?.list_names(&dir)? {
            let Some(name_text) = name.to_str() else {
                return Err(ScienceError::Invalid(
                    "workflow commit file name must be UTF-8".into(),
                ));
            };
            let Some(stem) = name_text.strip_suffix(".json") else {
                continue;
            };
            validate_commit_key(stem)?;
            let commit = self.load_commit(stem)?.ok_or_else(|| {
                ScienceError::Invalid(format!("listed workflow commit {stem} disappeared"))
            })?;
            commits.push(commit);
        }
        commits.sort_by(|left, right| left.commit_key.cmp(&right.commit_key));
        Ok(commits)
    }

    // ── Operation ledger ──────────────────────────────────────────

    fn reserve_operation(
        &self,
        request: &WorkflowExecutionRequest,
        run_id: &str,
    ) -> Result<OperationReservation> {
        crate::project::mutation::validate_operation_id(&request.operation_id)?;
        validate_run_id(run_id)?;
        let path = self.operation_path(&request.operation_id);
        let record = WorkflowOperationRecord {
            schema_version: WORKFLOW_RUN_SCHEMA_VERSION,
            operation_id: request.operation_id.clone(),
            session_id: request.session_id.clone(),
            owner_id: request.owner_id.clone(),
            run_id: run_id.to_string(),
            workflow_id: request.spec.workflow_id.clone(),
            reserved_at: self.clock.now(),
        };
        let bytes = serde_json::to_vec_pretty(&record)?;
        // `create_new` is the reservation: the filesystem, not a lock we hold,
        // decides which of two racing executions owns this operation id.
        match self.confined()?.write_new_atomic(&path, &bytes) {
            Ok(()) => Ok(OperationReservation::Fresh),
            Err(ScienceError::Io(error)) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let existing: WorkflowOperationRecord =
                    self.require_record(&path, "reserved workflow operation")?;
                self.validate_operation_record(&existing, &request.operation_id)?;
                Ok(OperationReservation::Replay(existing))
            }
            Err(error) => Err(error),
        }
    }

    // ── Execution ─────────────────────────────────────────────────

    /// The `Draft` record for a fresh run. Written by `execute` while it still
    /// holds the reservation guard.
    fn new_draft(
        &self,
        request: &WorkflowExecutionRequest,
        run_id: &str,
    ) -> Result<WorkflowRunRecord> {
        let spec = &request.spec;
        let started_at = self.clock.now();
        Ok(WorkflowRunRecord {
            schema_version: WORKFLOW_RUN_SCHEMA_VERSION,
            run_id: run_id.to_string(),
            operation_id: request.operation_id.clone(),
            session_id: request.session_id.clone(),
            owner_id: request.owner_id.clone(),
            workflow_id: spec.workflow_id.clone(),
            project_id: spec.project_id.clone(),
            spec_hash: spec_hash(spec)?,
            environment_hash: self.environment.identity_hash(),
            policy_hash: self.policy.policy_hash(),
            state: WorkflowState::Draft,
            state_history: vec![WorkflowStateTransition {
                state: WorkflowState::Draft,
                at: started_at,
                note: None,
            }],
            step_order: Vec::new(),
            refused_steps: Vec::new(),
            started_at,
            finished_at: None,
            failure: None,
        })
    }

    fn execute_fresh(
        &self,
        request: &WorkflowExecutionRequest,
        mut run: WorkflowRunRecord,
    ) -> Result<WorkflowRunReport> {
        let spec = &request.spec;

        // Draft → Validated.
        for step in &spec.steps {
            if let Err(detail) = validate_step_id(&step.step_id) {
                return self.finish(&mut run, WorkflowState::Failed, Some(detail));
            }
        }
        let step_order = match spec.topological_order() {
            Ok(order) => order,
            Err(detail) => {
                return self.finish(
                    &mut run,
                    WorkflowState::Failed,
                    Some(format!("spec invalid: {detail}")),
                );
            }
        };
        if let Err(detail) = spec.validate_dag() {
            return self.finish(
                &mut run,
                WorkflowState::Failed,
                Some(format!("spec invalid: {detail}")),
            );
        }
        run.step_order = step_order.clone();
        self.persist(&mut run, WorkflowState::Validated, None)?;

        // Validated → Admitted. Every step kind must be allowlisted, and a
        // kernel step must have an admitted kernel behind it.
        let by_id: BTreeMap<&str, &WorkflowStep> = spec
            .steps
            .iter()
            .map(|step| (step.step_id.as_str(), step))
            .collect();
        let mut operations: BTreeMap<String, StepOperation> = BTreeMap::new();
        for step_id in &step_order {
            let step = by_id[step_id.as_str()];
            if step.cache_policy == CachePolicy::LocalReuse {
                run.refused_steps.push(RefusedStep {
                    step_id: step.step_id.clone(),
                    kind: step.kind,
                    error_class: ErrorClass::PolicyViolation,
                    detail: "LocalReuse has no authority-scoped durable semantics and is disabled"
                        .into(),
                });
                continue;
            }
            if !self.policy.allowed_step_kinds.contains(&step.kind) {
                run.refused_steps.push(RefusedStep {
                    step_id: step.step_id.clone(),
                    kind: step.kind,
                    error_class: ErrorClass::StepKindNotAllowed,
                    detail: format!(
                        "step kind {:?} is not in the execution allowlist",
                        step.kind
                    ),
                });
                continue;
            }
            match self.build_operation(step) {
                Ok(operation) => {
                    operations.insert(step.step_id.clone(), operation);
                }
                Err(refusal) => run.refused_steps.push(refusal),
            }
        }
        if !run.refused_steps.is_empty() {
            let detail = run
                .refused_steps
                .iter()
                .map(|r| format!("{}: {}", r.step_id, r.detail))
                .collect::<Vec<_>>()
                .join("; ");
            return self.finish(&mut run, WorkflowState::Failed, Some(detail));
        }
        self.persist(&mut run, WorkflowState::Admitted, None)?;
        self.persist(&mut run, WorkflowState::Queued, None)?;

        if self.cancelled() {
            return self.finish(
                &mut run,
                WorkflowState::Cancelled,
                Some("cancelled before the first step".into()),
            );
        }
        self.persist(&mut run, WorkflowState::Running, None)?;

        // Running → terminal.
        let budget = Duration::from_secs(spec.resources.max_total_duration_secs);
        let mut outputs: BTreeMap<String, String> = BTreeMap::new();
        for step_id in &step_order {
            if self.cancelled() {
                return self.finish(
                    &mut run,
                    WorkflowState::Cancelled,
                    Some(format!("cancelled before step '{step_id}'")),
                );
            }
            if !budget.is_zero() {
                let elapsed = self.clock.now() - run.started_at;
                if elapsed.to_std().unwrap_or_default() > budget {
                    return self.finish(
                        &mut run,
                        WorkflowState::Failed,
                        Some(format!(
                            "workflow exceeded its {}s budget before step '{step_id}'",
                            spec.resources.max_total_duration_secs
                        )),
                    );
                }
            }

            let step = by_id[step_id.as_str()];
            let operation = operations[step_id].clone();
            let step_outcome = match self.run_step(spec, step, operation, &run, &outputs) {
                Ok(outcome) => outcome,
                Err(error) => {
                    return self.finish(
                        &mut run,
                        WorkflowState::Failed,
                        Some(format!(
                            "step '{step_id}' failed while publishing durable output: {error}"
                        )),
                    );
                }
            };
            match step_outcome {
                StepOutcome::Succeeded {
                    output_manifest_hash,
                } => {
                    outputs.insert(step.step_id.clone(), output_manifest_hash);
                }
                StepOutcome::Cancelled => {
                    return self.finish(
                        &mut run,
                        WorkflowState::Cancelled,
                        Some(format!("cancelled during step '{step_id}'")),
                    );
                }
                StepOutcome::Failed { class, detail } => {
                    return self.finish(
                        &mut run,
                        WorkflowState::Failed,
                        Some(format!("step '{step_id}' failed ({class:?}): {detail}")),
                    );
                }
            }
        }

        self.finish(&mut run, WorkflowState::Succeeded, None)
    }

    /// Build the typed operation for a step, or refuse it.
    fn build_operation(
        &self,
        step: &WorkflowStep,
    ) -> std::result::Result<StepOperation, RefusedStep> {
        let refuse = |class: ErrorClass, detail: String| RefusedStep {
            step_id: step.step_id.clone(),
            kind: step.kind,
            error_class: class,
            detail,
        };
        match step.kind {
            StepKind::ConnectorFetch => Ok(StepOperation::ConnectorFetch {
                connector_id: step.connector_id.clone().ok_or_else(|| {
                    refuse(
                        ErrorClass::SpecInvalid,
                        "connector_fetch step needs a connector_id".into(),
                    )
                })?,
            }),
            StepKind::ArtifactTransform => Ok(StepOperation::ArtifactTransform {
                transform_id: step
                    .parameters
                    .get("transform_id")
                    .cloned()
                    .unwrap_or_else(|| step.step_id.clone()),
            }),
            StepKind::NotebookCell => {
                let cell = step.notebook_cell.clone().ok_or_else(|| {
                    refuse(
                        ErrorClass::SpecInvalid,
                        "notebook_cell step needs a notebook_cell".into(),
                    )
                })?;
                let kind = match step.parameters.get("kernel_kind").map(String::as_str) {
                    Some("r") | Some("R") => KernelKind::R,
                    Some("julia") => KernelKind::Julia,
                    _ => KernelKind::Python,
                };
                let kernel = self.kernels.find_admitted(kind).ok_or_else(|| {
                    refuse(
                        ErrorClass::KernelNotAdmitted,
                        format!("no admitted {kind:?} kernel is available to this executor"),
                    )
                })?;
                if self.policy.require_admitted_kernel && !kernel.is_safe() {
                    return Err(refuse(
                        ErrorClass::KernelNotAdmitted,
                        format!("kernel '{}' is not admitted", kernel.kernel_id),
                    ));
                }
                Ok(StepOperation::KernelCell {
                    kernel: Box::new(kernel.clone()),
                    invocation: KernelInvocation {
                        interpreter_path: kernel.interpreter_path.clone(),
                        executable_sha256: kernel.executable_hash.clone(),
                        // Empty on purpose, and argv rather than a command
                        // line either way — nothing is ever interpolated into a
                        // shell string. The RUNNER decides how to drive the
                        // interpreter, because only it knows which driver it
                        // uses; PythonLoopRunner passes its exec-loop script
                        // here. A `-` ("read program from stdin") would be a
                        // stray argument to that script, and this field is not
                        // the place to guess the runner's invocation.
                        argv: Vec::new(),
                        cell_source_sha256: hex_sha256(cell.as_bytes()),
                        working_dir: None,
                        environment: BTreeMap::new(),
                        network_allowed: !kernel.default_no_network,
                        process_isolation_required: kernel.process_isolation,
                        resource_cap: kernel.resource_cap,
                    },
                })
            }
            StepKind::Renderer => Ok(StepOperation::Renderer {
                renderer_id: step
                    .parameters
                    .get("renderer_id")
                    .cloned()
                    .unwrap_or_else(|| step.step_id.clone()),
            }),
            StepKind::Reviewer => Ok(StepOperation::Reviewer {
                reviewer_id: step
                    .parameters
                    .get("reviewer_id")
                    .cloned()
                    .unwrap_or_else(|| step.step_id.clone()),
            }),
            StepKind::HumanApproval => Ok(StepOperation::HumanApproval {
                approval_key: step
                    .parameters
                    .get("approval_key")
                    .cloned()
                    .unwrap_or_else(|| step.step_id.clone()),
            }),
            StepKind::Export => Ok(StepOperation::Export {
                target: step
                    .parameters
                    .get("target")
                    .cloned()
                    .unwrap_or_else(|| step.step_id.clone()),
            }),
        }
    }

    #[allow(clippy::too_many_lines)]
    fn run_step(
        &self,
        spec: &WorkflowSpec,
        step: &WorkflowStep,
        operation: StepOperation,
        run: &WorkflowRunRecord,
        outputs: &BTreeMap<String, String>,
    ) -> Result<StepOutcome> {
        // Hashes that identify this unit of work.
        let inputs: BTreeMap<String, String> = step
            .inputs
            .iter()
            .map(|id| {
                (
                    id.clone(),
                    outputs.get(id).cloned().unwrap_or_else(|| "<none>".into()),
                )
            })
            .collect();
        let input_manifest_hash = hash_map(&inputs);
        let mut parameters = spec.parameters.clone();
        parameters.extend(step.parameters.clone());
        let parameter_hash = hash_map(&parameters);
        let implementation_hash = execution_contract_hash(spec, step, &operation)?;
        let reuse_key = ReuseKey {
            input_artifact_hashes: inputs.clone(),
            step_implementation_version: implementation_hash.clone(),
            parameters: parameters.clone(),
            compute_environment_hash: run.environment_hash.clone(),
            policy_version: run.policy_hash.clone(),
            connector_version: step.connector_id.clone(),
            renderer_build_id: None,
        };
        let reuse_key_hash = reuse_key.compute_hash();
        let commit_key = commit_key_for(
            &run.owner_id,
            &run.session_id,
            &spec.project_id,
            &spec.workflow_id,
            &step.step_id,
            step.cache_policy,
            &reuse_key_hash,
            &run.run_id,
        );

        let max_attempts = step
            .retry_policy
            .as_ref()
            .map(|policy| policy.max_attempts.max(1))
            .unwrap_or(1)
            .min(self.policy.max_attempts_ceiling.max(1));

        let mut attempt_number = 0u32;

        loop {
            attempt_number += 1;
            if self.cancelled() {
                return Ok(StepOutcome::Cancelled);
            }

            let attempt_id = attempt_id(&run.run_id, &step.step_id, attempt_number);
            let mut attempt = StepAttempt {
                schema_version: WORKFLOW_RUN_SCHEMA_VERSION,
                attempt_id: attempt_id.clone(),
                workflow_id: spec.workflow_id.clone(),
                run_id: run.run_id.clone(),
                step_id: step.step_id.clone(),
                attempt_number,
                input_manifest_hash: input_manifest_hash.clone(),
                parameter_hash: parameter_hash.clone(),
                implementation_hash: implementation_hash.clone(),
                environment_hash: run.environment_hash.clone(),
                policy_hash: run.policy_hash.clone(),
                reuse_key_hash: reuse_key_hash.clone(),
                commit_key: None,
                started_at: self.clock.now(),
                finished_at: None,
                terminal_state: None,
                output_manifest_hash: None,
                error_class: None,
                error_detail: None,
            };
            let attempt_path = self.attempt_path(&run.run_id, &attempt_id);

            // Durable *before* either cache verification or work. A corrupt
            // reusable commit is itself a failed attempt, not an unrecorded
            // executor error.
            self.write_record(&attempt_path, &attempt)?;

            // A deterministic-reuse step whose commit already exists does not
            // run at all: the artifact is already there and is the same one.
            let reusable = if step.cache_policy == CachePolicy::DeterministicReuse {
                match self.load_reusable_commit(&commit_key, run, step, &reuse_key_hash) {
                    Ok(existing) => existing,
                    Err(error) => {
                        attempt.terminal_state = Some(AttemptState::Failed);
                        attempt.error_class = Some(ErrorClass::PolicyViolation);
                        attempt.error_detail =
                            Some(format!("artifact reuse failed closed: {error}"));
                        attempt.finished_at = Some(self.clock.now());
                        self.write_record(&attempt_path, &attempt)?;
                        return Err(error);
                    }
                }
            } else {
                None
            };
            if let Some(existing) = reusable {
                attempt.terminal_state = Some(AttemptState::Reused);
                attempt.commit_key = Some(existing.commit_key.clone());
                attempt.output_manifest_hash = Some(existing.output_manifest_hash.clone());
                attempt.finished_at = Some(self.clock.now());
                self.write_record(&attempt_path, &attempt)?;
                return Ok(StepOutcome::Succeeded {
                    output_manifest_hash: existing.output_manifest_hash,
                });
            }

            let plan = StepPlan {
                workflow_id: spec.workflow_id.clone(),
                run_id: run.run_id.clone(),
                step_id: step.step_id.clone(),
                attempt_id: attempt_id.clone(),
                attempt_number,
                kind: step.kind,
                operation: operation.clone(),
                inputs: inputs.clone(),
                parameters: parameters.clone(),
                timeout: Duration::from_secs(step.timeout_secs),
                reuse_key_hash: reuse_key_hash.clone(),
                environment_hash: run.environment_hash.clone(),
                policy_hash: run.policy_hash.clone(),
            };

            let outcome = self.runner.run(&plan);

            if self.cancelled() {
                attempt.terminal_state = Some(AttemptState::Cancelled);
                attempt.error_class = Some(ErrorClass::Cancelled);
                attempt.finished_at = Some(self.clock.now());
                self.write_record(&attempt_path, &attempt)?;
                return Ok(StepOutcome::Cancelled);
            }

            // Each arm either finishes the step with a `return`, or yields
            // the retryable failure that the exhaustion check below reports.
            let retryable_failure: (ErrorClass, String) = match outcome {
                Ok(output) => {
                    if let Err(detail) = validate_step_output(spec, &output) {
                        attempt.terminal_state = Some(AttemptState::Failed);
                        attempt.error_class = Some(ErrorClass::PolicyViolation);
                        attempt.error_detail = Some(detail.clone());
                        attempt.finished_at = Some(self.clock.now());
                        self.write_record(&attempt_path, &attempt)?;
                        return Ok(StepOutcome::Failed {
                            class: ErrorClass::PolicyViolation,
                            detail,
                        });
                    }
                    match evaluate_acceptance(step, &output) {
                        AcceptanceVerdict::Accepted => {
                            let commit = match self.commit_output(
                                &commit_key,
                                run,
                                step,
                                &reuse_key_hash,
                                &attempt_id,
                                &output.artifacts,
                            ) {
                                Ok(commit) => commit,
                                Err(error) => {
                                    // A prepared record is an accepted-output
                                    // certificate whose recovery must remain
                                    // possible. Do not overwrite its in-flight
                                    // origin with Failed and permanently poison
                                    // the immutable key.
                                    if self
                                        .read_record::<ArtifactCommit>(
                                            &self.commit_path(&commit_key),
                                        )?
                                        .is_none()
                                    {
                                        attempt.terminal_state = Some(AttemptState::Failed);
                                        attempt.error_class = Some(ErrorClass::PolicyViolation);
                                        attempt.error_detail = Some(format!(
                                            "artifact publication failed closed: {error}"
                                        ));
                                        attempt.finished_at = Some(self.clock.now());
                                        self.write_record(&attempt_path, &attempt)?;
                                    }
                                    return Err(error);
                                }
                            };
                            if commit.committed_by_attempt != attempt_id {
                                // Another accepted attempt won the exact same
                                // commit while this runner was executing. This
                                // attempt still ran, so record Succeeded (not
                                // Reused) while pointing at the winner.
                                attempt.commit_key = Some(commit.commit_key.clone());
                                attempt.output_manifest_hash =
                                    Some(commit.output_manifest_hash.clone());
                                attempt.terminal_state = Some(AttemptState::Succeeded);
                                attempt.finished_at = Some(self.clock.now());
                                self.write_record(&attempt_path, &attempt)?;
                            }
                            return Ok(StepOutcome::Succeeded {
                                output_manifest_hash: commit.output_manifest_hash,
                            });
                        }
                        AcceptanceVerdict::Rejected { detail, on_fail } => {
                            attempt.error_class = Some(ErrorClass::AcceptanceFailed);
                            attempt.error_detail = Some(detail.clone());
                            attempt.finished_at = Some(self.clock.now());
                            match on_fail {
                                FailAction::Skip => {
                                    attempt.terminal_state = Some(AttemptState::Skipped);
                                    self.write_record(&attempt_path, &attempt)?;
                                    return Ok(StepOutcome::Succeeded {
                                        output_manifest_hash: hash_map(&BTreeMap::new()),
                                    });
                                }
                                FailAction::PauseForApproval => {
                                    attempt.terminal_state = Some(AttemptState::Failed);
                                    attempt.error_class = Some(ErrorClass::ApprovalRequired);
                                    self.write_record(&attempt_path, &attempt)?;
                                    return Ok(StepOutcome::Failed {
                                        class: ErrorClass::ApprovalRequired,
                                        detail: format!(
                                            "{detail}; human approval is required and this \
                                             engine has no approval state to wait in"
                                        ),
                                    });
                                }
                                FailAction::Abort => {
                                    attempt.terminal_state = Some(AttemptState::Failed);
                                    self.write_record(&attempt_path, &attempt)?;
                                    return Ok(StepOutcome::Failed {
                                        class: ErrorClass::AcceptanceFailed,
                                        detail,
                                    });
                                }
                                FailAction::Retry => {
                                    attempt.terminal_state = Some(AttemptState::Failed);
                                    self.write_record(&attempt_path, &attempt)?;
                                    (ErrorClass::AcceptanceFailed, detail)
                                }
                            }
                        }
                    }
                }
                Err(failure) => {
                    attempt.terminal_state = Some(AttemptState::Failed);
                    attempt.error_class = Some(failure.class);
                    attempt.error_detail = Some(failure.detail.clone());
                    attempt.finished_at = Some(self.clock.now());
                    self.write_record(&attempt_path, &attempt)?;
                    if !failure.retryable {
                        return Ok(StepOutcome::Failed {
                            class: failure.class,
                            detail: failure.detail,
                        });
                    }
                    (failure.class, failure.detail)
                }
            };

            if attempt_number >= max_attempts {
                let (class, detail) = retryable_failure;
                return Ok(StepOutcome::Failed {
                    class: ErrorClass::RetriesExhausted,
                    detail: format!(
                        "{max_attempts} attempt(s) exhausted; last failure ({class:?}): {detail}"
                    ),
                });
            }
            // Bounded backoff on the injected clock. Never `Instant::now`.
            if let Some(policy) = &step.retry_policy {
                let factor = policy
                    .backoff_multiplier
                    .max(1.0)
                    .powi(i32::try_from(attempt_number.saturating_sub(1)).unwrap_or(i32::MAX));
                let millis = (policy.base_delay_ms as f64 * factor).min(u64::MAX as f64);
                let delay = Duration::from_millis(millis as u64).min(self.policy.max_retry_delay);
                if !delay.is_zero() {
                    self.clock.sleep(delay);
                }
            }
        }
    }

    fn load_reusable_commit(
        &self,
        commit_key: &str,
        run: &WorkflowRunRecord,
        step: &WorkflowStep,
        reuse_key_hash: &str,
    ) -> Result<Option<ArtifactCommit>> {
        let Some(commit) = self.load_commit(commit_key)? else {
            return Ok(None);
        };
        if commit.owner_id != run.owner_id
            || commit.session_id != run.session_id
            || commit.project_id != run.project_id
            || commit.workflow_id != run.workflow_id
            || commit.step_id != step.step_id
            || commit.cache_policy != CachePolicy::DeterministicReuse
            || commit.reuse_key_hash != reuse_key_hash
        {
            return Err(ScienceError::Ownership);
        }
        Ok(Some(commit))
    }

    fn validate_operation_record(
        &self,
        operation: &WorkflowOperationRecord,
        expected_operation_id: &str,
    ) -> Result<()> {
        crate::project::mutation::validate_operation_id(expected_operation_id)?;
        if operation.schema_version != WORKFLOW_RUN_SCHEMA_VERSION
            || operation.operation_id != expected_operation_id
            || operation.session_id.is_empty()
            || operation.owner_id.is_empty()
            || operation.workflow_id.is_empty()
        {
            return Err(ScienceError::Ownership);
        }
        validate_run_id(&operation.run_id)?;
        Ok(())
    }

    fn validate_run_record(&self, run: &WorkflowRunRecord, expected_run_id: &str) -> Result<()> {
        validate_run_id(expected_run_id)?;
        if run.schema_version != WORKFLOW_RUN_SCHEMA_VERSION
            || run.run_id != expected_run_id
            || run.session_id.is_empty()
            || run.owner_id.is_empty()
            || run.workflow_id.is_empty()
            || run.project_id.0.is_empty()
            || run.state_history.is_empty()
            || run.state_history.last().map(|transition| transition.state) != Some(run.state)
            || (run.state.terminal() != run.finished_at.is_some())
        {
            return Err(ScienceError::Ownership);
        }
        crate::project::mutation::validate_operation_id(&run.operation_id)?;
        for digest in [&run.spec_hash, &run.environment_hash, &run.policy_hash] {
            validate_commit_key(digest)?;
        }
        for step_id in &run.step_order {
            validate_step_id(step_id).map_err(ScienceError::Invalid)?;
        }
        Ok(())
    }

    fn validate_attempt_record(
        &self,
        attempt: &StepAttempt,
        expected_run_id: &str,
        expected_attempt_id: &str,
    ) -> Result<()> {
        validate_run_id(expected_run_id)?;
        validate_workflow_record_stem(expected_attempt_id, "workflow attempt id")?;
        validate_step_id(&attempt.step_id).map_err(ScienceError::Invalid)?;
        if attempt.schema_version != WORKFLOW_RUN_SCHEMA_VERSION
            || attempt.run_id != expected_run_id
            || attempt.attempt_id != expected_attempt_id
            || attempt.workflow_id.is_empty()
            || attempt.attempt_number == 0
            || attempt.attempt_id
                != attempt_id(&attempt.run_id, &attempt.step_id, attempt.attempt_number)
            || (attempt.terminal_state.is_some() != attempt.finished_at.is_some())
        {
            return Err(ScienceError::Ownership);
        }
        for digest in [
            &attempt.input_manifest_hash,
            &attempt.parameter_hash,
            &attempt.implementation_hash,
            &attempt.environment_hash,
            &attempt.policy_hash,
            &attempt.reuse_key_hash,
        ] {
            validate_commit_key(digest)?;
        }
        if let Some(digest) = &attempt.output_manifest_hash {
            validate_commit_key(digest)?;
        }
        if let Some(commit_key) = &attempt.commit_key {
            validate_commit_key(commit_key)?;
        }
        let committed_terminal = matches!(
            attempt.terminal_state,
            Some(AttemptState::Succeeded | AttemptState::Reused)
        );
        if committed_terminal
            != (attempt.commit_key.is_some() && attempt.output_manifest_hash.is_some())
        {
            return Err(ScienceError::Ownership);
        }
        Ok(())
    }

    fn validate_commit_record(&self, commit: &ArtifactCommit, commit_key: &str) -> Result<()> {
        if commit.schema_version != WORKFLOW_RUN_SCHEMA_VERSION
            || commit.commit_key != commit_key
            || commit.owner_id.is_empty()
            || commit.session_id.is_empty()
            || commit.project_id.0.is_empty()
            || commit.workflow_id.is_empty()
            || commit.reuse_key_hash.len() != 64
            || commit.cache_policy == CachePolicy::LocalReuse
        {
            return Err(ScienceError::Ownership);
        }
        validate_run_id(&commit.run_id)?;
        validate_step_id(&commit.step_id).map_err(ScienceError::Invalid)?;
        validate_commit_key(&commit.reuse_key_hash)?;
        let expected_key = commit_key_for(
            &commit.owner_id,
            &commit.session_id,
            &commit.project_id,
            &commit.workflow_id,
            &commit.step_id,
            commit.cache_policy,
            &commit.reuse_key_hash,
            &commit.run_id,
        );
        if expected_key != commit.commit_key
            || hash_map(&commit.output_manifest) != commit.output_manifest_hash
        {
            return Err(ScienceError::Ownership);
        }
        for digest in commit.output_manifest.values() {
            validate_commit_key(digest)?;
        }
        Ok(())
    }

    /// Finish a two-phase commit while the store write guard is held.
    ///
    /// `Prepared` is written only after acceptance and CAS publication. If the
    /// process stops before the origin attempt and final state are written, a
    /// later reader can therefore deterministically finish the same accepted
    /// transaction instead of poisoning this immutable commit key forever.
    fn resolve_commit_locked(
        &self,
        mut commit: ArtifactCommit,
        commit_key: &str,
    ) -> Result<ArtifactCommit> {
        self.validate_commit_record(&commit, commit_key)?;
        self.verify_commit_artifacts(&commit)?;
        if commit.state == ArtifactCommitState::Committed {
            self.verify_commit_origin(&commit)?;
            return Ok(commit);
        }

        let attempt_path = self.attempt_path(&commit.run_id, &commit.committed_by_attempt);
        let mut attempt: StepAttempt =
            self.require_record(&attempt_path, "prepared workflow commit origin attempt")?;
        self.validate_attempt_record(&attempt, &commit.run_id, &commit.committed_by_attempt)?;
        match attempt.terminal_state {
            None if attempt.commit_key.is_none() && attempt.output_manifest_hash.is_none() => {
                attempt.commit_key = Some(commit.commit_key.clone());
                attempt.output_manifest_hash = Some(commit.output_manifest_hash.clone());
                attempt.terminal_state = Some(AttemptState::Succeeded);
                attempt.finished_at = Some(commit.committed_at);
                self.confined()?
                    .replace_atomic(&attempt_path, &serde_json::to_vec_pretty(&attempt)?)?;
            }
            Some(AttemptState::Succeeded)
                if attempt.commit_key.as_deref() == Some(commit.commit_key.as_str())
                    && attempt.output_manifest_hash.as_deref()
                        == Some(commit.output_manifest_hash.as_str()) => {}
            _ => return Err(ScienceError::Ownership),
        }

        commit.state = ArtifactCommitState::Committed;
        self.confined()?.replace_atomic(
            &self.commit_path(commit_key),
            &serde_json::to_vec_pretty(&commit)?,
        )?;
        self.verify_commit_origin(&commit)?;
        Ok(commit)
    }

    fn verify_commit_origin(&self, commit: &ArtifactCommit) -> Result<()> {
        if commit.state != ArtifactCommitState::Committed {
            return Err(ScienceError::Ownership);
        }
        let attempt_path = self.attempt_path(&commit.run_id, &commit.committed_by_attempt);
        let attempt: StepAttempt =
            self.require_record(&attempt_path, "workflow commit origin attempt")?;
        self.validate_attempt_record(&attempt, &commit.run_id, &commit.committed_by_attempt)?;
        if attempt.workflow_id != commit.workflow_id
            || attempt.step_id != commit.step_id
            || attempt.reuse_key_hash != commit.reuse_key_hash
            || attempt.commit_key.as_deref() != Some(commit.commit_key.as_str())
            || attempt.output_manifest_hash.as_deref() != Some(commit.output_manifest_hash.as_str())
            || attempt.terminal_state != Some(AttemptState::Succeeded)
        {
            return Err(ScienceError::Ownership);
        }
        let run = self.load_run(&commit.run_id)?;
        if run.owner_id != commit.owner_id
            || run.session_id != commit.session_id
            || run.project_id != commit.project_id
            || run.workflow_id != commit.workflow_id
            || attempt.environment_hash != run.environment_hash
            || attempt.policy_hash != run.policy_hash
        {
            return Err(ScienceError::Ownership);
        }
        Ok(())
    }

    fn verify_commit_artifacts(&self, commit: &ArtifactCommit) -> Result<()> {
        for (name, digest) in &commit.output_manifest {
            let bytes = self
                .confined()?
                .read_optional(&self.artifact_path(digest))?
                .ok_or_else(|| {
                    ScienceError::Invalid(format!(
                        "workflow artifact '{name}' ({digest}) is missing from the immutable store"
                    ))
                })?;
            let actual = hex_sha256(&bytes);
            if actual != *digest {
                return Err(ScienceError::Invalid(format!(
                    "workflow artifact '{name}' expected {digest}, but stored bytes hash to {actual}"
                )));
            }
        }
        Ok(())
    }

    fn publish_artifact(&self, digest: &str, bytes: &[u8]) -> Result<()> {
        validate_commit_key(digest)?;
        let path = self.artifact_path(digest);
        match self.confined()?.write_new_atomic(&path, bytes) {
            Ok(()) => Ok(()),
            Err(ScienceError::Io(error)) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let existing = self.confined()?.read_optional(&path)?.ok_or_else(|| {
                    ScienceError::Invalid(format!(
                        "workflow artifact {digest} disappeared during immutable publication"
                    ))
                })?;
                let actual = hex_sha256(&existing);
                if actual == digest {
                    Ok(())
                } else {
                    Err(ScienceError::Invalid(format!(
                        "workflow artifact {digest} already contains bytes hashing to {actual}"
                    )))
                }
            }
            Err(error) => Err(error),
        }
    }

    /// Publish accepted output into the immutable CAS and commit it exactly
    /// once under the authority-scoped key.
    fn commit_output(
        &self,
        commit_key: &str,
        run: &WorkflowRunRecord,
        step: &WorkflowStep,
        reuse_key_hash: &str,
        attempt_id: &str,
        artifacts: &BTreeMap<String, Vec<u8>>,
    ) -> Result<ArtifactCommit> {
        validate_commit_key(commit_key)?;
        validate_step_id(&step.step_id).map_err(ScienceError::Invalid)?;
        validate_workflow_record_stem(attempt_id, "workflow attempt id")?;
        let _guard = self.guard()?;
        let path = self.commit_path(commit_key);
        let manifest: BTreeMap<String, String> = artifacts
            .iter()
            .map(|(name, bytes)| (name.clone(), hex_sha256(bytes)))
            .collect();
        if let Some(existing) = self.read_record::<ArtifactCommit>(&path)? {
            let existing = self.resolve_commit_locked(existing, commit_key)?;
            if existing.owner_id != run.owner_id
                || existing.session_id != run.session_id
                || existing.project_id != run.project_id
                || existing.workflow_id != run.workflow_id
                || existing.step_id != step.step_id
                || existing.cache_policy != step.cache_policy
                || existing.reuse_key_hash != reuse_key_hash
                || (step.cache_policy == CachePolicy::NoCache && existing.run_id != run.run_id)
                || existing.output_manifest != manifest
            {
                return Err(ScienceError::Ownership);
            }
            return Ok(existing);
        }
        for (name, bytes) in artifacts {
            if name.is_empty() || name.as_bytes().contains(&0) {
                return Err(ScienceError::Invalid(
                    "workflow artifact names must be non-empty and contain no NUL bytes".into(),
                ));
            }
            self.publish_artifact(&hex_sha256(bytes), bytes)?;
        }
        let commit = ArtifactCommit {
            schema_version: WORKFLOW_RUN_SCHEMA_VERSION,
            state: ArtifactCommitState::Prepared,
            commit_key: commit_key.to_string(),
            owner_id: run.owner_id.clone(),
            session_id: run.session_id.clone(),
            project_id: run.project_id.clone(),
            run_id: run.run_id.clone(),
            workflow_id: run.workflow_id.clone(),
            step_id: step.step_id.clone(),
            cache_policy: step.cache_policy,
            reuse_key_hash: reuse_key_hash.to_string(),
            output_manifest_hash: hash_map(&manifest),
            output_manifest: manifest,
            committed_at: self.clock.now(),
            committed_by_attempt: attempt_id.to_string(),
        };
        let bytes = serde_json::to_vec_pretty(&commit)?;
        // `create_new` again, so two executors racing on one key cannot both
        // believe they committed.
        match self.confined()?.write_new_atomic(&path, &bytes) {
            Ok(()) => self.resolve_commit_locked(commit, commit_key),
            Err(ScienceError::Io(error)) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let existing: ArtifactCommit =
                    self.require_record(&path, "winning workflow commit")?;
                let existing = self.resolve_commit_locked(existing, commit_key)?;
                if existing.owner_id != run.owner_id
                    || existing.session_id != run.session_id
                    || existing.project_id != run.project_id
                    || existing.workflow_id != run.workflow_id
                    || existing.step_id != step.step_id
                    || existing.cache_policy != step.cache_policy
                    || existing.reuse_key_hash != reuse_key_hash
                    || (step.cache_policy == CachePolicy::NoCache && existing.run_id != run.run_id)
                    || existing.output_manifest != commit.output_manifest
                {
                    return Err(ScienceError::Ownership);
                }
                Ok(existing)
            }
            Err(error) => Err(error),
        }
    }

    // ── State bookkeeping ─────────────────────────────────────────

    fn persist(
        &self,
        run: &mut WorkflowRunRecord,
        state: WorkflowState,
        note: Option<String>,
    ) -> Result<()> {
        let at = self.clock.now();
        if run.state != state || run.state_history.is_empty() {
            run.state = state;
            run.state_history.push(WorkflowStateTransition {
                state,
                at,
                note: note.clone(),
            });
        }
        if state.terminal() {
            run.finished_at = Some(at);
        }
        let path = self.run_path(&run.run_id);
        self.write_record(&path, run)
    }

    fn finish(
        &self,
        run: &mut WorkflowRunRecord,
        state: WorkflowState,
        failure: Option<String>,
    ) -> Result<WorkflowRunReport> {
        run.failure = failure.clone();
        self.persist(run, state, failure)?;
        self.build_report(run)
    }

    fn build_report(&self, run: &WorkflowRunRecord) -> Result<WorkflowRunReport> {
        self.validate_run_record(run, &run.run_id)?;
        let attempts = self.load_attempts(&run.run_id)?;
        if attempts.iter().any(|attempt| {
            attempt.workflow_id != run.workflow_id
                || attempt.environment_hash != run.environment_hash
                || attempt.policy_hash != run.policy_hash
        }) {
            return Err(ScienceError::Ownership);
        }
        let steps_reused = attempts
            .iter()
            .filter(|a| a.terminal_state == Some(AttemptState::Reused))
            .count();
        let commit_keys: BTreeSet<String> = attempts
            .iter()
            .filter_map(|a| a.commit_key.as_ref())
            .cloned()
            .collect();
        let mut commits = Vec::with_capacity(commit_keys.len());
        for commit_key in commit_keys {
            let commit = self.load_commit(&commit_key)?.ok_or_else(|| {
                ScienceError::Invalid(format!(
                    "workflow attempt references missing commit {commit_key}"
                ))
            })?;
            if commit.owner_id != run.owner_id
                || commit.session_id != run.session_id
                || commit.project_id != run.project_id
                || commit.workflow_id != run.workflow_id
                || !attempts.iter().any(|attempt| {
                    attempt.commit_key.as_deref() == Some(commit.commit_key.as_str())
                        && attempt.step_id == commit.step_id
                        && attempt.output_manifest_hash.as_deref()
                            == Some(commit.output_manifest_hash.as_str())
                })
            {
                return Err(ScienceError::Ownership);
            }
            commits.push(commit);
        }
        let artifacts_committed = commits
            .iter()
            .filter(|commit| {
                attempts
                    .iter()
                    .any(|a| a.attempt_id == commit.committed_by_attempt)
            })
            .count();
        Ok(WorkflowRunReport {
            run: run.clone(),
            attempts,
            commits,
            artifacts_committed,
            steps_reused,
            replayed: false,
            recovered: false,
        })
    }

    /// Remove only this store's capability-relative durable-write temp files.
    fn sweep_temp_files(&self, dir: &Path) -> Result<usize> {
        let mut removed = 0;
        for name in self.confined()?.list_names(dir)? {
            let Some(name) = name.to_str() else { continue };
            if name.starts_with(".project-")
                && name.ends_with(".tmp")
                && self.confined()?.remove_file(&dir.join(name))?
            {
                removed += 1;
            }
        }
        Ok(removed)
    }
}

enum OperationReservation {
    Fresh,
    Replay(WorkflowOperationRecord),
}

enum OperationOutcome {
    Fresh(Box<WorkflowRunRecord>),
    Replay(Box<WorkflowOperationRecord>, Box<WorkflowRunRecord>),
}

/// Process-local marker that a run is being executed *right now*.
///
/// This is what separates "another thread owns this run" from "the process
/// that owned this run is gone". Without it, a concurrent second call on one
/// operation id would see a non-terminal record, conclude a crash, and mark a
/// live run `Interrupted` out from under the thread executing it.
///
/// Deliberately process-local: a cross-process lease needs a lock file with a
/// liveness check, which this engine does not have. Two processes executing
/// the same operation id against one store is out of scope.
struct RunLease(String);

impl RunLease {
    fn registry() -> &'static Mutex<std::collections::HashSet<String>> {
        static LEASES: std::sync::OnceLock<Mutex<std::collections::HashSet<String>>> =
            std::sync::OnceLock::new();
        LEASES.get_or_init(|| Mutex::new(std::collections::HashSet::new()))
    }

    fn lock() -> std::sync::MutexGuard<'static, std::collections::HashSet<String>> {
        Self::registry()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn acquire(run_id: &str) -> Self {
        Self::lock().insert(run_id.to_string());
        Self(run_id.to_string())
    }

    fn is_held(run_id: &str) -> bool {
        Self::lock().contains(run_id)
    }
}

impl Drop for RunLease {
    fn drop(&mut self) {
        Self::lock().remove(&self.0);
    }
}

enum StepOutcome {
    Succeeded { output_manifest_hash: String },
    Failed { class: ErrorClass, detail: String },
    Cancelled,
}

enum AcceptanceVerdict {
    Accepted,
    Rejected { detail: String, on_fail: FailAction },
}

/// Evaluate a step's acceptance rules against what it produced.
///
/// `CustomRule` fails closed: this engine has no rule interpreter, and
/// reporting a pass for a rule nobody evaluated is the same defect class as
/// admitting a kernel nobody probed.
fn evaluate_acceptance(step: &WorkflowStep, output: &StepOutput) -> AcceptanceVerdict {
    for rule in &step.acceptance_rules {
        let failure = match &rule.condition {
            AcceptanceCondition::ExitCodeZero => match output.exit_code {
                Some(0) => None,
                Some(code) => Some(format!("exit code was {code}, not 0")),
                None => Some("step reported no exit code to check against zero".into()),
            },
            AcceptanceCondition::OutputNotEmpty => output
                .artifacts
                .is_empty()
                .then(|| "step produced no artifacts".to_string()),
            AcceptanceCondition::ArtifactCount(expected) => {
                let actual = output.artifacts.len();
                (actual != *expected)
                    .then(|| format!("expected {expected} artifact(s), got {actual}"))
            }
            AcceptanceCondition::CustomRule(name) => Some(format!(
                "custom acceptance rule '{name}' cannot be evaluated by this engine"
            )),
        };
        if let Some(detail) = failure {
            return AcceptanceVerdict::Rejected {
                detail,
                on_fail: rule.on_fail.clone(),
            };
        }
    }
    AcceptanceVerdict::Accepted
}

/// Recompute every runner-supplied size claim and constrain artifact names
/// before acceptance or CAS publication. A `StepRunner` is an execution seam,
/// not a storage authority.
fn validate_step_output(
    spec: &WorkflowSpec,
    output: &StepOutput,
) -> std::result::Result<(), String> {
    let mut observed_bytes = 0u64;
    for (name, bytes) in &output.artifacts {
        let safe_name = !name.is_empty()
            && name.len() <= 4096
            && !name.starts_with('/')
            && !name.contains(['\0', '\\'])
            && name
                .split('/')
                .all(|component| !component.is_empty() && component != "." && component != "..");
        if !safe_name {
            return Err(format!(
                "runner returned unsafe workflow artifact name '{name}'"
            ));
        }
        observed_bytes = observed_bytes
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| "runner artifact byte count overflowed".to_string())?;
    }
    if observed_bytes != output.bytes_produced {
        return Err(format!(
            "runner claimed {} output bytes but supplied {observed_bytes}",
            output.bytes_produced
        ));
    }
    let max_bytes = spec
        .resources
        .max_disk_mb
        .checked_mul(1024 * 1024)
        .ok_or_else(|| "workflow max_disk_mb overflows bytes".to_string())?;
    if observed_bytes > max_bytes {
        return Err(format!(
            "runner supplied {observed_bytes} output bytes, exceeding the workflow {max_bytes} byte disk limit"
        ));
    }
    Ok(())
}

// ── Hashing helpers ───────────────────────────────────────────────

fn hex_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Length-prefixed so `{"ab": "c"}` and `{"a": "bc"}` cannot collide.
fn hash_map(map: &BTreeMap<String, String>) -> String {
    let mut hasher = Sha256::new();
    for (key, value) in map {
        hasher.update(key.len().to_le_bytes());
        hasher.update(key.as_bytes());
        hasher.update(value.len().to_le_bytes());
        hasher.update(value.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

fn execution_contract_hash(
    spec: &WorkflowSpec,
    step: &WorkflowStep,
    operation: &StepOperation,
) -> Result<String> {
    // The reuse identity must cover every contract that can change whether
    // these bytes are safe to serve. In particular, acceptance rules and
    // resource/permission ceilings cannot be omitted: doing so would let a
    // commit accepted under an older, weaker contract bypass the new one.
    let mut operation_contract = operation.clone();
    if let StepOperation::KernelCell { kernel, .. } = &mut operation_contract {
        // Probe time and the descriptive actor identity are provenance, not
        // execution semantics. Including them makes every fresh admission a
        // different cache key and silently disables deterministic reuse.
        kernel.admitted_at = None;
        kernel.admitted_by = None;
    }
    let bytes = serde_json::to_vec(&(
        "workflow-execution-contract-v1",
        step,
        operation_contract,
        &spec.resources,
        &spec.permissions,
    ))?;
    Ok(hex_sha256(&bytes))
}

fn spec_hash(spec: &WorkflowSpec) -> Result<String> {
    Ok(hex_sha256(&serde_json::to_vec(spec)?))
}

fn commit_key_for(
    owner_id: &str,
    session_id: &str,
    project_id: &ProjectId,
    workflow_id: &str,
    step_id: &str,
    cache_policy: CachePolicy,
    reuse_key_hash: &str,
    run_id: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(owner_id.len().to_le_bytes());
    hasher.update(owner_id.as_bytes());
    hasher.update(session_id.len().to_le_bytes());
    hasher.update(session_id.as_bytes());
    hasher.update(project_id.0.len().to_le_bytes());
    hasher.update(project_id.0.as_bytes());
    hasher.update(workflow_id.as_bytes());
    hasher.update([0]);
    hasher.update(step_id.as_bytes());
    hasher.update([0]);
    hasher.update(format!("{cache_policy:?}").as_bytes());
    hasher.update([0]);
    hasher.update(reuse_key_hash.as_bytes());
    if cache_policy == CachePolicy::NoCache {
        hasher.update([0]);
        hasher.update(run_id.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

/// Deterministic run id, so the workflow ledger and the actor-owned Science
/// run share one durable identity and an operation retry cannot scatter
/// half-runs across two stores.
pub fn run_id_for_operation(operation_id: &str) -> Result<String> {
    crate::project::mutation::validate_operation_id(operation_id)?;
    Ok(derive_run_id(operation_id))
}

fn derive_run_id(operation_id: &str) -> String {
    hex_sha256(operation_id.as_bytes())[..32].to_string()
}

fn attempt_id(run_id: &str, step_id: &str, attempt_number: u32) -> String {
    format!("{run_id}-{step_id}-{attempt_number:04}")
}

#[cfg(all(test, unix))]
mod confinement_tests {
    use super::*;
    use crate::workflow::{NetworkPolicy, ResourceLimits};
    use std::{collections::BTreeMap, fs, os::unix::fs::symlink};
    use tempfile::tempdir;

    fn environment() -> ComputeEnvironment {
        ComputeEnvironment {
            environment_id: "confinement-test".into(),
            os: "test".into(),
            architecture: "test".into(),
            lumen_binary_hash: "test".into(),
            rust_lock_hash: None,
            python_hash: None,
            r_hash: None,
            julia_hash: None,
            dependency_lock_hash: "test".into(),
            locale: "C".into(),
            timezone: "UTC".into(),
            environment_allowlist: Vec::new(),
            cpu_identity: None,
            gpu_identity: None,
            deterministic_flags: Vec::new(),
            network_policy: NetworkPolicy::None,
            container_digest: None,
        }
    }

    fn request(operation_id: &str) -> WorkflowExecutionRequest {
        WorkflowExecutionRequest {
            operation_id: operation_id.into(),
            session_id: "session-1".into(),
            owner_id: "owner-1".into(),
            spec: WorkflowSpec {
                workflow_id: "workflow-1".into(),
                project_id: ProjectId("project-1".into()),
                name: "confinement".into(),
                steps: Vec::new(),
                parameters: BTreeMap::new(),
                permissions: Vec::new(),
                resources: ResourceLimits {
                    max_concurrent_steps: 1,
                    max_total_duration_secs: 1,
                    max_memory_mb: 1,
                    max_disk_mb: 1,
                },
                schema_version: 1,
            },
        }
    }

    #[test]
    fn held_project_root_guard_executes_without_relocking_and_serializes_project_mutation() {
        use std::sync::mpsc;

        let workspace = tempdir().unwrap();
        let root = workspace.path().join("store");
        let executor = WorkflowExecutor::new(&root, environment());
        let project_store = ProjectStore::new(&root);
        let project = project_store
            .create_project(
                "owner-project-guard",
                "Before guarded workflow",
                "Can a project mutate during guarded workflow authority?",
            )
            .unwrap();
        let initial_revision = project_store.project_revision(&project.project_id).unwrap();

        let guarded_request = WorkflowExecutionRequest {
            operation_id: "operation-project-guard".into(),
            session_id: "session-project-guard".into(),
            owner_id: "owner-project-guard".into(),
            spec: WorkflowSpec {
                workflow_id: "workflow-project-guard".into(),
                project_id: project.project_id.clone(),
                name: "guarded workflow".into(),
                steps: Vec::new(),
                parameters: BTreeMap::new(),
                permissions: Vec::new(),
                resources: ResourceLimits {
                    max_concurrent_steps: 1,
                    max_total_duration_secs: 1,
                    max_memory_mb: 1,
                    max_disk_mb: 1,
                },
                schema_version: 1,
            },
        };

        let (start_tx, start_rx) = mpsc::channel();
        let (attempting_tx, attempting_rx) = mpsc::channel();
        let (finished_tx, finished_rx) = mpsc::channel();
        let mutation_store = project_store.clone();
        let mut changed_project = project.clone();
        changed_project.title = "After guarded workflow".into();
        let mutation = std::thread::spawn(move || {
            start_rx.recv().unwrap();
            attempting_tx.send(()).unwrap();
            mutation_store.save_project(&changed_project).unwrap();
            finished_tx.send(()).unwrap();
        });

        let report = project_store
            .with_owned_project_revision_guarded(
                &project.project_id,
                "owner-project-guard",
                |_owned, revision, held| {
                    assert_eq!(revision, initial_revision);
                    start_tx.send(()).unwrap();
                    attempting_rx.recv().unwrap();
                    assert!(
                        finished_rx
                            .recv_timeout(std::time::Duration::from_millis(100))
                            .is_err(),
                        "project mutation completed while the root guard was held"
                    );
                    executor.with_project_guard(held, |executor| {
                        let report = executor.execute(&guarded_request)?;
                        assert!(
                            executor
                                .lookup_operation(&guarded_request.operation_id)?
                                .is_some(),
                            "post-execution ledger read failed inside the same held-root scope"
                        );
                        Ok(report)
                    })
                },
            )
            .unwrap();
        assert_eq!(report.run.state, WorkflowState::Succeeded);
        finished_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("project mutation did not resume after the guarded workflow");
        mutation.join().unwrap();

        assert_ne!(
            project_store.project_revision(&project.project_id).unwrap(),
            initial_revision
        );
        assert_eq!(
            project_store
                .load_project(&project.project_id)
                .unwrap()
                .title,
            "After guarded workflow"
        );
    }

    #[test]
    fn held_project_root_guard_is_rejected_by_an_executor_on_another_root() {
        let workspace = tempdir().unwrap();
        let guarded_root = workspace.path().join("guarded-store");
        let other_root = workspace.path().join("other-store");
        let project_store = ProjectStore::new(&guarded_root);
        let project = project_store
            .create_project("owner-root-binding", "Root binding", "Which root is held?")
            .unwrap();
        let other_executor = WorkflowExecutor::new(&other_root, environment());
        let request = WorkflowExecutionRequest {
            operation_id: "operation-wrong-root-guard".into(),
            session_id: "session-root-binding".into(),
            owner_id: "owner-root-binding".into(),
            spec: WorkflowSpec {
                workflow_id: "workflow-root-binding".into(),
                project_id: project.project_id.clone(),
                name: "wrong root guard".into(),
                steps: Vec::new(),
                parameters: BTreeMap::new(),
                permissions: Vec::new(),
                resources: ResourceLimits {
                    max_concurrent_steps: 1,
                    max_total_duration_secs: 1,
                    max_memory_mb: 1,
                    max_disk_mb: 1,
                },
                schema_version: 1,
            },
        };

        project_store
            .with_owned_project_revision_guarded(
                &project.project_id,
                "owner-root-binding",
                |_owned, _revision, held| {
                    assert!(matches!(
                        other_executor.execute_with_project_guard(held, &request),
                        Err(ScienceError::Ownership)
                    ));
                    Ok(())
                },
            )
            .unwrap();
        assert!(
            !other_root.join("workflow-operations").exists(),
            "wrong-root guard created workflow execution state"
        );
    }

    #[test]
    fn retained_workflow_root_ignores_renamed_path_and_outside_symlink() {
        let workspace = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let root = workspace.path().join("store");
        let retained = workspace.path().join("retained");
        let executor = WorkflowExecutor::new(&root, environment());
        let run_id = "a".repeat(32);

        fs::rename(&root, &retained).unwrap();
        symlink(outside.path(), &root).unwrap();
        executor
            .write_record(
                &executor.run_path(&run_id),
                &serde_json::json!({"run_id": run_id}),
            )
            .unwrap();

        assert!(
            outside.path().read_dir().unwrap().next().is_none(),
            "root pathname replacement redirected workflow bytes outside"
        );
        assert!(
            retained
                .join("workflow-runs")
                .join("a".repeat(32))
                .join("run.json")
                .is_file(),
            "retained workflow capability did not receive the record"
        );
    }

    #[test]
    fn workflow_parent_symlinks_fail_without_outside_bytes_or_sweep() {
        let workspace = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let root = workspace.path().join("store");
        fs::create_dir_all(&root).unwrap();
        for name in ["workflow-runs", "workflow-operations", "workflow-commits"] {
            let target = outside.path().join(name);
            fs::create_dir_all(&target).unwrap();
            symlink(&target, root.join(name)).unwrap();
        }
        let sentinel = outside
            .path()
            .join("workflow-runs")
            .join(".project-sentinel.tmp");
        fs::write(&sentinel, b"outside").unwrap();

        let executor = WorkflowExecutor::new(&root, environment());
        let run_id = "b".repeat(32);
        assert!(
            executor
                .write_record(
                    &executor.run_path(&run_id),
                    &serde_json::json!({"run_id": run_id})
                )
                .is_err()
        );
        assert!(
            executor
                .write_record(
                    &executor.attempt_path(&run_id, &format!("{run_id}-step-0001")),
                    &serde_json::json!({"attempt": 1})
                )
                .is_err()
        );
        assert!(
            executor
                .reserve_operation(&request("operation-1"), &run_id)
                .is_err()
        );
        assert!({
            let authority_request = request("operation-2");
            let run = executor.new_draft(&authority_request, &run_id).unwrap();
            let step = WorkflowStep {
                step_id: "step".into(),
                kind: StepKind::Renderer,
                connector_id: None,
                notebook_cell: None,
                inputs: Vec::new(),
                parameters: BTreeMap::new(),
                timeout_secs: 1,
                retry_policy: None,
                cache_policy: CachePolicy::NoCache,
                acceptance_rules: Vec::new(),
            };
            let reuse_key_hash = "d".repeat(64);
            let key = commit_key_for(
                &run.owner_id,
                &run.session_id,
                &run.project_id,
                &run.workflow_id,
                &step.step_id,
                step.cache_policy,
                &reuse_key_hash,
                &run.run_id,
            );
            executor
                .commit_output(
                    &key,
                    &run,
                    &step,
                    &reuse_key_hash,
                    &format!("{run_id}-step-0001"),
                    &BTreeMap::new(),
                )
                .is_err()
        });
        assert!(
            executor
                .sweep_temp_files(&executor.run_dir(&run_id))
                .is_err()
        );
        assert_eq!(fs::read(&sentinel).unwrap(), b"outside");
        assert_eq!(
            fs::read_dir(outside.path().join("workflow-operations"))
                .unwrap()
                .count(),
            0
        );
        assert_eq!(
            fs::read_dir(outside.path().join("workflow-commits"))
                .unwrap()
                .count(),
            0
        );
    }

    #[test]
    fn public_workflow_record_lookups_reject_path_stems() {
        let workspace = tempdir().unwrap();
        let executor = WorkflowExecutor::new(workspace.path().join("store"), environment());

        for invalid in ["../escape", "/absolute", ".", "..", "é", "a/b"] {
            assert!(executor.load_run(invalid).is_err());
            assert!(executor.load_attempts(invalid).is_err());
            assert!(executor.recover_run(invalid).is_err());
            assert!(executor.lookup_operation(invalid).is_err());
            assert!(executor.load_commit(invalid).is_err());
        }
        assert!(
            workspace.path().join("escape").symlink_metadata().is_err(),
            "invalid workflow record ids caused an outside side effect"
        );
    }

    #[test]
    fn confined_executor_constructor_rejects_outside_and_symlink_roots() {
        let workspace = tempdir().unwrap();
        let outside = tempdir().unwrap();
        assert!(
            WorkflowExecutor::new_confined(outside.path(), workspace.path(), environment())
                .is_err()
        );

        let linked = workspace.path().join("linked-store");
        symlink(outside.path(), &linked).unwrap();
        assert!(WorkflowExecutor::new_confined(&linked, workspace.path(), environment()).is_err());
        assert!(outside.path().read_dir().unwrap().next().is_none());
    }
}

#[cfg(test)]
mod tests;
