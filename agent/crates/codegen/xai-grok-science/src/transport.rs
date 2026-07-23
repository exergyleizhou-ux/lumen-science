//! Real SSH/SCP transport primitives for the S3 connector seam.
//!
//! This module deliberately contains no policy decision.  Callers may invoke
//! it only after `connector::finish_ssh_scp_admission` has returned an allowed
//! ticket.  It never records paths, host names, keys, command lines, stdout,
//! or stderr in the durable Science store.

use crate::{
    Evidence, Provenance, Result, RunState, ScienceError, ScienceStore, connector::AdmissionTicket,
};
use chrono::Utc;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

const SCP: &str = "/usr/bin/scp";

/// A single, non-shell SCP operation.  Paths remain process-local and must
/// never be copied into a Science event, artifact preview, evidence, or
/// provenance record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScpOperation {
    Put {
        local_source: PathBuf,
        remote_path: String,
    },
    Get {
        remote_path: String,
        local_destination: PathBuf,
    },
}

/// Process-local fixture credentials and host pinning.  All paths are
/// intentionally non-serializable and have no durable representation.
#[derive(Debug, Clone)]
pub struct ScpExecutionConfig {
    pub identity_file: PathBuf,
    pub known_hosts_file: PathBuf,
    pub user: String,
    pub cancel: Arc<AtomicBool>,
    /// Debug fixture only; production passes `None`.
    pub fixture_ssh_config: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct TransportReceipt {
    pub run_id: crate::RunId,
    pub target_sha256: String,
    pub outcome: TransportOutcome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportOutcome {
    Complete,
    TimedOut,
    Cancelled,
    Failed,
}

/// Produce the pre-approval binding for an SCP operation. Raw paths remain
/// in the live process; only this digest can be attached to a ticket.
pub fn operation_sha256(operation: &ScpOperation) -> String {
    let mut hash = Sha256::new();
    hash.update(b"lumen-science-scp-operation-v1\0");
    match operation {
        ScpOperation::Put {
            local_source,
            remote_path,
        } => {
            hash.update(b"put\0");
            hash.update(local_source.as_os_str().as_encoded_bytes());
            hash.update(b"\0");
            hash.update(remote_path.as_bytes());
        }
        ScpOperation::Get {
            remote_path,
            local_destination,
        } => {
            hash.update(b"get\0");
            hash.update(remote_path.as_bytes());
            hash.update(b"\0");
            hash.update(local_destination.as_os_str().as_encoded_bytes());
        }
    }
    format!("{:x}", hash.finalize())
}

fn target_hash(ticket: &AdmissionTicket) -> String {
    let mut hash = Sha256::new();
    hash.update(b"lumen-science-ssh-scp-target-v1\0");
    hash.update(ticket.target.host.as_bytes());
    hash.update(b":");
    hash.update(ticket.target.port.to_be_bytes());
    format!("{:x}", hash.finalize())
}

fn reject_unsafe_remote_path(path: &str) -> Result<()> {
    if path.is_empty() || path.contains('\0') || path.contains('\n') || path.starts_with('-') {
        return Err(ScienceError::Invalid("invalid SCP remote path".into()));
    }
    Ok(())
}

fn ensure_regular_file(path: &Path, name: &str) -> Result<()> {
    if !path.is_file() {
        return Err(ScienceError::Invalid(format!(
            "{name} must be a regular file"
        )));
    }
    Ok(())
}

fn decode_base64(value: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut acc = 0u32;
    let mut bits = 0u8;
    for byte in value.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(value);
        bits += 6;
        while bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

fn verify_pinned_host_key(ticket: &AdmissionTicket, known_hosts: &Path) -> Result<()> {
    let expected_host = if ticket.target.port == 22 {
        ticket.target.host.clone()
    } else {
        format!("[{}]:{}", ticket.target.host, ticket.target.port)
    };
    let text = std::fs::read_to_string(known_hosts)?;
    let matches = text
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let hosts = fields.next()?;
            let _kind = fields.next()?;
            let key = fields.next()?;
            hosts
                .split(',')
                .any(|host| host == expected_host)
                .then_some(key)
        })
        .any(|key| {
            decode_base64(key).is_some_and(|bytes| {
                format!("{:x}", Sha256::digest(bytes)) == ticket.target.host_key_sha256
            })
        });
    matches.then_some(()).ok_or_else(|| {
        ScienceError::Invalid("known-host key does not match approved fingerprint".into())
    })
}

/// Invoke the system SCP binary with batch mode and a pinned, fixture-scoped
/// known-hosts file.  The child is killed and reaped on cancellation or
/// timeout.  Durable output is redacted to a target fingerprint and outcome.
pub fn execute_scp(
    store: &ScienceStore,
    ticket: AdmissionTicket,
    operation: ScpOperation,
    config: &ScpExecutionConfig,
) -> Result<TransportReceipt> {
    let run = store.load_run(&ticket.context.run_id)?;
    let allowed = store
        .approvals(&ticket.context.run_id)?
        .iter()
        .any(|approval| {
            approval.project_id == ticket.context.project_id
                && approval.owner_id == ticket.context.owner_id
                && approval.call_id == ticket.call_id
                && approval.decision == crate::ApprovalDecision::Allow
        });
    if run.context != ticket.context || run.state != RunState::AwaitingApproval || !allowed {
        return Err(ScienceError::Invalid(
            "SCP requires an allowed awaiting run".into(),
        ));
    }
    if ticket.operation_sha256.as_deref() != Some(&operation_sha256(&operation)) {
        return Err(ScienceError::Invalid(
            "SCP operation does not match the approved ticket".into(),
        ));
    }
    ensure_regular_file(&config.identity_file, "identity file")?;
    ensure_regular_file(&config.known_hosts_file, "known-hosts file")?;
    verify_pinned_host_key(&ticket, &config.known_hosts_file)?;
    if let Some(path) = &config.fixture_ssh_config {
        ensure_regular_file(path, "fixture SSH config")?;
    }
    if config.user.is_empty() || config.user.contains(['@', ':', '\n', '\0']) {
        return Err(ScienceError::Invalid("invalid SCP user".into()));
    }
    let target_sha256 = target_hash(&ticket);
    let (source, destination) = match &operation {
        ScpOperation::Put {
            local_source,
            remote_path,
        } => {
            ensure_regular_file(local_source, "local source")?;
            reject_unsafe_remote_path(remote_path)?;
            (
                local_source.clone(),
                PathBuf::from(format!(
                    "{}@{}:{}",
                    config.user, ticket.target.host, remote_path
                )),
            )
        }
        ScpOperation::Get {
            remote_path,
            local_destination,
        } => {
            reject_unsafe_remote_path(remote_path)?;
            if local_destination.exists() && !local_destination.is_file() {
                return Err(ScienceError::Invalid(
                    "local destination must be a file path".into(),
                ));
            }
            (
                PathBuf::from(format!(
                    "{}@{}:{}",
                    config.user, ticket.target.host, remote_path
                )),
                local_destination.clone(),
            )
        }
    };
    store.transition(&ticket.context.run_id, RunState::Running, None)?;
    store.append_event(
        &ticket.context.run_id,
        "LumenSshScpTransport",
        "connector.transport.started",
        serde_json::json!({
            "connector": "ssh-scp-v1", "target_sha256": target_sha256,
        }),
    )?;
    let timeout = Duration::from_millis(ticket.target.timeout_ms);
    let mut command = Command::new(SCP);
    command
        .args(["-q", "-B", "-i"])
        .arg(&config.identity_file)
        .args([
            "-o",
            "BatchMode=yes",
            "-o",
            "StrictHostKeyChecking=yes",
            "-o",
        ])
        .arg(format!(
            "UserKnownHostsFile={}",
            config.known_hosts_file.display()
        ))
        .arg("-o")
        .arg(format!("ConnectTimeout={}", timeout.as_secs().max(1)))
        .arg("-P")
        .arg(ticket.target.port.to_string());
    if let Some(path) = &config.fixture_ssh_config {
        command.arg("-F").arg(path);
    }
    let mut child = command
        .arg(&source)
        .arg(&destination)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let started = Instant::now();
    let outcome = loop {
        if config.cancel.load(Ordering::SeqCst) {
            let _ = child.kill();
            let _ = child.wait();
            break TransportOutcome::Cancelled;
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            break TransportOutcome::TimedOut;
        }
        if let Some(status) = child.try_wait()? {
            break if status.success() {
                TransportOutcome::Complete
            } else {
                TransportOutcome::Failed
            };
        }
        thread::sleep(Duration::from_millis(10));
    };
    if outcome == TransportOutcome::Complete {
        // The registered artifact is the locally verified byte sequence.  The
        // remote path is deliberately not used as its durable name.
        let verified_path = match &operation {
            ScpOperation::Put { local_source, .. } => local_source,
            ScpOperation::Get {
                local_destination, ..
            } => local_destination,
        };
        let bytes = std::fs::read(verified_path)?;
        let artifact = store.put_artifact(
            &ticket.context.project_id,
            &ticket.context.run_id,
            &ticket.context.owner_id,
            ticket.call_id.clone(),
            Path::new("ssh-scp-transfer.bin"),
            &bytes,
            "application/octet-stream",
            "SSH/SCP transfer bytes (redacted target)",
        )?;
        store.add_evidence(Evidence {
            run_id: ticket.context.run_id.clone(),
            claim: format!("SSH/SCP transfer completed; target_sha256={target_sha256}"),
            source: format!("ssh-scp-target-sha256:{target_sha256}"),
            artifact_sha256: Some(artifact.sha256.clone()),
            verified_at: Utc::now(),
        })?;
        store.add_provenance(Provenance {
            run_id: ticket.context.run_id.clone(),
            source_uri: format!("ssh-scp-target-sha256:{target_sha256}"),
            source_commit: None,
            source_path: None,
            license: "OpenSSH system binary (BSD license family)".into(),
            retrieved_at: Utc::now(),
            input_sha256: artifact.sha256,
            tool: "system-scp".into(),
            environment: BTreeMap::from([("target_sha256".into(), target_sha256.clone())]),
        })?;
    }
    let (state, kind) = match outcome {
        TransportOutcome::Complete => (RunState::Succeeded, "connector.transport.completed"),
        TransportOutcome::TimedOut => (RunState::TimedOut, "connector.transport.timed_out"),
        TransportOutcome::Cancelled => (RunState::Cancelled, "connector.transport.cancelled"),
        TransportOutcome::Failed => (RunState::Failed, "connector.transport.failed"),
    };
    store.transition(
        &ticket.context.run_id,
        state,
        Some("SSH/SCP transport terminal".into()),
    )?;
    store.append_event(
        &ticket.context.run_id,
        "LumenSshScpTransport",
        kind,
        serde_json::json!({
            "connector": "ssh-scp-v1", "target_sha256": target_sha256,
        }),
    )?;
    Ok(TransportReceipt {
        run_id: ticket.context.run_id,
        target_sha256,
        outcome,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_binding_is_direction_and_path_sensitive() {
        let put = ScpOperation::Put {
            local_source: PathBuf::from("fixture/input.bin"),
            remote_path: "data/input.bin".into(),
        };
        let other_remote = ScpOperation::Put {
            local_source: PathBuf::from("fixture/input.bin"),
            remote_path: "data/other.bin".into(),
        };
        let get = ScpOperation::Get {
            remote_path: "data/input.bin".into(),
            local_destination: PathBuf::from("fixture/input.bin"),
        };
        assert_ne!(operation_sha256(&put), operation_sha256(&other_remote));
        assert_ne!(operation_sha256(&put), operation_sha256(&get));
        assert_eq!(operation_sha256(&put).len(), 64);
    }

    #[test]
    fn remote_path_rejects_option_and_control_injection() {
        for path in ["", "-oProxyCommand=bad", "data\nother", "data\0other"] {
            assert!(reject_unsafe_remote_path(path).is_err(), "{path:?}");
        }
        assert!(reject_unsafe_remote_path("data/result.bin").is_ok());
    }
}
