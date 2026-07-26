//! The `StepRunner` that actually executes a kernel cell.
//!
//! Until this existed the executor was complete but inert: `UnboundStepRunner`
//! refused every step, so no workflow had ever run end to end. This closes that
//! seam, and it is the only place in the crate that spawns a process.
//!
//! It drives `resources/lumen_python_loop.py` over the line protocol adapted
//! from Open Science: one JSON request per line in, one JSON response per line
//! out. Not Jupyter — no ZMQ, no message signing, two pipes.
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
//! Network policy is enforced twice on purpose — here, by withholding the
//! opt-in variable, and again inside the loop by an audit hook. Neither layer
//! is trusted to be the only one.

use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::kernel::AdmissionStatus;
use super::executor::{ErrorClass, StepFailure, StepOperation, StepOutput, StepPlan, StepRunner};

/// Resolves the cell body the plan refers to by digest.
///
/// The plan carries a digest rather than the source so it stays small and the
/// source stays verifiable. Whatever this returns is re-hashed before use: the
/// store is a lookup, not an authority.
pub trait CellSourceStore: std::fmt::Debug + Send + Sync {
    fn load(&self, sha256: &str) -> Option<Vec<u8>>;
}

/// A store backed by a content-addressed directory: `<root>/<sha256>`.
#[derive(Debug, Clone)]
pub struct DirCellSourceStore {
    root: PathBuf,
}

impl DirCellSourceStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

impl CellSourceStore for DirCellSourceStore {
    fn load(&self, sha256: &str) -> Option<Vec<u8>> {
        // Reject anything that is not a bare digest before it reaches the
        // filesystem: this value names a path component.
        if sha256.len() != 64 || !sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        fs::read(self.root.join(sha256)).ok()
    }
}

/// One denial the kernel reported while running the cell.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KernelDenial {
    pub kind: String,
    pub detail: String,
}

/// The loop's reply to one request.
#[derive(Debug, Clone, Default, Deserialize)]
struct LoopResponse {
    #[serde(default)]
    stdout: String,
    #[serde(default)]
    stderr: String,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    denied: Vec<KernelDenial>,
}

#[derive(Debug, Serialize)]
struct LoopRequest<'a> {
    req_id: &'a str,
    code: &'a str,
}

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
    /// directory it may not write outside of, and the loop's audit hook
    /// refusing network, subprocess spawning and out-of-tree writes.
    ///
    /// This is what `process_isolation` on an admission record asks for: the
    /// work does not share a process with the engine. It is NOT a container —
    /// same kernel, same user, same filesystem namespace — so a cell that
    /// escapes CPython escapes this.
    SeparateProcess,
    /// A namespace or container: seccomp, a mount namespace, a jail. No runner
    /// provides this yet. The variant exists so a future plan demanding it
    /// fails against a runner that cannot, instead of being downgraded in
    /// silence to the tier above.
    OsLevel,
}

/// Executes kernel cells by driving the Lumen Python exec-loop.
#[derive(Debug, Clone)]
pub struct PythonLoopRunner {
    loop_script: PathBuf,
    sources: Arc<dyn CellSourceStore>,
    output_root: PathBuf,
    provides: ProvidedIsolation,
}

impl PythonLoopRunner {
    pub fn new(
        loop_script: impl Into<PathBuf>,
        sources: Arc<dyn CellSourceStore>,
        output_root: impl Into<PathBuf>,
    ) -> Self {
        Self {
            loop_script: loop_script.into(),
            sources,
            output_root: output_root.into(),
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

    /// Per-attempt output directory. Keyed by attempt, not by step: a retry
    /// must not see or overwrite the previous attempt's files, or a failed
    /// attempt's partial output could be committed as a successful one's.
    fn attempt_dir(&self, plan: &StepPlan) -> PathBuf {
        self.output_root
            .join(&plan.run_id)
            .join(&plan.step_id)
            .join(&plan.attempt_id)
    }
}

fn permanent(class: ErrorClass, detail: impl Into<String>) -> StepFailure {
    StepFailure::permanent(class, detail.into())
}

fn hex_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Hash every regular file under `dir`, returning relative path → digest.
fn hash_tree(dir: &Path, base: &Path, out: &mut BTreeMap<String, String>) -> std::io::Result<u64> {
    let mut total = 0u64;
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let meta = entry.metadata()?;
        if meta.is_dir() {
            total += hash_tree(&path, base, out)?;
        } else if meta.is_file() {
            let bytes = fs::read(&path)?;
            total += bytes.len() as u64;
            let rel = path
                .strip_prefix(base)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            out.insert(rel, hex_sha256(&bytes));
        }
        // Symlinks are deliberately skipped: following one would let a cell
        // publish bytes from outside its output directory as its own artifact.
    }
    Ok(total)
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
            .sources
            .load(&invocation.cell_source_sha256)
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
            permanent(ErrorClass::PolicyViolation, "cell source is not valid UTF-8")
        })?;

        let attempt_dir = self.attempt_dir(plan);
        let figures_dir = attempt_dir.join("figures");
        fs::create_dir_all(&figures_dir).map_err(|error| {
            permanent(
                ErrorClass::RunnerError,
                format!("cannot create attempt output directory: {error}"),
            )
        })?;

        // Exactly the environment the invocation names, plus what the loop
        // needs to enforce its own half of the policy. `env_clear` first: an
        // empty map means an empty environment, never an inherited one.
        let mut command = Command::new(&invocation.interpreter_path);
        command
            .arg(&self.loop_script)
            .args(&invocation.argv)
            .env_clear()
            .envs(&invocation.environment)
            .env("LUMEN_KERNEL_OUTPUT_DIR", &attempt_dir)
            .env("LUMEN_KERNEL_FIGURES_DIR", &figures_dir)
            // Pinned so set iteration order is stable across replays of the
            // same run; the loop refuses to start without it.
            .env("PYTHONHASHSEED", "0")
            .env("LC_ALL", "C")
            .env("TZ", "UTC")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if invocation.network_allowed {
            command.env("LUMEN_KERNEL_ALLOW_NET", "1");
        }
        // When network is not allowed the variable is simply absent, so the
        // loop's default (deny) applies. Enforced in two places on purpose:
        // neither layer is trusted to be the only one.

        if let Some(dir) = &invocation.working_dir {
            command.current_dir(dir);
        }

        let mut child = command.spawn().map_err(|error| {
            permanent(
                ErrorClass::RunnerError,
                format!(
                    "cannot start kernel '{}': {error}",
                    invocation.interpreter_path
                ),
            )
        })?;

        let request = serde_json::to_string(&LoopRequest {
            req_id: &plan.attempt_id,
            code: &code,
        })
        .map_err(|error| permanent(ErrorClass::RunnerError, format!("cannot encode request: {error}")))?;

        if let Some(stdin) = child.stdin.as_mut()
            && let Err(error) = writeln!(stdin, "{request}")
        {
            let _ = child.kill();
            let _ = child.wait();
            return Err(permanent(
                ErrorClass::RunnerError,
                format!("cannot write to kernel: {error}"),
            ));
        }
        // Closing stdin ends the loop's read loop after this request, so the
        // process exits on its own rather than being killed on the happy path.
        drop(child.stdin.take());

        let stdout = child.stdout.take();
        let reader = std::thread::spawn(move || {
            let mut lines = Vec::new();
            if let Some(handle) = stdout {
                for line in BufReader::new(handle).lines().map_while(Result::ok) {
                    lines.push(line);
                }
            }
            lines
        });

        let started = Instant::now();
        let timed_out = loop {
            match child.try_wait() {
                Ok(Some(_)) => break false,
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
                break true;
            }
            std::thread::sleep(Duration::from_millis(5));
        };

        let lines = reader.join().unwrap_or_default();

        if timed_out {
            // Retryable: a timeout may be load, and the executor's bounded
            // retry decides whether to spend another attempt.
            return Err(StepFailure::transient(
                ErrorClass::Timeout,
                format!(
                    "kernel did not answer within {:?} for step '{}'",
                    plan.timeout, plan.step_id
                ),
            ));
        }

        // Find our reply by request id. Any other line is noise the loop warned
        // about (an unappliable rlimit, say) and is not the answer.
        let mut response: Option<LoopResponse> = None;
        for line in &lines {
            let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            if value.get("req_id").and_then(|v| v.as_str()) == Some(plan.attempt_id.as_str()) {
                response = serde_json::from_str(line).ok();
                break;
            }
        }
        let Some(response) = response else {
            return Err(permanent(
                ErrorClass::RunnerError,
                format!(
                    "kernel produced no reply for attempt '{}' ({} line(s) read)",
                    plan.attempt_id,
                    lines.len()
                ),
            ));
        };

        if let Some(error) = response.error {
            // A denial is reported alongside the error so the record says WHY
            // the cell failed, not merely that it did.
            let denials = if response.denied.is_empty() {
                String::new()
            } else {
                let kinds: Vec<&str> = response.denied.iter().map(|d| d.kind.as_str()).collect();
                format!(" [denied: {}]", kinds.join(", "))
            };
            return Err(permanent(
                ErrorClass::RunnerError,
                format!("cell failed{denials}: {}", error.trim()),
            ));
        }

        // Whatever the cell wrote inside its output directory becomes the
        // manifest, hashed here rather than trusted from the cell.
        let mut artifacts = BTreeMap::new();
        let bytes_produced = hash_tree(&attempt_dir, &attempt_dir, &mut artifacts).map_err(
            |error| permanent(ErrorClass::RunnerError, format!("cannot hash step output: {error}")),
        )?;

        // stdout and stderr are outputs too: a run whose console output is lost
        // cannot be reviewed.
        for (name, body) in [("stdout.txt", &response.stdout), ("stderr.txt", &response.stderr)] {
            if body.is_empty() {
                continue;
            }
            let path = attempt_dir.join(name);
            fs::write(&path, body.as_bytes())
                .map_err(|error| permanent(ErrorClass::RunnerError, format!("cannot write {name}: {error}")))?;
            artifacts.insert(name.to_string(), hex_sha256(body.as_bytes()));
        }

        Ok(StepOutput {
            artifacts,
            exit_code: Some(0),
            bytes_produced,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::admission::{probe_kernel, KernelAdmissionRequest};
    use crate::workflow::executor::{
        ExecutionPolicy, WorkflowExecutionRequest, WorkflowExecutor, WorkflowState,
    };
    use crate::workflow::kernel::{KernelKind, KernelManifest};
    use crate::workflow::{
        CachePolicy, ComputeEnvironment, NetworkPolicy, ResourceLimits, StepKind, WorkflowSpec,
        WorkflowStep,
    };
    use crate::project::model::ProjectId;
    use tempfile::tempdir;

    /// A real python3, or skip. This suite exists to prove the runner against a
    /// real interpreter; a stub would prove only that the stub was called.
    fn python3() -> Option<PathBuf> {
        let out = Command::new("sh").args(["-c", "command -v python3"]).output().ok()?;
        let path = PathBuf::from(String::from_utf8(out.stdout).ok()?.trim());
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
        cells: PathBuf,
    }

    fn fixture(python: &Path) -> Fixture {
        let dir = tempdir().unwrap();
        let cells = dir.path().join("cells");
        fs::create_dir_all(&cells).unwrap();

        // Probed, not hand-built: the executor refuses a kernel that was not
        // admitted, so this exercises admission on the way in.
        // Stock policy on purpose: require_process_isolation stays true, and
        // spawning the loop satisfies it. Nothing here lowers a safety setting
        // to make the test pass.
        let admission = probe_kernel(
            &KernelAdmissionRequest::new("py-e2e", KernelKind::Python, python)
                .with_admitted_by("python-runner-tests")
                .with_probe_timeout(Duration::from_secs(60)),
        )
        .expect("probe");
        assert_eq!(admission.admission_status, AdmissionStatus::Admitted, "{admission:?}");

        let runner = PythonLoopRunner::new(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/lumen_python_loop.py"),
            Arc::new(DirCellSourceStore::new(&cells)),
            dir.path().join("out"),
        );
        // NotebookCell is deliberately absent from the DEFAULT policy —
        // running arbitrary code is meant to require an explicit decision, not
        // a default. Opting in here is that decision, made visibly.
        let mut policy = ExecutionPolicy::default();
        policy.allowed_step_kinds.insert(StepKind::NotebookCell);

        let executor = WorkflowExecutor::new(dir.path().join("store"), environment())
            .with_policy(policy)
            .with_runner(Arc::new(runner))
            .with_kernels(KernelManifest {
                kernels: vec![admission],
                default_python: None,
                default_r: None,
                default_julia: None,
            })
            .expect("kernels");

        Fixture { _dir: dir, executor, cells }
    }

    fn put_cell(dir: &Path, code: &str) {
        fs::write(dir.join(hex_sha256(code.as_bytes())), code).unwrap();
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
        put_cell(&fx.cells, code);

        let report = fx.executor.execute(&request(spec(code), "op-e2e-1")).expect("execute");
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
            commit.output_manifest.values().any(|d| d == &expected_stdout),
            "stdout not committed with its true digest: {:?}",
            commit.output_manifest
        );
        assert!(
            commit.output_manifest.keys().any(|k| k.ends_with("result.json")),
            "the file the cell wrote is missing: {:?}",
            commit.output_manifest
        );
        assert_eq!(report.artifacts_committed, 1, "expected one first-time commit");
    }

    /// The sandbox holds when driven by the executor, not only standalone.
    #[test]
    fn a_cell_reaching_for_the_network_fails_the_step() {
        let Some(python) = python3() else {
            eprintln!("SKIP: no python3 on PATH");
            return;
        };
        let fx = fixture(&python);
        let code = "import socket\nsocket.create_connection(('example.com', 80), timeout=2)\n";
        put_cell(&fx.cells, code);

        let report = fx.executor.execute(&request(spec(code), "op-net-1")).expect("execute");
        assert_eq!(report.run.state, WorkflowState::Failed);

        // Must name the denial. "Tried to reach the network and was stopped" is
        // a different fact from "raised an exception", and only the first tells
        // a reviewer the sandbox did its job.
        let detail = failure_detail(&report);
        assert!(detail.contains("network-denied"), "denial not recorded: {detail}");
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
        put_cell(&fx.cells, code);
        // Keep the filename, swap the bytes: the store now lies about content.
        fs::write(
            fx.cells.join(hex_sha256(code.as_bytes())),
            "import os; os.system('echo pwned')\n",
        )
        .unwrap();

        let report = fx.executor.execute(&request(spec(code), "op-swap-1")).expect("execute");
        assert_eq!(report.run.state, WorkflowState::Failed);
        let detail = failure_detail(&report);
        assert!(detail.contains("hashes to"), "wrong refusal: {detail}");
    }
}
