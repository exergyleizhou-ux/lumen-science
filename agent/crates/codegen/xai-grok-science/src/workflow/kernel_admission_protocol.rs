//! Durable, SessionActor-gated kernel admission protocol.
//!
//! This module owns records and artifacts, not permission. Product callers
//! must call `begin_kernel_admission`, await the production permission manager,
//! persist the decision, and call `finish_kernel_admission` only for Allow.

use super::{AdmissionStatus, KernelAdmission, KernelAdmissionRequest};
use crate::csv::ScienceRunTicket;
use crate::{
    Approval, ApprovalDecision, Artifact, CallId, Evidence, Provenance, Result, RunContext,
    RunRecord, RunState, ScienceError, ScienceStore,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path};

/// Durable result of an operator-approved kernel admission assessment.
///
/// `Rejected` means the authorized assessment completed and found an unsafe
/// kernel. It is distinct from an operator denial, which never executes the
/// interpreter and never creates this result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KernelAdmissionResult {
    pub run: RunRecord,
    pub admission: KernelAdmission,
    pub artifacts: Vec<Artifact>,
    pub evidence: Vec<Evidence>,
    pub provenance: Vec<Provenance>,
    pub approvals: Vec<Approval>,
    pub replay_after: u64,
}

/// Create the run and Pending approval before the permission manager is
/// awaited. This function never reads, hashes, or executes the interpreter.
pub fn begin_kernel_admission(
    store: &ScienceStore,
    context: RunContext,
) -> Result<ScienceRunTicket> {
    let ticket = ScienceRunTicket {
        project_id: context.project_id.clone(),
        run_id: context.run_id.clone(),
        owner_id: context.owner_id.clone(),
        call_id: CallId::new("science_kernel_admission"),
    };
    store.create_run(context)?;
    store.append_event(
        &ticket.run_id,
        "SessionActor",
        "run.created",
        serde_json::json!({ "kind": "kernel_admission" }),
    )?;
    store.request_approval(Approval {
        project_id: ticket.project_id.clone(),
        run_id: ticket.run_id.clone(),
        call_id: ticket.call_id.clone(),
        owner_id: ticket.owner_id.clone(),
        decision: ApprovalDecision::Pending,
        decided_at: None,
    })?;
    store.transition(&ticket.run_id, RunState::AwaitingApproval, None)?;
    Ok(ticket)
}

/// Execute and persist an admission assessment after, and only after, the
/// owning run has a durable Allow decision and is Running.
pub fn finish_kernel_admission(
    store: &ScienceStore,
    ticket: ScienceRunTicket,
    request: &KernelAdmissionRequest,
) -> Result<KernelAdmissionResult> {
    let run = store.load_run(&ticket.run_id)?;
    if ticket.project_id != run.context.project_id
        || ticket.owner_id != run.context.owner_id
        || run.state != RunState::Running
        || store
            .approvals(&ticket.run_id)?
            .iter()
            .find(|approval| approval.call_id == ticket.call_id)
            .is_none_or(|approval| approval.decision != ApprovalDecision::Allow)
    {
        return Err(ScienceError::Invalid(
            "kernel admission requires an allowed running run".into(),
        ));
    }

    match finish_allowed_kernel_admission(store, &ticket, request) {
        Ok(result) => Ok(result),
        Err(error) => {
            let cleanup_error = store
                .discard_running_outputs(
                    &ticket.project_id,
                    &ticket.run_id,
                    &ticket.owner_id,
                    &ticket.call_id,
                    &[Path::new("kernel-admission.json")],
                )
                .err();
            let detail = match &cleanup_error {
                Some(cleanup_error) => format!(
                    "{error}; kernel admission artifact cleanup also failed: {cleanup_error}"
                ),
                None => error.to_string(),
            };
            let _ = store.append_recoverable_commit_event(
                &ticket.run_id,
                "SessionActor",
                "kernel_admission.failed",
                serde_json::json!({
                    "reason": detail,
                    "artifact_cleanup": if cleanup_error.is_none() {
                        "completed"
                    } else {
                        "failed_non_serviceable"
                    },
                }),
            );
            store
                .transition(&ticket.run_id, RunState::Failed, Some(detail.clone()))
                .map_err(|terminal_error| {
                    ScienceError::Invalid(format!(
                        "kernel admission failed ({error}) and its Failed terminal could not be persisted: {terminal_error}"
                    ))
                })?;
            match cleanup_error {
                Some(_) => Err(ScienceError::Invalid(detail)),
                None => Err(error),
            }
        }
    }
}

fn finish_allowed_kernel_admission(
    store: &ScienceStore,
    ticket: &ScienceRunTicket,
    request: &KernelAdmissionRequest,
) -> Result<KernelAdmissionResult> {
    // Bind the exact request before anything executes or creates an artifact.
    // A non-UTF-8 path, for example, must fail the run rather than leave a
    // partially committed record with no provenance digest.
    let request_bytes = request_binding_bytes(request)?;
    let admission = super::probe_kernel(request)?;
    let artifact_bytes = serde_json::to_vec_pretty(&admission)?;
    let artifact = store.put_artifact(
        &ticket.project_id,
        &ticket.run_id,
        &ticket.owner_id,
        ticket.call_id.clone(),
        Path::new("kernel-admission.json"),
        &artifact_bytes,
        "application/json",
        "record",
    )?;

    let status = match admission.admission_status {
        AdmissionStatus::Admitted => "admitted",
        AdmissionStatus::Rejected => "rejected",
        AdmissionStatus::Pending => "pending",
        AdmissionStatus::Unavailable => "unavailable",
    };
    store.add_provenance(Provenance {
        run_id: ticket.run_id.clone(),
        source_uri: format!("file://{}", request.interpreter_path.display()),
        source_commit: None,
        source_path: Some(request.interpreter_path.display().to_string()),
        license: "local runtime identity probe".into(),
        retrieved_at: Utc::now(),
        input_sha256: format!("{:x}", Sha256::digest(&request_bytes)),
        tool: "kernel-admission-v1 inside SessionActor".into(),
        environment: BTreeMap::from([
            ("authority".into(), "SessionActor".into()),
            (
                "execution_network_requirement".into(),
                if request.policy.require_no_network {
                    "network_disabled_required".into()
                } else {
                    "operator_policy_allows_network".into()
                },
            ),
            (
                "identity_probe_authorization".into(),
                "operator_authorized_via_session_permission".into(),
            ),
            (
                "identity_probe_network".into(),
                "not_enforced_during_identity_probe".into(),
            ),
            ("kernel_kind".into(), format!("{:?}", request.kind)),
            ("verdict".into(), status.into()),
        ]),
    })?;
    store.add_evidence(Evidence {
        run_id: ticket.run_id.clone(),
        claim: format!(
            "kernel {} admission assessment completed as {status}",
            request.kernel_id
        ),
        source: request.interpreter_path.display().to_string(),
        artifact_sha256: Some(artifact.sha256.clone()),
        verified_at: Utc::now(),
    })?;
    store.append_event(
        &ticket.run_id,
        "SessionActor",
        "kernel_admission.completed",
        serde_json::json!({
            "kernel_id": request.kernel_id,
            "status": status,
            "artifact_sha256": artifact.sha256,
        }),
    )?;
    // Gather the response before the terminal write. After the transition no
    // fallible operation remains, so a returned success and durable Succeeded
    // cannot diverge.
    let artifacts = store.artifacts(&ticket.run_id)?;
    let evidence = store.evidence(&ticket.run_id)?;
    let provenance = store.provenance(&ticket.run_id)?;
    let approvals = store.approvals(&ticket.run_id)?;
    let events = store.events_after(&ticket.run_id, 0, 1_000)?;
    let run = store.transition_succeeded_verified(&ticket.run_id)?;
    Ok(KernelAdmissionResult {
        artifacts,
        evidence,
        provenance,
        approvals,
        replay_after: events.last().map_or(0, |event| event.seq),
        run,
        admission,
    })
}

fn request_binding_bytes(request: &KernelAdmissionRequest) -> Result<Vec<u8>> {
    #[derive(Serialize)]
    struct Binding<'a> {
        kernel_id: &'a str,
        kind: super::KernelKind,
        interpreter_path: &'a Path,
        allowed_root: Option<&'a Path>,
        supplied_executable_hash: Option<&'a str>,
        package_lock_path: Option<&'a Path>,
        supplied_package_lock_hash: Option<&'a str>,
        version_probe_args: Option<&'a [String]>,
        probe_timeout_ms: u128,
        policy: &'a super::KernelPolicy,
        resource_cap: &'a super::ResourceCap,
        admitted_by: &'a str,
    }

    Ok(serde_json::to_vec(&Binding {
        kernel_id: &request.kernel_id,
        kind: request.kind,
        interpreter_path: &request.interpreter_path,
        allowed_root: request.allowed_root.as_deref(),
        supplied_executable_hash: request.supplied_executable_hash.as_deref(),
        package_lock_path: request.package_lock_path.as_deref(),
        supplied_package_lock_hash: request.supplied_package_lock_hash.as_deref(),
        version_probe_args: request.version_probe_args.as_deref(),
        probe_timeout_ms: request.probe_timeout.as_millis(),
        policy: &request.policy,
        resource_cap: &request.resource_cap,
        admitted_by: &request.admitted_by,
    })?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ProjectId, RunId, workflow::KernelKind};
    use std::{fs, path::PathBuf, time::Duration};
    use tempfile::tempdir;

    fn context(root: &Path, project: &str, owner: &str) -> RunContext {
        RunContext {
            run_id: RunId::new_v7(),
            project_id: ProjectId::new(project),
            session_id: "session-kernel".into(),
            owner_id: owner.into(),
            workspace_root: root.to_path_buf(),
            provider: "offline-deterministic".into(),
            approval_policy: "production-session-permission".into(),
            tool_profile: "science-kernel-admission-v1".into(),
            artifact_root: root.join("science-store").join("runs"),
            environment: BTreeMap::from([(
                "network".into(),
                "not_enforced_during_identity_probe".into(),
            )]),
        }
    }

    fn request(path: impl Into<PathBuf>) -> KernelAdmissionRequest {
        KernelAdmissionRequest::new("py-test", KernelKind::Python, path)
            .with_admitted_by("test-suite")
            .with_probe_timeout(Duration::from_secs(60))
    }

    #[cfg(unix)]
    fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join(name);
        fs::write(&path, body).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[cfg(unix)]
    #[test]
    fn finish_requires_allow_and_does_not_execute_early() {
        let temp = tempdir().unwrap();
        let store = ScienceStore::new(temp.path().join("science-store"));
        let marker = temp.path().join("probe-ran");
        let path = script(
            temp.path(),
            "python3",
            &format!(
                "#!/bin/sh\nprintf ran > '{}'\necho 'Python 3.12.0'\n",
                marker.display()
            ),
        );
        let ticket = begin_kernel_admission(&store, context(temp.path(), "p", "alice")).unwrap();
        assert_eq!(
            store.load_run(&ticket.run_id).unwrap().state,
            RunState::AwaitingApproval
        );
        assert!(finish_kernel_admission(&store, ticket.clone(), &request(&path)).is_err());
        assert!(
            !marker.exists(),
            "finish before Allow executed the interpreter"
        );
        assert!(store.artifacts(&ticket.run_id).unwrap().is_empty());
        assert!(store.evidence(&ticket.run_id).unwrap().is_empty());
        assert!(store.provenance(&ticket.run_id).unwrap().is_empty());
    }

    #[test]
    fn non_allow_terminals_never_create_artifacts() {
        for (decision, state) in [
            (ApprovalDecision::Deny, RunState::Denied),
            (ApprovalDecision::Timeout, RunState::TimedOut),
            (ApprovalDecision::Cancel, RunState::Cancelled),
        ] {
            let temp = tempdir().unwrap();
            let store = ScienceStore::new(temp.path().join("science-store"));
            let ticket =
                begin_kernel_admission(&store, context(temp.path(), "p", "alice")).unwrap();
            crate::csv::finish_without_execution(&store, &ticket, decision, "focused test")
                .unwrap();
            assert_eq!(store.load_run(&ticket.run_id).unwrap().state, state);
            assert!(store.artifacts(&ticket.run_id).unwrap().is_empty());
            assert!(store.evidence(&ticket.run_id).unwrap().is_empty());
            assert!(store.provenance(&ticket.run_id).unwrap().is_empty());
        }
    }

    #[cfg(unix)]
    #[test]
    fn allow_commits_store_owned_hashed_assessment() {
        let temp = tempdir().unwrap();
        let store_root = temp.path().join("science-store");
        let store = ScienceStore::new(&store_root);
        let path = script(temp.path(), "python3", "#!/bin/sh\necho 'Python 3.12.0'\n");
        let ticket = begin_kernel_admission(&store, context(temp.path(), "p", "alice")).unwrap();
        crate::csv::mark_allowed(&store, &ticket).unwrap();
        let result = finish_kernel_admission(&store, ticket.clone(), &request(&path)).unwrap();
        assert_eq!(result.run.state, RunState::Succeeded);
        assert_eq!(result.admission.admission_status, AdmissionStatus::Admitted);
        assert_eq!(result.artifacts.len(), 1);
        assert_eq!(result.evidence.len(), 1);
        assert_eq!(result.provenance.len(), 1);
        assert_eq!(result.approvals[0].decision, ApprovalDecision::Allow);
        assert_eq!(
            result.provenance[0]
                .environment
                .get("identity_probe_network")
                .map(String::as_str),
            Some("not_enforced_during_identity_probe")
        );
        assert_eq!(
            result.provenance[0]
                .environment
                .get("identity_probe_authorization")
                .map(String::as_str),
            Some("operator_authorized_via_session_permission")
        );
        assert!(
            !result.provenance[0]
                .environment
                .values()
                .any(|value| value == "disabled"),
            "identity-probe provenance must not claim network enforcement"
        );
        let bytes = store
            .artifact_bytes(
                &ticket.project_id,
                &ticket.run_id,
                &ticket.owner_id,
                Path::new("kernel-admission.json"),
            )
            .unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(&bytes)),
            result.artifacts[0].sha256
        );
        assert!(!store_root.join("kernel-admission.json").exists());
    }

    #[cfg(unix)]
    #[test]
    fn post_allow_failure_is_durably_failed_without_an_artifact() {
        use std::{ffi::OsString, os::unix::ffi::OsStringExt};

        let temp = tempdir().unwrap();
        let store = ScienceStore::new(temp.path().join("science-store"));
        let ticket = begin_kernel_admission(&store, context(temp.path(), "p", "alice")).unwrap();
        crate::csv::mark_allowed(&store, &ticket).unwrap();
        let invalid_utf8_path =
            PathBuf::from(OsString::from_vec(vec![b'/', b't', b'm', b'p', b'/', 0xff]));
        assert!(
            finish_kernel_admission(&store, ticket.clone(), &request(invalid_utf8_path)).is_err()
        );
        assert_eq!(
            store.load_run(&ticket.run_id).unwrap().state,
            RunState::Failed
        );
        assert!(store.artifacts(&ticket.run_id).unwrap().is_empty());
        assert!(store.evidence(&ticket.run_id).unwrap().is_empty());
        assert!(store.provenance(&ticket.run_id).unwrap().is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn post_artifact_commit_failure_rolls_back_registration_and_bytes() {
        let temp = tempdir().unwrap();
        let store_root = temp.path().join("science-store");
        let store = ScienceStore::new(&store_root);
        let path = script(temp.path(), "python3", "#!/bin/sh\necho 'Python 3.12.0'\n");
        let ticket = begin_kernel_admission(&store, context(temp.path(), "p", "alice")).unwrap();
        crate::csv::mark_allowed(&store, &ticket).unwrap();

        fs::write(
            store_root
                .join("runs")
                .join(&ticket.run_id.0)
                .join("evidence.json"),
            b"{invalid-json",
        )
        .unwrap();

        assert!(finish_kernel_admission(&store, ticket.clone(), &request(&path)).is_err());
        assert_eq!(
            store.load_run(&ticket.run_id).unwrap().state,
            RunState::Failed
        );
        assert!(store.artifacts(&ticket.run_id).unwrap().is_empty());
        assert!(
            !store_root
                .join("runs")
                .join(&ticket.run_id.0)
                .join("artifacts")
                .join("kernel-admission.json")
                .exists(),
            "failed commit left kernel-admission bytes behind"
        );
        assert!(store.evidence(&ticket.run_id).unwrap().is_empty());
        assert!(
            store.provenance(&ticket.run_id).unwrap().is_empty(),
            "provenance committed before the injected evidence failure was not rolled back"
        );
    }

    #[cfg(unix)]
    #[test]
    fn owner_project_and_call_boundaries_fail_closed() {
        for mutate in ["owner", "project", "call"] {
            let temp = tempdir().unwrap();
            let store = ScienceStore::new(temp.path().join("science-store"));
            let marker = temp.path().join(format!("{mutate}-probe-ran"));
            let path = script(
                temp.path(),
                &format!("{mutate}-python3"),
                &format!(
                    "#!/bin/sh\nprintf ran > '{}'\necho 'Python 3.12.0'\n",
                    marker.display()
                ),
            );
            let mut ticket =
                begin_kernel_admission(&store, context(temp.path(), "p", "alice")).unwrap();
            crate::csv::mark_allowed(&store, &ticket).unwrap();
            match mutate {
                "owner" => ticket.owner_id = "mallory".into(),
                "project" => ticket.project_id = ProjectId::new("other"),
                "call" => ticket.call_id = CallId::new("forged-call"),
                _ => unreachable!(),
            }
            assert!(finish_kernel_admission(&store, ticket.clone(), &request(&path)).is_err());
            assert!(
                !marker.exists(),
                "forged {mutate} boundary executed the interpreter"
            );
            assert!(store.artifacts(&ticket.run_id).unwrap().is_empty());
            assert!(store.evidence(&ticket.run_id).unwrap().is_empty());
            assert!(store.provenance(&ticket.run_id).unwrap().is_empty());
        }
    }
}
