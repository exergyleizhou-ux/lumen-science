//! Deterministic offline file import with structured preview. Seam contract: S2.
//!
//! Import mirrors the CSV micro-loop protocol: `SessionActor` begins a durable
//! run with a pending approval, the production permission bridge decides, and
//! only then do bytes that transited Lumen's formal workspace tool dispatch
//! reach [`finish_import`]. The kernel independently re-derives the preview
//! from those bytes and fails closed on malformed content before any
//! authoritative record is written.

use crate::{
    Approval, ApprovalDecision, Artifact, CallId, Evidence, Provenance, Result, RunContext,
    RunRecord, RunState, ScienceError, ScienceStore,
    csv::{self, ScienceRunTicket},
    preview::{self, PreviewRecord},
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImportResult {
    pub run: RunRecord,
    pub artifacts: Vec<Artifact>,
    pub previews: Vec<PreviewRecord>,
    pub evidence: Vec<Evidence>,
    pub provenance: Vec<Provenance>,
    pub approvals: Vec<Approval>,
    pub replay_after: u64,
}

/// Phase one of the import protocol. `SessionActor` calls this before the
/// production permission manager is awaited, so every allow/deny/timeout/cancel
/// has a durable pending record to finish.
pub fn begin_import(store: &ScienceStore, context: RunContext) -> Result<ScienceRunTicket> {
    let ticket = ScienceRunTicket {
        project_id: context.project_id.clone(),
        run_id: context.run_id.clone(),
        owner_id: context.owner_id.clone(),
        call_id: CallId::new("science_file_import"),
    };
    store.create_run(context)?;
    store.append_event(&ticket.run_id, "SessionActor", "run.created", serde_json::json!({}))?;
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

/// Complete an allowed import run from bytes that transited Lumen's formal
/// workspace tool dispatch. The kernel independently re-derives the preview
/// from those bytes before registering the artifact, preview record,
/// provenance, and evidence. Malformed content fails the run closed: no
/// artifact is registered for content the previewer rejected.
pub fn finish_import(
    store: &ScienceStore,
    ticket: ScienceRunTicket,
    source_path: &Path,
    bytes: &[u8],
    tool_identity: impl Into<String>,
) -> Result<ImportResult> {
    let run = store.load_run(&ticket.run_id)?;
    if run.state != RunState::Running
        || store
            .approvals(&ticket.run_id)?
            .iter()
            .find(|approval| approval.call_id == ticket.call_id)
            .is_none_or(|approval| approval.decision != ApprovalDecision::Allow)
    {
        return Err(ScienceError::Invalid(
            "import requires an allowed running run".into(),
        ));
    }
    let built = match preview::build_preview(bytes, preview::DEFAULT_MAX_BYTES) {
        Ok(built) => built,
        Err(error) => {
            let reason = format!("preview failed closed: {error}");
            let _ = store.transition(&ticket.run_id, RunState::Failed, Some(reason.clone()));
            return Err(ScienceError::Invalid(reason));
        }
    };
    let file_name = source_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            ScienceError::Invalid("import source must have a UTF-8 file name".into())
        })?;
    let summary = preview::summarize(&built);
    let tool_identity = tool_identity.into();
    let artifact = store.put_artifact(
        &ticket.project_id,
        &ticket.run_id,
        &ticket.owner_id,
        ticket.call_id.clone(),
        Path::new(file_name),
        bytes,
        built.mime.clone(),
        summary.clone(),
    )?;
    store.add_preview(PreviewRecord {
        run_id: ticket.run_id.clone(),
        call_id: ticket.call_id.clone(),
        relative_path: artifact.relative_path.clone(),
        artifact_sha256: artifact.sha256.clone(),
        preview: built,
        generated_at: Utc::now(),
        tool: tool_identity.clone(),
    })?;
    store.add_provenance(Provenance {
        run_id: ticket.run_id.clone(),
        source_uri: format!("file://{}", source_path.display()),
        source_commit: None,
        source_path: Some(source_path.display().to_string()),
        license: "caller-asserted workspace file".into(),
        retrieved_at: Utc::now(),
        input_sha256: format!("{:x}", Sha256::digest(bytes)),
        tool: tool_identity.clone(),
        environment: BTreeMap::from([
            ("algorithm".into(), "content-sniff-preview-v1".into()),
            ("dispatch".into(), "WorkspaceOps::call_tool".into()),
        ]),
    })?;
    store.add_evidence(Evidence {
        run_id: ticket.run_id.clone(),
        claim: format!("imported {file_name}: {summary}"),
        source: source_path.display().to_string(),
        artifact_sha256: Some(artifact.sha256.clone()),
        verified_at: Utc::now(),
    })?;
    store.append_event(
        &ticket.run_id,
        "LumenToolDispatch",
        "tool.completed",
        serde_json::json!({
            "tool": tool_identity,
            "artifacts": [artifact.sha256]
        }),
    )?;
    let run = store.transition(&ticket.run_id, RunState::Succeeded, None)?;
    store.append_event(
        &ticket.run_id,
        "HostVerification",
        "run.succeeded",
        serde_json::json!({}),
    )?;
    aggregate(store, run)
}

pub fn aggregate(store: &ScienceStore, run: RunRecord) -> Result<ImportResult> {
    let run_id = &run.context.run_id;
    let events = store.events_after(run_id, 0, 1_000)?;
    Ok(ImportResult {
        artifacts: store.artifacts(run_id)?,
        previews: store.previews(run_id)?,
        evidence: store.evidence(run_id)?,
        provenance: store.provenance(run_id)?,
        approvals: store.approvals(run_id)?,
        replay_after: events.last().map_or(0, |event| event.seq),
        run,
    })
}

/// Kernel-test convenience: begin, allow, and finish an import without the
/// product permission bridge. Product code must use the SessionActor path.
pub fn execute_approved_import(
    store: &ScienceStore,
    context: RunContext,
    source_path: &Path,
    bytes: &[u8],
) -> Result<ImportResult> {
    let ticket = begin_import(store, context)?;
    csv::mark_allowed(store, &ticket)?;
    finish_import(store, ticket, source_path, bytes, "kernel-test-only/direct-executor")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ProjectId, preview::PreviewStats};

    const CSV: &[u8] = b"sample_id,condition,value\na,A,2\nb,A,4\n";
    const FASTA: &[u8] = b">seq1 example\nACGT\nAC\n>seq2\nTT\n";

    #[test]
    fn csv_import_records_preview_evidence_and_replays() {
        let temp = tempfile::tempdir().unwrap();
        let store = ScienceStore::new(temp.path());
        let result = execute_approved_import(
            &store,
            csv::fixture_context(temp.path(), ProjectId::new("p"), "alice"),
            Path::new("fixtures/micro.csv"),
            CSV,
        )
        .unwrap();
        assert_eq!(result.artifacts.len(), 1);
        let artifact = &result.artifacts[0];
        assert_eq!(artifact.mime, "text/csv");
        assert!(artifact.preview.contains("3 rows x 3 columns"));
        assert_eq!(result.previews.len(), 1);
        let record = &result.previews[0];
        assert_eq!(record.artifact_sha256, artifact.sha256);
        assert_eq!(
            record.preview.stats,
            PreviewStats::Tabular { rows: 3, columns: 3, ragged: false }
        );
        assert_eq!(result.evidence.len(), 1);
        assert_eq!(
            result.evidence[0].artifact_sha256.as_deref(),
            Some(artifact.sha256.as_str())
        );
        let before = serde_json::to_value(&result).unwrap();
        drop(store);
        let reopened = ScienceStore::new(temp.path());
        let run = reopened.load_run(&result.run.context.run_id).unwrap();
        let replay = aggregate(&reopened, run).unwrap();
        assert_eq!(before, serde_json::to_value(replay).unwrap());
    }

    #[test]
    fn fasta_import_records_sequence_stats() {
        let temp = tempfile::tempdir().unwrap();
        let store = ScienceStore::new(temp.path());
        let result = execute_approved_import(
            &store,
            csv::fixture_context(temp.path(), ProjectId::new("p"), "alice"),
            Path::new("fixtures/micro.fasta"),
            FASTA,
        )
        .unwrap();
        assert_eq!(result.artifacts[0].mime, "text/x-fasta");
        match &result.previews[0].preview.stats {
            PreviewStats::Fasta {
                sequences,
                total_residues,
                min_len,
                max_len,
                ..
            } => {
                assert_eq!((*sequences, *total_residues, *min_len, *max_len), (2, 8, 2, 6));
            }
            other => panic!("unexpected stats: {other:?}"),
        }
    }

    #[test]
    fn malformed_content_fails_run_closed_without_artifact() {
        let temp = tempfile::tempdir().unwrap();
        let store = ScienceStore::new(temp.path());
        let context = csv::fixture_context(temp.path(), ProjectId::new("p"), "alice");
        let run_id = context.run_id.clone();
        let error = execute_approved_import(
            &store,
            context,
            Path::new("fixtures/corrupt.fasta"),
            b">s\nAC\x01GT\n",
        )
        .unwrap_err();
        assert!(error.to_string().contains("preview failed closed"));
        let run = store.load_run(&run_id).unwrap();
        assert_eq!(run.state, RunState::Failed);
        assert!(store.artifacts(&run_id).unwrap().is_empty());
        assert!(store.previews(&run_id).unwrap().is_empty());
        assert!(store.evidence(&run_id).unwrap().is_empty());
    }

    #[test]
    fn import_without_allowance_is_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let store = ScienceStore::new(temp.path());
        let ticket = begin_import(
            &store,
            csv::fixture_context(temp.path(), ProjectId::new("p"), "alice"),
        )
        .unwrap();
        assert!(
            finish_import(&store, ticket, Path::new("a.csv"), CSV, "kernel-test-only")
                .is_err()
        );
    }
}
