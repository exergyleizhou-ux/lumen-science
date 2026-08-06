//! The `StepRunner` that actually executes a kernel cell.
//!
//! Until this existed the executor was complete but inert: `UnboundStepRunner`
//! refused every step, so no workflow had ever run end to end. This closes that
//! seam, and it is the only place in the crate that spawns a process.
//!
//! It launches one retained interpreter process per cell. Exit status belongs
//! to the Rust parent, while stdout and stderr are captured on separate pipes.
//! There is deliberately no in-process JSON control channel for cell code to
//! forge.
//!
//! # What this runner refuses to do
//!
//! The executor hands down a policy alongside the work. A runner that cannot
//! honour that policy must FAIL the step, never run it anyway with weaker
//! guarantees — a step that ran under conditions nobody authorised is worse
//! than a step that did not run, because the evidence chain records it as
//! having succeeded.
//!
//! So this runner refuses when: the plan is not kernel work; the cell source
//! does not hash to the digest the plan names; process isolation is demanded
//! and cannot be provided; or the kernel record was not admitted.
//!
//! Python audit hooks are not a security boundary for adversarial Python.
//! Artifact and network confinement therefore require an OS-level sandbox;
//! platforms without one must fail closed before user code is spawned.

use std::io::Read;
#[cfg(target_os = "linux")]
use std::io::Write;
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

use super::executor::{ErrorClass, StepFailure, StepOperation, StepOutput, StepPlan, StepRunner};
use super::kernel::{AdmissionStatus, KernelKind};
use super::{PinnedExecutable, WorkflowIoCapability};

/// What isolation a runner actually provides.
///
/// Declared rather than assumed. `KernelPolicy::require_process_isolation`
/// defaults to true, so with stock settings no kernel step runs at all — which
/// is the correct default and makes this the decision that unblocks it. Naming
/// the level forces that decision to be visible instead of arriving as a
/// silently weaker execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProvidedIsolation {
    /// The cell runs as its own process with an emptied environment, an output
    /// capability it may not write outside of, and mandatory OS policy
    /// refusing unapproved reads, network, signals, secondary exec and
    /// out-of-tree writes.
    ///
    /// This is what `process_isolation` on an admission record asks for: the
    /// work does not share a process with the engine. It is NOT a container —
    /// same kernel, same user, same filesystem namespace — so a cell that
    /// escapes CPython escapes this.
    SeparateProcess,
    /// A dedicated namespace or container. The current Seatbelt or
    /// Landlock/seccomp policy still shares the host kernel and user identity,
    /// so it deliberately does not claim this stronger tier.
    OsLevel,
}

/// Executes kernel cells by driving the Lumen Python exec-loop.
#[derive(Debug, Clone)]
pub struct PythonLoopRunner {
    io: WorkflowIoCapability,
    executable: Arc<PinnedExecutable>,
    provides: ProvidedIsolation,
}

impl PythonLoopRunner {
    pub fn new(io: WorkflowIoCapability, executable: Arc<PinnedExecutable>) -> Self {
        Self {
            io,
            executable,
            provides: ProvidedIsolation::SeparateProcess,
        }
    }

    /// Declare the isolation this runner provides.
    ///
    /// Only lowers or restates what the implementation does; it grants nothing.
    /// Setting `OsLevel` here would be a false claim, so nothing in-tree does.
    pub fn with_provided_isolation(mut self, provides: ProvidedIsolation) -> Self {
        self.provides = provides;
        self
    }
}

fn permanent(class: ErrorClass, detail: impl Into<String>) -> StepFailure {
    StepFailure::permanent(class, detail.into())
}

fn hex_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn validate_invocation_environment(
    environment: &std::collections::BTreeMap<String, String>,
) -> std::result::Result<(), StepFailure> {
    for key in environment.keys() {
        if key.starts_with("LD_")
            || key.starts_with("PYTHON")
            || matches!(
                key.as_str(),
                "GCONV_PATH" | "GLIBC_TUNABLES" | "LOCPATH" | "MALLOC_CHECK_" | "MALLOC_TRACE"
            )
        {
            return Err(permanent(
                ErrorClass::PolicyViolation,
                format!(
                    "kernel environment key '{key}' can alter the dynamic loader or Python runtime"
                ),
            ));
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
pub(crate) fn linux_post_load_exec_seal_bootstrap() -> std::io::Result<String> {
    let filter = super::pinned_executable::linux_deny_exec_filter()?;
    let instructions = filter
        .instructions
        .iter()
        .map(|instruction| {
            format!(
                "F({},{},{},{})",
                instruction.code, instruction.jt, instruction.jf, instruction.k
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    Ok(format!(
        r#"import ctypes as _c, os as _o, sys as _s
class F(_c.Structure):
 _fields_=[("code",_c.c_ushort),("jt",_c.c_ubyte),("jf",_c.c_ubyte),("k",_c.c_uint)]
class P(_c.Structure):
 _fields_=[("len",_c.c_ushort),("filter",_c.POINTER(F))]
_f=[{instructions}]
_a=(F*len(_f))(*_f);_p=P(len(_f),_a);_l=_c.CDLL(None,use_errno=True)
if _l.prctl(38,1,0,0,0)!=0 or _l.prctl(22,2,_c.byref(_p))!=0:
 _o.write(2,b"LUMEN_POST_LOAD_EXEC_SEAL_FAILED\n");_o._exit(190)
_source=_s.stdin.buffer.read()
exec(compile(_source,"<lumen-cell>","exec"),{{"__name__":"__main__","__builtins__":__builtins__}})
"#
    ))
}

fn drain_bounded<R: Read>(reader: Option<R>, retain_limit: u64) -> (Vec<u8>, u64) {
    let Some(mut reader) = reader else {
        return (Vec::new(), 0);
    };
    let retain_limit = usize::try_from(retain_limit).unwrap_or(usize::MAX);
    let mut retained = Vec::new();
    let mut total = 0u64;
    let mut buffer = [0u8; 16 * 1024];
    while let Ok(read) = reader.read(&mut buffer) {
        if read == 0 {
            break;
        }
        total = total.saturating_add(read as u64);
        if retained.len() < retain_limit {
            let remaining = retain_limit - retained.len();
            retained.extend_from_slice(&buffer[..read.min(remaining)]);
        }
    }
    (retained, total)
}

impl StepRunner for PythonLoopRunner {
    fn run(&self, plan: &StepPlan) -> std::result::Result<StepOutput, StepFailure> {
        let StepOperation::KernelCell { kernel, invocation } = &plan.operation else {
            // Say which kind was refused. A runner that silently returns empty
            // success for work it does not understand reports a step as done.
            return Err(permanent(
                ErrorClass::StepKindNotAllowed,
                format!(
                    "PythonLoopRunner executes kernel cells only; step '{}' is {:?}",
                    plan.step_id, plan.kind
                ),
            ));
        };

        if kernel.admission_status != AdmissionStatus::Admitted {
            return Err(permanent(
                ErrorClass::KernelNotAdmitted,
                format!(
                    "kernel '{}' is {:?}; a step may not run on an interpreter that was not admitted",
                    kernel.kernel_id, kernel.admission_status
                ),
            ));
        }
        if kernel.kind != KernelKind::Python {
            return Err(permanent(
                ErrorClass::KernelNotAdmitted,
                format!(
                    "PythonLoopRunner refuses {:?} kernel '{}'; its embedded protocol is Python",
                    kernel.kind, kernel.kernel_id
                ),
            ));
        }
        if kernel.executable_hash != self.executable.sha256()
            || invocation.executable_sha256 != self.executable.sha256()
        {
            return Err(permanent(
                ErrorClass::PolicyViolation,
                format!(
                    "kernel executable digest does not match the actor-retained capability \
                     (admission={}, invocation={}, retained={})",
                    kernel.executable_hash,
                    invocation.executable_sha256,
                    self.executable.sha256()
                ),
            ));
        }
        if Path::new(&invocation.interpreter_path) != self.executable.canonical_path() {
            return Err(permanent(
                ErrorClass::PolicyViolation,
                format!(
                    "kernel executable path evidence '{}' does not match retained source '{}'",
                    invocation.interpreter_path,
                    self.executable.canonical_path().display()
                ),
            ));
        }

        // `process_isolation` asks that the cell not share a process with the
        // engine, which spawning the loop satisfies. It is deliberately NOT
        // read as "namespace or container": that is the stronger `OsLevel`
        // tier, which nothing provides yet, and conflating the two would either
        // block every step forever or quietly overstate what was enforced.
        if invocation.process_isolation_required
            && !matches!(
                self.provides,
                ProvidedIsolation::SeparateProcess | ProvidedIsolation::OsLevel
            )
        {
            return Err(permanent(
                ErrorClass::PolicyViolation,
                format!(
                    "step requires process isolation; this runner provides {:?}. Refusing \
                     rather than executing under weaker isolation than the policy states.",
                    self.provides
                ),
            ));
        }

        // Resolve the cell body and re-hash it. The store is a lookup, so its
        // answer is checked: a corrupted or substituted source would otherwise
        // execute under a digest that says it is something else.
        let source = self
            .io
            .read_cell(&invocation.cell_source_sha256)
            .map_err(|error| permanent(ErrorClass::RunnerError, error.to_string()))?
            .ok_or_else(|| {
                permanent(
                    ErrorClass::RunnerError,
                    format!(
                        "cell source {} is not in the source store",
                        invocation.cell_source_sha256
                    ),
                )
            })?;
        let actual = hex_sha256(&source);
        if actual != invocation.cell_source_sha256 {
            return Err(permanent(
                ErrorClass::PolicyViolation,
                format!(
                    "cell source hashes to {actual}, plan names {}",
                    invocation.cell_source_sha256
                ),
            ));
        }
        let code = String::from_utf8(source).map_err(|_| {
            permanent(
                ErrorClass::PolicyViolation,
                "cell source is not valid UTF-8",
            )
        })?;
        validate_invocation_environment(&invocation.environment)?;

        let attempt = self
            .io
            .create_attempt_output(&plan.run_id, &plan.step_id, &plan.attempt_id)
            .map_err(|error| permanent(ErrorClass::RunnerError, error.to_string()))?;
        let child_paths = attempt
            .child_paths()
            .map_err(|error| permanent(ErrorClass::RunnerError, error.to_string()))?;
        let address_space_bytes = invocation
            .resource_cap
            .max_memory_mb
            .checked_mul(1024 * 1024)
            .filter(|value| *value > 0)
            .ok_or_else(|| {
                permanent(
                    ErrorClass::PolicyViolation,
                    "kernel max_memory_mb must be non-zero and fit in bytes",
                )
            })?;
        if invocation.resource_cap.max_cpu_seconds == 0
            || invocation.resource_cap.max_output_bytes == 0
            || invocation.resource_cap.max_file_descriptors == 0
        {
            return Err(permanent(
                ErrorClass::PolicyViolation,
                "kernel resource caps must all be greater than zero",
            ));
        }

        if invocation.network_allowed {
            return Err(permanent(
                ErrorClass::PolicyViolation,
                "network-enabled workflow kernels are not supported by this confined runner",
            ));
        }

        // Exactly the environment the invocation names, plus deterministic
        // interpreter settings. `env_clear` first: an empty map means an empty
        // environment, never an inherited one.
        #[cfg(target_os = "linux")]
        let mut pinned_command = self
            .executable
            .spawn_linux_sandboxed_command(child_paths.output_fd())
            .map_err(|error| {
                permanent(
                    ErrorClass::PolicyViolation,
                    format!("cannot install Linux workflow sandbox: {error}"),
                )
            })?;
        #[cfg(not(target_os = "linux"))]
        let mut pinned_command = self.executable.spawn_command().map_err(|error| {
            permanent(
                ErrorClass::RunnerError,
                format!("cannot prepare retained kernel executable: {error}"),
            )
        })?;
        #[cfg(target_os = "macos")]
        pinned_command
            .enable_os_sandbox(
                child_paths.sandbox_root(),
                invocation.resource_cap.max_memory_mb,
            )
            .map_err(|error| {
                permanent(
                    ErrorClass::PolicyViolation,
                    format!("cannot install macOS workflow sandbox: {error}"),
                )
            })?;
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        return Err(permanent(
            ErrorClass::PolicyViolation,
            "workflow kernel OS sandbox is unavailable on this platform",
        ));

        #[cfg(any(target_os = "macos", target_os = "linux"))]
        pinned_command
            .apply_resource_limits(
                address_space_bytes,
                invocation.resource_cap.max_cpu_seconds,
                invocation.resource_cap.max_output_bytes,
                invocation.resource_cap.max_file_descriptors,
            )
            .map_err(|error| {
                permanent(
                    ErrorClass::PolicyViolation,
                    format!("cannot install workflow resource ceilings: {error}"),
                )
            })?;
        let command = pinned_command.command_mut();
        #[cfg(target_os = "linux")]
        let exec_seal_bootstrap = linux_post_load_exec_seal_bootstrap().map_err(|error| {
            permanent(
                ErrorClass::PolicyViolation,
                format!("cannot construct Linux post-load exec seal: {error}"),
            )
        })?;
        #[cfg(target_os = "linux")]
        command
            // Isolated mode ignores every PYTHON* setting; -S prevents
            // sitecustomize/.pth execution before the kernel seal is installed.
            .args(["-I", "-S", "-u", "-c"])
            .arg(exec_seal_bootstrap)
            .args(&invocation.argv);
        #[cfg(not(target_os = "linux"))]
        command
            .arg("-c")
            .arg(&code)
            .args(&invocation.argv);
        command
            .env_clear()
            .envs(&invocation.environment)
            .env("LUMEN_KERNEL_OUTPUT_DIR", child_paths.output_path())
            .env("LUMEN_KERNEL_FIGURES_DIR", child_paths.figures_path())
            // Pinned so set iteration order is stable across replays of the
            // same run; the loop refuses to start without it.
            .env("PYTHONHASHSEED", "0")
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .env("LC_ALL", "C")
            .env("TZ", "UTC")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if invocation.working_dir.is_some() {
            return Err(permanent(
                ErrorClass::PolicyViolation,
                "kernel working_dir requires a retained directory capability; path-only cwd is refused",
            ));
        }
        child_paths.configure_command(command);

        let mut child = pinned_command.spawn().map_err(|error| {
            permanent(
                ErrorClass::RunnerError,
                format!(
                    "cannot start retained kernel '{}' (sha256:{}): {error}",
                    self.executable.canonical_path().display(),
                    self.executable.sha256()
                ),
            )
        })?;
        #[cfg(target_os = "linux")]
        {
            let mut stdin = child.stdin.take().ok_or_else(|| {
                permanent(
                    ErrorClass::RunnerError,
                    "confined Python bootstrap did not expose its source pipe",
                )
            })?;
            if let Err(error) = stdin.write_all(code.as_bytes()) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(permanent(
                    ErrorClass::RunnerError,
                    format!("cannot deliver cell source to confined Python bootstrap: {error}"),
                ));
            }
            drop(stdin);
        }
        #[cfg(not(target_os = "linux"))]
        drop(child.stdin.take());

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let capture_limit = invocation.resource_cap.max_output_bytes.saturating_add(1);
        let stdout_reader = std::thread::spawn(move || drain_bounded(stdout, capture_limit));
        let stderr_reader = std::thread::spawn(move || drain_bounded(stderr, capture_limit));

        let started = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Some(status),
                Ok(None) => {}
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(permanent(
                        ErrorClass::RunnerError,
                        format!("cannot wait on kernel: {error}"),
                    ));
                }
            }
            if started.elapsed() >= plan.timeout {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            std::thread::sleep(Duration::from_millis(5));
        };

        let (stdout, stdout_total) = stdout_reader.join().unwrap_or_default();
        let (stderr, stderr_total) = stderr_reader.join().unwrap_or_default();

        let Some(status) = status else {
            // Retryable: a timeout may be load, and the executor's bounded
            // retry decides whether to spend another attempt.
            return Err(StepFailure::transient(
                ErrorClass::Timeout,
                format!(
                    "kernel did not answer within {:?} for step '{}'",
                    plan.timeout, plan.step_id
                ),
            ));
        };

        if !status.success() {
            let diagnostic = String::from_utf8_lossy(&stderr);
            let (class, prefix) = if diagnostic.contains("Operation not permitted")
                || diagnostic.contains("PermissionError")
                || diagnostic.contains("[Errno 1]")
            {
                (
                    ErrorClass::PolicyViolation,
                    "OS sandbox denied kernel operation",
                )
            } else {
                (
                    ErrorClass::RunnerError,
                    "kernel process exited unsuccessfully before commit",
                )
            };
            return Err(permanent(
                class,
                format!(
                    "{prefix} ({status}) for step '{}': {}",
                    plan.step_id,
                    diagnostic.trim()
                ),
            ));
        }
        if stdout_total.saturating_add(stderr_total) > invocation.resource_cap.max_output_bytes {
            return Err(permanent(
                ErrorClass::PolicyViolation,
                format!(
                    "kernel console output exceeded the admitted {} byte output cap",
                    invocation.resource_cap.max_output_bytes
                ),
            ));
        }

        // stdout and stderr are outputs too: a run whose console output is lost
        // cannot be reviewed.
        for (name, body) in [("stdout.txt", &stdout), ("stderr.txt", &stderr)] {
            if body.is_empty() {
                continue;
            }
            attempt
                .write_atomic(Path::new(name), body)
                .map_err(|error| {
                    permanent(
                        ErrorClass::RunnerError,
                        format!("cannot write {name}: {error}"),
                    )
                })?;
        }
        // Whatever the cell wrote inside its retained output capability becomes
        // the manifest. Symlinks and special files reject the whole snapshot;
        // no path is followed outside the approved attempt directory.
        let snapshot = attempt
            .snapshot_bounded(invocation.resource_cap.max_output_bytes)
            .map_err(|error| {
                permanent(
                    ErrorClass::PolicyViolation,
                    format!("cannot hash retained step output: {error}"),
                )
            })?;
        if snapshot.bytes_produced > invocation.resource_cap.max_output_bytes {
            return Err(permanent(
                ErrorClass::PolicyViolation,
                format!(
                    "kernel produced {} bytes, exceeding the admitted {} byte output cap",
                    snapshot.bytes_produced, invocation.resource_cap.max_output_bytes
                ),
            ));
        }

        Ok(StepOutput {
            artifacts: snapshot.artifact_bytes,
            exit_code: Some(0),
            bytes_produced: snapshot.bytes_produced,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::model::ProjectId;
    use crate::workflow::admission::{probe_kernel, KernelAdmissionRequest};
    use crate::workflow::executor::{
        ExecutionPolicy, WorkflowExecutionRequest, WorkflowExecutor, WorkflowState,
    };
    use crate::workflow::kernel::{KernelKind, KernelManifest, ResourceCap};
    use crate::workflow::{
        CachePolicy, ComputeEnvironment, NetworkPolicy, ResourceLimits, StepKind, WorkflowSpec,
        WorkflowStep,
    };
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::PathBuf;
    #[cfg(not(target_os = "macos"))]
    use std::process::Command;
    use tempfile::tempdir;

    /// A real python3, or skip. This suite exists to prove the runner against a
    /// real interpreter; a stub would prove only that the stub was called.
    fn python3() -> Option<PathBuf> {
        #[cfg(target_os = "macos")]
        {
            let path = PathBuf::from(
                "/Library/Developer/CommandLineTools/Library/Frameworks/\
                 Python3.framework/Versions/3.9/Resources/Python.app/Contents/MacOS/Python",
            );
            path.is_file().then_some(path)
        }
        #[cfg(not(target_os = "macos"))]
        let out = Command::new("sh")
            .args(["-c", "command -v python3"])
            .output()
            .ok()?;
        #[cfg(not(target_os = "macos"))]
        let path = PathBuf::from(String::from_utf8(out.stdout).ok()?.trim());
        #[cfg(not(target_os = "macos"))]
        path.is_absolute().then_some(path)
    }

    fn environment() -> ComputeEnvironment {
        ComputeEnvironment {
            environment_id: "env-pyrunner".into(),
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

    fn spec(cell: &str) -> WorkflowSpec {
        WorkflowSpec {
            workflow_id: "wf-pyrunner".into(),
            project_id: ProjectId("proj-pyrunner".into()),
            name: "python runner e2e".into(),
            steps: vec![WorkflowStep {
                step_id: "compute".into(),
                kind: StepKind::NotebookCell,
                connector_id: None,
                notebook_cell: Some(cell.to_string()),
                inputs: vec![],
                parameters: BTreeMap::new(),
                timeout_secs: 120,
                retry_policy: None,
                cache_policy: CachePolicy::NoCache,
                acceptance_rules: vec![],
            }],
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

    struct Fixture {
        _dir: tempfile::TempDir,
        executor: WorkflowExecutor,
        io: WorkflowIoCapability,
    }

    fn fixture(python: &Path) -> Fixture {
        fixture_with_cap(python, None)
    }

    fn fixture_with_cap(python: &Path, resource_cap: Option<ResourceCap>) -> Fixture {
        let dir = tempdir().unwrap();
        let store_root = dir.path().join("store");
        fs::create_dir_all(&store_root).unwrap();
        let io = WorkflowIoCapability::open_existing_confined(&store_root, dir.path()).unwrap();
        let pinned = Arc::new(PinnedExecutable::pin(python).expect("pin test interpreter"));

        // Probed, not hand-built: the executor refuses a kernel that was not
        // admitted, so this exercises admission on the way in.
        // Stock policy on purpose: require_process_isolation stays true, and
        // spawning the loop satisfies it. Nothing here lowers a safety setting
        // to make the test pass.
        let mut admission_request =
            KernelAdmissionRequest::new("py-e2e", KernelKind::Python, python)
                .with_admitted_by("python-runner-tests")
                .with_probe_timeout(Duration::from_secs(60));
        if let Some(resource_cap) = resource_cap {
            admission_request.resource_cap = resource_cap;
        }
        let admission = probe_kernel(&admission_request).expect("probe");
        assert_eq!(
            admission.admission_status,
            AdmissionStatus::Admitted,
            "{admission:?}"
        );

        let runner = PythonLoopRunner::new(io.share(), pinned);
        // NotebookCell is deliberately absent from the DEFAULT policy —
        // running arbitrary code is meant to require an explicit decision, not
        // a default. Opting in here is that decision, made visibly.
        let mut policy = ExecutionPolicy::default();
        policy.allowed_step_kinds.insert(StepKind::NotebookCell);

        let executor = WorkflowExecutor::from_io(&store_root, &io, environment())
            .with_policy(policy)
            .with_runner(Arc::new(runner))
            .with_kernels(KernelManifest {
                kernels: vec![admission],
                default_python: None,
                default_r: None,
                default_julia: None,
            })
            .expect("kernels");

        Fixture {
            _dir: dir,
            executor,
            io,
        }
    }

    fn put_cell(io: &WorkflowIoCapability, code: &str) {
        io.stage_cell(&hex_sha256(code.as_bytes()), code.as_bytes())
            .unwrap();
    }

    fn request(spec: WorkflowSpec, op: &str) -> WorkflowExecutionRequest {
        WorkflowExecutionRequest {
            operation_id: op.into(),
            session_id: "sess-1".into(),
            owner_id: "owner-1".into(),
            spec,
        }
    }

    fn failure_detail(report: &crate::workflow::executor::WorkflowRunReport) -> String {
        let mut parts: Vec<String> = report
            .attempts
            .iter()
            .filter_map(|a| a.error_detail.clone())
            .collect();
        // A step refused before any attempt existed reports here instead, and
        // an empty diagnostic is how the first run of this suite looked.
        parts.extend(
            report
                .run
                .refused_steps
                .iter()
                .map(|r| format!("refused {}: {}", r.step_id, r.detail)),
        );
        parts.join(" | ")
    }

    fn assert_failed_without_commit(
        report: &crate::workflow::executor::WorkflowRunReport,
        expected_detail: &str,
    ) {
        let detail = failure_detail(report);
        assert_eq!(
            report.run.state,
            WorkflowState::Failed,
            "workflow unexpectedly succeeded: {detail}"
        );
        assert_eq!(
            report.artifacts_committed, 0,
            "commits: {:?}",
            report.commits
        );
        assert!(report.commits.is_empty(), "commits: {:?}", report.commits);
        assert!(
            detail.contains(expected_detail),
            "missing '{expected_detail}' in failure detail: {detail}"
        );
    }

    #[cfg(target_os = "macos")]
    fn python_path_literal(path: &Path) -> String {
        serde_json::to_string(
            path.to_str()
                .expect("macOS sandbox test path must be valid UTF-8"),
        )
        .expect("encode macOS sandbox test path")
    }

    #[cfg(target_os = "macos")]
    fn execute_macos_cell(
        fixture: &Fixture,
        code: &str,
        operation_id: &str,
    ) -> crate::workflow::executor::WorkflowRunReport {
        put_cell(&fixture.io, code);
        fixture
            .executor
            .execute(&request(spec(code), operation_id))
            .expect("execute adversarial macOS cell")
    }

    #[cfg(target_os = "macos")]
    fn assert_macos_sandbox_denied(
        fixture: &Fixture,
        report: &crate::workflow::executor::WorkflowRunReport,
    ) {
        let detail = failure_detail(report);
        assert_eq!(
            report.run.state,
            WorkflowState::Failed,
            "sandbox attack did not fail the workflow: {detail}"
        );
        assert_eq!(
            report.artifacts_committed, 0,
            "sandbox-denied execution committed an artifact: {:?}",
            report.commits
        );
        assert!(
            report.commits.is_empty(),
            "sandbox-denied execution returned commits: {:?}",
            report.commits
        );
        assert!(
            report
                .attempts
                .iter()
                .any(|attempt| attempt.error_class == Some(ErrorClass::PolicyViolation)),
            "sandbox refusal was not classified as PolicyViolation: {detail}"
        );
        assert!(
            detail.contains("OS sandbox denied kernel operation"),
            "sandbox refusal did not carry the OS-denial diagnostic: {detail}"
        );

        let commit_root = fixture._dir.path().join("store/workflow-commits");
        assert!(
            fs::read_dir(&commit_root)
                .map(|mut entries| entries.next().is_none())
                .unwrap_or(true),
            "sandbox-denied execution left a durable commit"
        );
    }

    /// THE end-to-end proof: a WorkflowSpec runs to Succeeded through a real
    /// interpreter, and the committed artifacts are the bytes the cell wrote.
    #[test]
    fn a_workflow_runs_end_to_end_through_a_real_interpreter() {
        let Some(python) = python3() else {
            eprintln!("SKIP: no python3 on PATH");
            return;
        };
        let fx = fixture(&python);
        let code = "import os\n\
                    p = os.path.join(os.environ['LUMEN_KERNEL_OUTPUT_DIR'], 'result.json')\n\
                    open(p, 'w').write('{\"mean\": 1.5}')\n\
                    print('computed')\n";
        put_cell(&fx.io, code);

        let report = fx
            .executor
            .execute(&request(spec(code), "op-e2e-1"))
            .expect("execute");
        assert_eq!(
            report.run.state,
            WorkflowState::Succeeded,
            "failed: {}",
            failure_detail(&report)
        );

        let commit = report.commits.first().expect("an artifact commit");

        // Recomputed here rather than read back from the record that claims it.
        let expected_stdout = hex_sha256(b"computed\n");
        assert!(
            commit
                .output_manifest
                .values()
                .any(|d| d == &expected_stdout),
            "stdout not committed with its true digest: {:?}",
            commit.output_manifest
        );
        assert!(
            commit
                .output_manifest
                .keys()
                .any(|k| k.ends_with("result.json")),
            "the file the cell wrote is missing: {:?}",
            commit.output_manifest
        );
        assert_eq!(
            report.artifacts_committed, 1,
            "expected one first-time commit"
        );
    }

    #[test]
    fn drain_bounded_retains_only_the_limit_and_counts_every_byte() {
        let bytes = vec![b'x'; 64 * 1024 + 37];
        let (retained, total) = drain_bounded(Some(std::io::Cursor::new(bytes.clone())), 1024);
        assert_eq!(retained, bytes[..1024]);
        assert_eq!(total, bytes.len() as u64);
    }

    #[test]
    fn invocation_environment_rejects_loader_and_python_injection_keys() {
        for key in [
            "LD_PRELOAD",
            "LD_AUDIT",
            "LD_LIBRARY_PATH",
            "GLIBC_TUNABLES",
            "GCONV_PATH",
            "LOCPATH",
            "PYTHONPATH",
            "PYTHONHOME",
            "PYTHONSTARTUP",
            "PYTHONINSPECT",
            "PYTHONUSERBASE",
        ] {
            let environment =
                std::collections::BTreeMap::from([(key.to_string(), "ATTACK_VALUE".to_string())]);
            let failure = validate_invocation_environment(&environment)
                .unwrap_err();
            assert_eq!(failure.class, ErrorClass::PolicyViolation);
            assert!(
                failure.detail.contains(key),
                "diagnostic did not name rejected key {key}: {}",
                failure.detail
            );
            assert!(!failure.retryable);
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_post_load_seal_denies_python_and_loader_reexec_before_user_artifacts() {
        let python = python3().expect("real Python is required for post-load exec-seal proof");
        let loader = super::super::pinned_executable::parse_linux_elf_interp(&python)
            .expect("parse admitted Python PT_INTERP");
        let fixture = fixture(&python);
        let code = format!(
            r#"
import errno
import ctypes
import json
import os
import sys

attack = "open('ATTACK_SUCCEEDED', 'w').write('unsafe')"
libc = ctypes.CDLL(None, use_errno=True)
argv = (ctypes.c_char_p * 7)(
    sys.executable.encode(), b"-I", b"-S", b"-c", attack.encode(), None, None
)
envp = (ctypes.c_char_p * 1)(None)
result = libc.syscall(
    {execveat}, -100, sys.executable.encode(), argv, envp, 0
)
if result != -1 or ctypes.get_errno() != errno.EPERM:
    raise RuntimeError(
        f"execveat returned {{result}} errno {{ctypes.get_errno()}}, expected -1/EPERM"
    )

targets = [
    ("python", sys.executable, [sys.executable, "-I", "-S", "-c", attack]),
    ("loader", {loader}, [{loader}, sys.executable, "-I", "-S", "-c", attack]),
]
blocked = {{}}
blocked["execveat"] = errno.EPERM
for label, executable, argv in targets:
    try:
        os.execv(executable, argv)
    except OSError as error:
        if error.errno != errno.EPERM:
            raise RuntimeError(f"{{label}} exec returned errno {{error.errno}}, expected EPERM")
        blocked[label] = error.errno
    else:
        raise RuntimeError(f"{{label}} exec unexpectedly returned")

with open("exec-denied.json", "w") as output:
    json.dump(blocked, output, sort_keys=True)
"#,
            loader = serde_json::to_string(
                loader
                    .to_str()
                    .expect("Linux loader path must be valid UTF-8"),
            )
            .expect("encode loader path"),
            execveat = libc::SYS_execveat,
        );
        put_cell(&fixture.io, &code);
        let report = fixture
            .executor
            .execute(&request(spec(&code), "op-linux-post-load-exec-seal"))
            .expect("execute post-load exec-seal proof");
        assert_eq!(
            report.run.state,
            WorkflowState::Succeeded,
            "{}",
            failure_detail(&report)
        );
        let manifest = &report.commits[0].output_manifest;
        assert!(
            manifest.contains_key("exec-denied.json"),
            "the cell did not observe exact EPERM denials: {manifest:?}"
        );
        assert!(
            !manifest.contains_key("ATTACK_SUCCEEDED"),
            "a secondary executable image ran before artifact commit: {manifest:?}"
        );
    }

    #[test]
    fn child_observes_admitted_cpu_file_and_fd_limits() {
        let Some(python) = python3() else {
            panic!("real Python is required for resource-limit proof");
        };
        let cap = ResourceCap {
            max_memory_mb: 512,
            max_cpu_seconds: 7,
            max_output_bytes: 4096,
            max_file_descriptors: 16,
        };
        let fx = fixture_with_cap(&python, Some(cap));
        let code = r#"
import json
import resource

expected = {
    "cpu": (resource.RLIMIT_CPU, 7),
    "file": (resource.RLIMIT_FSIZE, 4096),
    "nofile": (resource.RLIMIT_NOFILE, 16),
}
observed = {}
for name, (kind, ceiling) in expected.items():
    soft, hard = resource.getrlimit(kind)
    if soft <= 0 or hard <= 0 or soft > ceiling or hard > ceiling:
        raise RuntimeError(
            f"{name} rlimit was not applied: soft={soft}, hard={hard}, ceiling={ceiling}"
        )
    observed[name] = [soft, hard]
with open("observed-limits.json", "w") as output:
    json.dump(observed, output, sort_keys=True)
"#;
        put_cell(&fx.io, code);
        let report = fx
            .executor
            .execute(&request(spec(code), "op-observed-rlimits"))
            .expect("execute rlimit proof");
        assert_eq!(
            report.run.state,
            WorkflowState::Succeeded,
            "{}",
            failure_detail(&report)
        );
        assert!(
            report.commits[0]
                .output_manifest
                .contains_key("observed-limits.json"),
            "the child did not publish its observed limits"
        );
    }

    #[test]
    fn console_output_over_cap_fails_without_commit_or_pipe_deadlock() {
        let Some(python) = python3() else {
            panic!("real Python is required for console-cap proof");
        };
        let cap = ResourceCap {
            max_memory_mb: 512,
            max_cpu_seconds: 30,
            max_output_bytes: 1024,
            max_file_descriptors: 32,
        };
        let fx = fixture_with_cap(&python, Some(cap));
        let code = "import sys\nsys.stdout.write('x' * 4096)\nsys.stderr.write('y' * 4096)\n";
        put_cell(&fx.io, code);
        let started = Instant::now();
        let report = fx
            .executor
            .execute(&request(spec(code), "op-console-cap"))
            .expect("execute console-cap proof");
        assert_failed_without_commit(&report, "console output exceeded");
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "bounded pipe draining deadlocked"
        );
    }

    #[test]
    fn aggregate_file_output_over_cap_fails_without_commit() {
        let Some(python) = python3() else {
            panic!("real Python is required for aggregate-output proof");
        };
        let cap = ResourceCap {
            max_memory_mb: 512,
            max_cpu_seconds: 30,
            max_output_bytes: 1024,
            max_file_descriptors: 32,
        };
        let fx = fixture_with_cap(&python, Some(cap));
        let code = "open('first.bin', 'wb').write(b'a' * 700)\nopen('second.bin', 'wb').write(b'b' * 700)\n";
        put_cell(&fx.io, code);
        let report = fx
            .executor
            .execute(&request(spec(code), "op-aggregate-cap"))
            .expect("execute aggregate-cap proof");
        assert_failed_without_commit(&report, "1024 byte cap");
    }

    /// The sandbox holds when driven by the executor, not only standalone.
    #[test]
    fn os_sandbox_blocks_socket_creation_without_external_network_access() {
        let Some(python) = python3() else {
            eprintln!("SKIP: no python3 on PATH");
            return;
        };
        let fx = fixture(&python);
        let code = "import socket\n\
                    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)\n\
                    s.connect(('127.0.0.1', 9))\n";
        put_cell(&fx.io, code);

        let report = fx
            .executor
            .execute(&request(spec(code), "op-net-1"))
            .expect("execute");
        assert_eq!(report.run.state, WorkflowState::Failed);

        let detail = failure_detail(&report);
        assert!(
            detail.contains("OS sandbox denied kernel operation")
                && (detail.contains("Operation not permitted")
                    || detail.contains("PermissionError")
                    || detail.contains("[Errno 1]")),
            "denial not recorded: {detail}"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_sandbox_blocks_all_external_write_primitives_without_commit() {
        let Some(python) = python3() else {
            panic!("protected macOS test Python is unavailable");
        };
        let fixture = fixture(&python);
        let outside = fixture._dir.path().join("outside");
        fs::create_dir(&outside).unwrap();
        let direct = outside.join("direct.bin");
        let os_open = outside.join("os-open.bin");
        let renamed = outside.join("renamed.bin");
        let symlink_target = outside.join("symlink.bin");
        let ctypes_target = outside.join("ctypes.bin");
        let code = format!(
            r#"
import ctypes
import errno
import os

direct_path = {direct}
os_open_path = {os_open}
rename_path = {renamed}
symlink_target = {symlink_target}
ctypes_path = {ctypes_target}
denied = []

def expect_denied(label, action):
    try:
        action()
    except OSError as exc:
        if exc.errno not in (errno.EPERM, errno.EACCES):
            raise
        denied.append(label)
    else:
        raise RuntimeError(label + " unexpectedly wrote outside the sandbox")

def direct_write():
    with open(direct_path, "wb") as handle:
        handle.write(b"direct-escape")

def os_open_write():
    fd = os.open(os_open_path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    try:
        os.write(fd, b"os-open-escape")
    finally:
        os.close(fd)

def rename_write():
    with open("rename-source.bin", "wb") as handle:
        handle.write(b"rename-escape")
    os.rename("rename-source.bin", rename_path)

def symlink_write():
    os.symlink(symlink_target, "escape-link")
    with open("escape-link", "wb") as handle:
        handle.write(b"symlink-escape")

def ctypes_write():
    libc = ctypes.CDLL(None, use_errno=True)
    libc.open.argtypes = [ctypes.c_char_p, ctypes.c_int, ctypes.c_int]
    libc.open.restype = ctypes.c_int
    fd = libc.open(
        ctypes_path.encode(),
        os.O_WRONLY | os.O_CREAT | os.O_TRUNC,
        0o600,
    )
    if fd < 0:
        value = ctypes.get_errno()
        raise OSError(value, os.strerror(value), ctypes_path)
    try:
        os.write(fd, b"ctypes-escape")
    finally:
        os.close(fd)

for label, action in (
    ("direct", direct_write),
    ("os.open", os_open_write),
    ("rename", rename_write),
    ("symlink", symlink_write),
    ("ctypes", ctypes_write),
):
    expect_denied(label, action)

if len(denied) != 5:
    raise RuntimeError("not every external write primitive was attempted")
raise PermissionError(errno.EPERM, "all external write primitives were denied")
"#,
            direct = python_path_literal(&direct),
            os_open = python_path_literal(&os_open),
            renamed = python_path_literal(&renamed),
            symlink_target = python_path_literal(&symlink_target),
            ctypes_target = python_path_literal(&ctypes_target),
        );

        let report = execute_macos_cell(&fixture, &code, "op-macos-write-escapes");
        assert_macos_sandbox_denied(&fixture, &report);
        for path in [direct, os_open, renamed, symlink_target, ctypes_target] {
            assert!(
                !path.exists(),
                "macOS sandbox allowed external bytes at {}",
                path.display()
            );
        }
        assert!(
            fs::read_dir(&outside).unwrap().next().is_none(),
            "macOS sandbox left bytes outside its approved output root"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_sandbox_refuses_sensitive_read_without_artifact_commit() {
        let Some(python) = python3() else {
            panic!("protected macOS test Python is unavailable");
        };
        let fixture = fixture(&python);
        let secret = fixture._dir.path().join("sensitive-fixture.txt");
        let secret_bytes = b"lumen-sensitive-fixture-must-not-become-an-artifact";
        fs::write(&secret, secret_bytes).unwrap();
        let code = format!(
            r#"
secret_path = {secret}
with open(secret_path, "rb") as source:
    stolen = source.read()
with open("stolen-sensitive-fixture.bin", "wb") as artifact:
    artifact.write(stolen)
"#,
            secret = python_path_literal(&secret),
        );

        let report = execute_macos_cell(&fixture, &code, "op-macos-sensitive-read");
        assert_macos_sandbox_denied(&fixture, &report);
        assert_eq!(
            fs::read(&secret).unwrap(),
            secret_bytes,
            "sandboxed cell changed the sensitive fixture"
        );
        assert!(
            !failure_detail(&report)
                .contains(std::str::from_utf8(secret_bytes).expect("fixture text is UTF-8")),
            "sensitive fixture bytes leaked into the workflow diagnostic"
        );
        assert!(
            report.commits.iter().all(|commit| !commit
                .output_manifest
                .contains_key("stolen-sensitive-fixture.bin")),
            "sensitive fixture became a committed artifact"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_sandbox_refuses_signalling_parent_without_commit() {
        let Some(python) = python3() else {
            panic!("protected macOS test Python is unavailable");
        };
        let fixture = fixture(&python);
        let code = "import os\n\
                    os.kill(os.getppid(), 0)\n\
                    open('parent-signal-was-allowed', 'w').write('unsafe')\n";

        let report = execute_macos_cell(&fixture, code, "op-macos-parent-signal");
        assert_macos_sandbox_denied(&fixture, &report);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_sandbox_refuses_exec_of_bin_sh_without_commit() {
        let Some(python) = python3() else {
            panic!("protected macOS test Python is unavailable");
        };
        let fixture = fixture(&python);
        let code = "import os\n\
                    os.execv('/bin/sh', ['/bin/sh', '-c', 'exit 0'])\n";

        let report = execute_macos_cell(&fixture, code, "op-macos-bin-sh-exec");
        assert_macos_sandbox_denied(&fixture, &report);
    }

    /// A substituted cell body must not execute under a digest that names
    /// something else.
    #[test]
    fn a_source_that_does_not_match_its_digest_is_refused() {
        let Some(python) = python3() else {
            eprintln!("SKIP: no python3 on PATH");
            return;
        };
        let fx = fixture(&python);
        let code = "x = 1\n";
        put_cell(&fx.io, code);
        // Keep the filename, swap the bytes: the store now lies about content.
        fx.io
            .shared_root()
            .replace_atomic(
                &PathBuf::from("workflow-cells").join(hex_sha256(code.as_bytes())),
                b"import os; os.system('echo pwned')\n",
            )
            .unwrap();

        let report = fx
            .executor
            .execute(&request(spec(code), "op-swap-1"))
            .expect("execute");
        assert_eq!(report.run.state, WorkflowState::Failed);
        let detail = failure_detail(&report);
        assert!(detail.contains("hashing to"), "wrong refusal: {detail}");
    }
}
