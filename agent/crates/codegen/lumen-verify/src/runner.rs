//! Step execution with timeout.

use super::{Diagnostic, StepResult};
use crate::steps::Step;
use anyhow::{Context, Result};
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use wait_timeout::ChildExt;

const MAX_CAPTURE_BYTES: usize = 256 * 1024;

/// Run a single verification step with timeout.
///
/// If the tool is not found on PATH the step is skipped (returns ok=true with
/// a "SKIP" note) — a missing tool is never a failure.
pub fn run_step(root: &Path, step: &Step, timeout_secs: u64) -> Result<StepResult> {
    let start = Instant::now();

    // Check if tool exists — skip if not found (don't treat missing tool as failure).
    let executable = match which::which(&step.command) {
        Ok(path) => path,
        Err(_) => {
            return Ok(StepResult {
                language: step.language.clone(),
                command: format!("{} {}", step.command, step.args.join(" ")),
                ok: true,
                output: format!("SKIP: {} not found on PATH", step.command),
                diagnostics: vec![],
                duration_ms: start.elapsed().as_millis() as u64,
                skipped: true,
                timed_out: false,
            });
        }
    };

    // Execute the exact path we just resolved. Re-resolving the command name
    // through PATH after the allowlist check would leave a check/use race.
    let mut command = Command::new(&executable);
    command
        .args(&step.args)
        .current_dir(root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if step.command == "go" {
        // GOFLAGS may contain `-toolexec`, which would turn a fixed verifier
        // command into arbitrary command execution inherited from the agent's
        // environment. Verification intentionally uses its fixed argv only.
        command.env_remove("GOFLAGS").env("GOTOOLCHAIN", "local");
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }

    let mut child = command
        .spawn()
        .with_context(|| format!("spawn verification command {}", step.command))?;
    let stdout = child.stdout.take().context("capture verifier stdout")?;
    let stderr = child.stderr.take().context("capture verifier stderr")?;
    let stdout_reader = std::thread::spawn(move || read_bounded(stdout));
    let stderr_reader = std::thread::spawn(move || read_bounded(stderr));

    let timeout = Duration::from_secs(timeout_secs.max(1));
    let (status, timed_out) = match child.wait_timeout(timeout)? {
        Some(status) => (status, false),
        None => {
            kill_process_tree(&mut child).context("kill timed-out verification command")?;
            (
                child
                    .wait()
                    .context("reap timed-out verification command")?,
                true,
            )
        }
    };
    let (stdout, stdout_truncated) = join_reader(stdout_reader, "stdout")?;
    let (stderr, stderr_truncated) = join_reader(stderr_reader, "stderr")?;
    let mut combined = format!(
        "{}{}",
        String::from_utf8_lossy(&stdout),
        String::from_utf8_lossy(&stderr)
    );
    if stdout_truncated || stderr_truncated {
        combined.push_str("\n[verification output truncated]\n");
    }
    if timed_out {
        combined.push_str(&format!(
            "\nverification timed out after {} seconds\n",
            timeout.as_secs()
        ));
    }

    let ok = status.success() && !timed_out;
    let mut diagnostics = if ok {
        vec![]
    } else {
        parse_diagnostics(&step.language, &combined)
    };
    if !ok && diagnostics.is_empty() {
        diagnostics.push(Diagnostic {
            file: String::new(),
            line: None,
            col: None,
            severity: if timed_out { "TIMEOUT" } else { "ERROR" }.into(),
            message: combined.trim().to_string(),
        });
    }

    Ok(StepResult {
        language: step.language.clone(),
        command: format!("{} {}", step.command, step.args.join(" ")),
        ok,
        output: combined,
        diagnostics,
        duration_ms: start.elapsed().as_millis() as u64,
        skipped: false,
        timed_out,
    })
}

#[cfg(unix)]
fn kill_process_tree(child: &mut std::process::Child) -> std::io::Result<()> {
    use nix::sys::signal::{Signal, killpg};
    use nix::unistd::Pid;

    match killpg(Pid::from_raw(child.id() as i32), Signal::SIGKILL) {
        Ok(()) => Ok(()),
        Err(_) => child.kill(),
    }
}

#[cfg(not(unix))]
fn kill_process_tree(child: &mut std::process::Child) -> std::io::Result<()> {
    child.kill()
}

fn read_bounded(mut reader: impl Read) -> std::io::Result<(Vec<u8>, bool)> {
    let mut captured = Vec::new();
    let mut truncated = false;
    let mut buffer = [0u8; 8192];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        let remaining = MAX_CAPTURE_BYTES.saturating_sub(captured.len());
        if remaining > 0 {
            captured.extend_from_slice(&buffer[..read.min(remaining)]);
        }
        truncated |= read > remaining;
    }
    Ok((captured, truncated))
}

fn join_reader(
    handle: std::thread::JoinHandle<std::io::Result<(Vec<u8>, bool)>>,
    stream: &str,
) -> Result<(Vec<u8>, bool)> {
    handle
        .join()
        .map_err(|_| anyhow::anyhow!("verification {stream} reader panicked"))?
        .with_context(|| format!("read verification {stream}"))
}

/// Parse diagnostics from tool output (Go / Python / TS).
fn parse_diagnostics(language: &str, output: &str) -> Vec<Diagnostic> {
    match language {
        "go" => parse_go_diagnostics(output),
        "python" => parse_python_diagnostics(output),
        "typescript" => parse_ts_diagnostics(output),
        _ => vec![],
    }
}

/// Parse Go compiler errors: `file.go:10:5: message`
fn parse_go_diagnostics(output: &str) -> Vec<Diagnostic> {
    let re = regex::Regex::new(r"^(.+?):(\d+):(\d+):\s*(.+)$").unwrap();
    let mut diags = Vec::new();
    for line in output.lines() {
        if let Some(caps) = re.captures(line) {
            diags.push(Diagnostic {
                file: caps[1].to_string(),
                line: caps[2].parse().ok(),
                col: caps[3].parse().ok(),
                severity: "ERROR".into(),
                message: caps[4].to_string(),
            });
        }
    }
    diags
}

/// Parse Python (ruff / pytest) diagnostics.
fn parse_python_diagnostics(output: &str) -> Vec<Diagnostic> {
    let re = regex::Regex::new(r"^(.+?\.py):(\d+):(\d+):\s*(.+)").unwrap();
    let mut diags = Vec::new();
    for line in output.lines() {
        if let Some(caps) = re.captures(line) {
            diags.push(Diagnostic {
                file: caps[1].to_string(),
                line: caps[2].parse().ok(),
                col: caps[3].parse().ok(),
                severity: "ERROR".into(),
                message: caps[4].to_string(),
            });
        }
    }
    // pytest: FAILED file.py::test_name - message
    let pt_re = regex::Regex::new(r"FAILED\s+(.+?\.py).+-\s*(.+)").unwrap();
    for line in output.lines() {
        if let Some(caps) = pt_re.captures(line) {
            diags.push(Diagnostic {
                file: caps[1].to_string(),
                line: None,
                col: None,
                severity: "FAIL".into(),
                message: caps[2].to_string(),
            });
        }
    }
    if diags.is_empty() && !output.trim().is_empty() {
        // Catch-all: return raw output as a single diagnostic
        diags.push(Diagnostic {
            file: String::new(),
            line: None,
            col: None,
            severity: "ERROR".into(),
            message: output.trim().to_string(),
        });
    }
    diags
}

/// Parse TypeScript (tsc / jest) diagnostics.
fn parse_ts_diagnostics(output: &str) -> Vec<Diagnostic> {
    let re =
        regex::Regex::new(r"^(.+?)\((\d+),(\d+)\):\s*(error|warning)\s+TS(\d+):\s*(.+)").unwrap();
    let mut diags = Vec::new();
    for line in output.lines() {
        if let Some(caps) = re.captures(line) {
            diags.push(Diagnostic {
                file: caps[1].to_string(),
                line: caps[2].parse().ok(),
                col: caps[3].parse().ok(),
                severity: format!("{}/TS{}", &caps[4], &caps[5]),
                message: caps[6].to_string(),
            });
        }
    }
    diags
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[cfg(unix)]
    #[test]
    fn command_timeout_is_enforced_and_reported() {
        let tmp = TempDir::new().unwrap();
        let step = Step {
            language: "test".into(),
            label: "timeout fixture".into(),
            command: "sleep".into(),
            args: vec!["5".into()],
        };
        let started = Instant::now();
        let result = run_step(tmp.path(), &step, 1).unwrap();
        assert!(result.timed_out);
        assert!(!result.ok);
        assert!(result.output.contains("timed out after 1 seconds"));
        assert!(started.elapsed() < Duration::from_secs(4));
    }
}
