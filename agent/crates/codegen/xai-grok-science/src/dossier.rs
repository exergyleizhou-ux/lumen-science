//! Actor-owned biomedical evidence dossier composition.
//!
//! A dossier is not a second execution engine. It is a verified projection of
//! already-succeeded Science runs. Every source artifact is reopened through
//! [`ScienceStore`], re-hashed, and copied into the new run before the
//! manifest becomes authoritative.

use crate::{
    Approval, ApprovalDecision, Artifact, CallId, Evidence, ProjectId, Provenance, Result,
    RunContext, RunId, RunRecord, RunState, ScienceError, ScienceStore, csv::ScienceRunTicket,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

pub const DOSSIER_SCHEMA_VERSION: u32 = 1;
pub const DOSSIER_ADMISSION_SCHEMA_VERSION: u32 = 1;
pub const DOSSIER_ADMISSION_ENV_KEY: &str = "evidence_dossier_admission_sha256";
pub const MAX_SOURCE_RUNS: usize = 32;
pub const MAX_SOURCE_ARTIFACTS: usize = 256;
pub const MAX_BUNDLED_BYTES: u64 = 256 * 1024 * 1024;
pub const MAX_RECORDS_PER_SOURCE: usize = 1_024;
pub const MAX_MANIFEST_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_SOURCE_METADATA_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_TEXT_FIELD_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DossierArtifact {
    pub source_run_id: RunId,
    pub source_relative_path: PathBuf,
    pub bundled_relative_path: PathBuf,
    pub sha256: String,
    pub bytes: u64,
    pub mime: String,
    pub preview: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DossierSource {
    pub run_id: RunId,
    pub tool_profile: String,
    pub provider: String,
    pub artifacts: Vec<DossierArtifact>,
    pub evidence: Vec<Evidence>,
    pub provenance: Vec<Provenance>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DossierManifest {
    pub schema_version: u32,
    pub project_id: ProjectId,
    pub owner_id: String,
    pub session_id: String,
    pub dossier_run_id: RunId,
    pub admission_sha256: String,
    pub project_revision: String,
    pub title: String,
    pub research_question: String,
    pub generated_at: DateTime<Utc>,
    pub sources: Vec<DossierSource>,
    pub limitations: Vec<String>,
}

/// Content-addressed registry snapshot for one already-succeeded source run.
///
/// Artifact payload bytes are still reopened and re-hashed at Finish. This
/// snapshot additionally prevents artifact/evidence/provenance records from
/// changing while the user is deciding whether to Allow the dossier request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DossierSourceSnapshot {
    run_id: RunId,
    run_sha256: String,
    artifacts_sha256: String,
    evidence_sha256: String,
    provenance_sha256: String,
}

pub fn capture_source_snapshots(
    store: &ScienceStore,
    source_run_ids: &[RunId],
) -> Result<Vec<DossierSourceSnapshot>> {
    if source_run_ids.is_empty() || source_run_ids.len() > MAX_SOURCE_RUNS {
        return Err(ScienceError::Invalid(format!(
            "dossier requires 1..={MAX_SOURCE_RUNS} source runs"
        )));
    }
    let mut metadata_bytes = 0usize;
    source_run_ids
        .iter()
        .map(|run_id| {
            let run = store.load_run_bounded(run_id, remaining_metadata_bytes(metadata_bytes)?)?;
            charge_metadata(&mut metadata_bytes, &run)?;
            let artifacts = store.artifacts_bounded(
                run_id,
                MAX_SOURCE_ARTIFACTS,
                remaining_metadata_bytes(metadata_bytes)?,
            )?;
            charge_metadata(&mut metadata_bytes, &artifacts)?;
            let evidence = store.evidence_bounded(
                run_id,
                MAX_RECORDS_PER_SOURCE,
                remaining_metadata_bytes(metadata_bytes)?,
            )?;
            charge_metadata(&mut metadata_bytes, &evidence)?;
            let provenance = store.provenance_bounded(
                run_id,
                MAX_RECORDS_PER_SOURCE,
                remaining_metadata_bytes(metadata_bytes)?,
            )?;
            charge_metadata(&mut metadata_bytes, &provenance)?;
            validate_source_text(&artifacts, &evidence, &provenance)?;
            source_snapshot(&run, &artifacts, &evidence, &provenance)
        })
        .collect()
}

fn source_snapshot(
    run: &RunRecord,
    artifacts: &[Artifact],
    evidence: &[Evidence],
    provenance: &[Provenance],
) -> Result<DossierSourceSnapshot> {
    Ok(DossierSourceSnapshot {
        run_id: run.context.run_id.clone(),
        run_sha256: sha256_json(run)?,
        artifacts_sha256: sha256_json(artifacts)?,
        evidence_sha256: sha256_json(evidence)?,
        provenance_sha256: sha256_json(provenance)?,
    })
}

fn sha256_json<T: Serialize + ?Sized>(value: &T) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(value)?)))
}

/// Immutable, content-addressed request admitted before permission is asked.
///
/// The fields are private so the shell can retain this value as a capability
/// but cannot rewrite the approved source set, project revision, or report
/// content before Finish.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DossierAdmission {
    schema_version: u32,
    dossier_run_id: RunId,
    project_id: ProjectId,
    owner_id: String,
    session_id: String,
    workspace_root: PathBuf,
    artifact_root: PathBuf,
    project_revision: String,
    title: String,
    research_question: String,
    source_run_ids: Vec<RunId>,
    source_snapshots: Vec<DossierSourceSnapshot>,
    tool_identity: String,
}

impl DossierAdmission {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        context: &RunContext,
        source_snapshots: Vec<DossierSourceSnapshot>,
        project_revision: String,
        title: String,
        research_question: String,
        tool_identity: String,
    ) -> Result<Self> {
        let source_run_ids = source_snapshots
            .iter()
            .map(|snapshot| snapshot.run_id.clone())
            .collect::<Vec<_>>();
        validate_request(
            &source_run_ids,
            &project_revision,
            &title,
            &research_question,
        )?;
        if tool_identity.trim().is_empty()
            || tool_identity.len() > MAX_TEXT_FIELD_BYTES
            || tool_identity.contains('\0')
        {
            return Err(ScienceError::Invalid(
                "dossier tool identity must be bounded, non-empty, and contain no NUL".into(),
            ));
        }
        Ok(Self {
            schema_version: DOSSIER_ADMISSION_SCHEMA_VERSION,
            dossier_run_id: context.run_id.clone(),
            project_id: context.project_id.clone(),
            owner_id: context.owner_id.clone(),
            session_id: context.session_id.clone(),
            workspace_root: context.workspace_root.clone(),
            artifact_root: context.artifact_root.clone(),
            project_revision,
            title,
            research_question,
            source_run_ids,
            source_snapshots,
            tool_identity,
        })
    }

    pub fn sha256(&self) -> Result<String> {
        let bytes = serde_json::to_vec(self)?;
        Ok(format!("{:x}", Sha256::digest(bytes)))
    }

    pub fn source_run_ids(&self) -> &[RunId] {
        &self.source_run_ids
    }

    pub fn project_revision(&self) -> &str {
        &self.project_revision
    }

    fn validate(&self, context: &RunContext) -> Result<()> {
        if self.schema_version != DOSSIER_ADMISSION_SCHEMA_VERSION
            || self.dossier_run_id != context.run_id
            || self.project_id != context.project_id
            || self.owner_id != context.owner_id
            || self.session_id != context.session_id
            || self.workspace_root != context.workspace_root
            || self.artifact_root != context.artifact_root
        {
            return Err(ScienceError::Ownership);
        }
        validate_request(
            &self.source_run_ids,
            &self.project_revision,
            &self.title,
            &self.research_question,
        )?;
        if self.source_snapshots.len() != self.source_run_ids.len()
            || self
                .source_snapshots
                .iter()
                .zip(&self.source_run_ids)
                .any(|(snapshot, run_id)| &snapshot.run_id != run_id)
        {
            return Err(ScienceError::Invalid(
                "dossier source snapshots do not match the admitted source order".into(),
            ));
        }
        if self.tool_identity.trim().is_empty()
            || self.tool_identity.len() > MAX_TEXT_FIELD_BYTES
            || self.tool_identity.contains('\0')
        {
            return Err(ScienceError::Invalid(
                "dossier tool identity must be bounded, non-empty, and contain no NUL".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DossierResult {
    pub run: RunRecord,
    pub manifest: DossierManifest,
    pub artifacts: Vec<Artifact>,
    pub evidence: Vec<Evidence>,
    pub provenance: Vec<Provenance>,
    pub approvals: Vec<Approval>,
    pub replay_after: u64,
}

#[doc(hidden)]
pub fn begin_dossier(
    store: &ScienceStore,
    mut context: RunContext,
    admission: &DossierAdmission,
) -> Result<ScienceRunTicket> {
    admission.validate(&context)?;
    let admission_sha256 = admission.sha256()?;
    if context
        .environment
        .insert(DOSSIER_ADMISSION_ENV_KEY.into(), admission_sha256.clone())
        .is_some()
    {
        return Err(ScienceError::Invalid(
            "dossier admission environment key is reserved".into(),
        ));
    }
    let ticket = ScienceRunTicket {
        project_id: context.project_id.clone(),
        run_id: context.run_id.clone(),
        owner_id: context.owner_id.clone(),
        call_id: CallId::new(format!("science_evidence_dossier_{admission_sha256}")),
    };
    store.create_run(context)?;
    let admitted = (|| {
        store.append_event(
            &ticket.run_id,
            "SessionActor",
            "run.created",
            serde_json::json!({
                "operation": "evidence_dossier",
                "admission_sha256": admission_sha256,
                "project_revision": admission.project_revision,
                "source_runs": admission.source_run_ids.iter().map(|run| &run.0).collect::<Vec<_>>(),
            }),
        )?;
        store.request_approval(Approval {
            project_id: ticket.project_id.clone(),
            run_id: ticket.run_id.clone(),
            call_id: ticket.call_id.clone(),
            owner_id: ticket.owner_id.clone(),
            decision: ApprovalDecision::Pending,
            decided_at: None,
        })?;
        store.transition(&ticket.run_id, RunState::AwaitingApproval, None)
    })();
    if let Err(error) = admitted {
        if store.approvals(&ticket.run_id).is_ok_and(|approvals| {
            approvals.iter().any(|approval| {
                approval.call_id == ticket.call_id && approval.decision == ApprovalDecision::Pending
            })
        }) {
            let _ = store.decide_approval(
                &ticket.project_id,
                &ticket.run_id,
                &ticket.owner_id,
                &ticket.call_id,
                ApprovalDecision::Cancel,
            );
        }
        if store
            .load_run(&ticket.run_id)
            .is_ok_and(|run| !run.state.terminal())
        {
            let _ = store.transition(
                &ticket.run_id,
                RunState::Failed,
                Some(format!("dossier admission failed closed: {error}")),
            );
        }
        return Err(error);
    }
    Ok(ticket)
}

#[doc(hidden)]
pub fn finish_dossier(
    store: &ScienceStore,
    ticket: ScienceRunTicket,
    admission: DossierAdmission,
) -> Result<DossierResult> {
    let run = store.load_run(&ticket.run_id)?;
    admission.validate(&run.context)?;
    let admission_sha256 = admission.sha256()?;
    if run.context.environment.get(DOSSIER_ADMISSION_ENV_KEY) != Some(&admission_sha256) {
        return Err(ScienceError::Invalid(
            "evidence dossier admission digest does not match durable run".into(),
        ));
    }
    if run.state != RunState::Running
        || store
            .approvals(&ticket.run_id)?
            .iter()
            .find(|approval| approval.call_id == ticket.call_id)
            .is_none_or(|approval| approval.decision != ApprovalDecision::Allow)
    {
        return Err(ScienceError::Invalid(
            "evidence dossier requires an allowed running run".into(),
        ));
    }

    let mut written_paths = Vec::new();
    let result = commit_dossier(
        store,
        &ticket,
        &admission,
        &admission_sha256,
        &mut written_paths,
    );
    match result {
        Ok(result) => Ok(result),
        Err(error) => {
            let output_refs = written_paths
                .iter()
                .map(PathBuf::as_path)
                .collect::<Vec<_>>();
            let rollback = store.discard_running_outputs(
                &ticket.project_id,
                &ticket.run_id,
                &ticket.owner_id,
                &ticket.call_id,
                &output_refs,
            );
            let reason = match rollback {
                Ok(()) => format!("evidence dossier failed closed: {error}"),
                Err(rollback_error) => format!(
                    "evidence dossier failed closed: {error}; rollback failed: {rollback_error}"
                ),
            };
            let _ = store.transition(&ticket.run_id, RunState::Failed, Some(reason.clone()));
            Err(ScienceError::Invalid(reason))
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn commit_dossier(
    store: &ScienceStore,
    ticket: &ScienceRunTicket,
    admission: &DossierAdmission,
    admission_sha256: &str,
    written_paths: &mut Vec<PathBuf>,
) -> Result<DossierResult> {
    validate_request(
        &admission.source_run_ids,
        &admission.project_revision,
        &admission.title,
        &admission.research_question,
    )?;
    let dossier_run = store.load_run(&ticket.run_id)?;
    let mut seen = BTreeSet::new();
    let mut sources = Vec::with_capacity(admission.source_run_ids.len());
    let mut total_artifacts = 0usize;
    let mut total_bytes = 0u64;
    let mut metadata_bytes = 0usize;
    let mut source_payloads = Vec::<(PathBuf, Vec<u8>, String, String, String)>::new();

    for (source_index, source_run_id) in admission.source_run_ids.iter().enumerate() {
        if source_run_id == &ticket.run_id || !seen.insert(source_run_id.clone()) {
            return Err(ScienceError::Invalid(
                "dossier source runs must be unique and cannot include the dossier run".into(),
            ));
        }
        let source_run =
            store.load_run_bounded(source_run_id, remaining_metadata_bytes(metadata_bytes)?)?;
        charge_metadata(&mut metadata_bytes, &source_run)?;
        if source_run.state != RunState::Succeeded
            || source_run.context.project_id != ticket.project_id
            || source_run.context.owner_id != ticket.owner_id
            || source_run.context.session_id != dossier_run.context.session_id
            || source_run.context.workspace_root != dossier_run.context.workspace_root
            || source_run.context.artifact_root != dossier_run.context.artifact_root
        {
            return Err(ScienceError::Ownership);
        }
        let remaining_artifacts = MAX_SOURCE_ARTIFACTS.saturating_sub(total_artifacts);
        let artifacts = store.artifacts_bounded(
            source_run_id,
            remaining_artifacts,
            remaining_metadata_bytes(metadata_bytes)?,
        )?;
        charge_metadata(&mut metadata_bytes, &artifacts)?;
        let evidence = store.evidence_bounded(
            source_run_id,
            MAX_RECORDS_PER_SOURCE,
            remaining_metadata_bytes(metadata_bytes)?,
        )?;
        charge_metadata(&mut metadata_bytes, &evidence)?;
        let provenance = store.provenance_bounded(
            source_run_id,
            MAX_RECORDS_PER_SOURCE,
            remaining_metadata_bytes(metadata_bytes)?,
        )?;
        charge_metadata(&mut metadata_bytes, &provenance)?;
        validate_source_text(&artifacts, &evidence, &provenance)?;
        let actual_snapshot = source_snapshot(&source_run, &artifacts, &evidence, &provenance)?;
        if admission.source_snapshots.get(source_index) != Some(&actual_snapshot) {
            return Err(ScienceError::Invalid(format!(
                "source run {} changed after dossier admission",
                source_run_id.0
            )));
        }
        if artifacts.is_empty() || evidence.is_empty() || provenance.is_empty() {
            return Err(ScienceError::Invalid(
                "every dossier source run must have artifact, evidence, and provenance".into(),
            ));
        }

        total_artifacts = total_artifacts
            .checked_add(artifacts.len())
            .ok_or_else(|| ScienceError::Invalid("dossier artifact count overflow".into()))?;
        if total_artifacts > MAX_SOURCE_ARTIFACTS {
            return Err(ScienceError::Invalid(format!(
                "dossier may bundle at most {MAX_SOURCE_ARTIFACTS} source artifacts"
            )));
        }

        let mut dossier_artifacts = Vec::with_capacity(artifacts.len());
        for (artifact_index, artifact) in artifacts.iter().enumerate() {
            let matching_evidence = evidence
                .iter()
                .any(|item| item.artifact_sha256.as_deref() == Some(&artifact.sha256));
            let matching_provenance = provenance
                .iter()
                .find(|item| item.input_sha256 == artifact.sha256);
            if !matching_evidence || matching_provenance.is_none() {
                return Err(ScienceError::Invalid(
                    "every bundled artifact must be bound by matching evidence and provenance"
                        .into(),
                ));
            }
            let remaining_bytes = MAX_BUNDLED_BYTES.saturating_sub(total_bytes);
            if artifact.bytes > remaining_bytes {
                return Err(ScienceError::Invalid(format!(
                    "dossier source bytes exceed the {MAX_BUNDLED_BYTES}-byte cap"
                )));
            }
            let bytes = store.artifact_bytes_bounded(
                &ticket.project_id,
                source_run_id,
                &ticket.owner_id,
                &artifact.relative_path,
                remaining_bytes,
            )?;
            total_bytes = total_bytes
                .checked_add(bytes.len() as u64)
                .ok_or_else(|| ScienceError::Invalid("dossier byte count overflow".into()))?;
            if total_bytes > MAX_BUNDLED_BYTES {
                return Err(ScienceError::Invalid(format!(
                    "dossier source bytes exceed the {MAX_BUNDLED_BYTES}-byte cap"
                )));
            }
            let bundled_relative_path =
                bundled_path(source_index, artifact_index, &artifact.relative_path);
            dossier_artifacts.push(DossierArtifact {
                source_run_id: source_run_id.clone(),
                source_relative_path: artifact.relative_path.clone(),
                bundled_relative_path: bundled_relative_path.clone(),
                sha256: artifact.sha256.clone(),
                bytes: artifact.bytes,
                mime: artifact.mime.clone(),
                preview: artifact.preview.clone(),
            });
            source_payloads.push((
                bundled_relative_path,
                bytes,
                artifact.mime.clone(),
                artifact.preview.clone(),
                artifact.sha256.clone(),
            ));
        }
        sources.push(DossierSource {
            run_id: source_run_id.clone(),
            tool_profile: source_run.context.tool_profile,
            provider: source_run.context.provider,
            artifacts: dossier_artifacts,
            evidence,
            provenance,
        });
    }

    let manifest = DossierManifest {
        schema_version: DOSSIER_SCHEMA_VERSION,
        project_id: ticket.project_id.clone(),
        owner_id: ticket.owner_id.clone(),
        session_id: dossier_run.context.session_id.clone(),
        dossier_run_id: ticket.run_id.clone(),
        admission_sha256: admission_sha256.to_owned(),
        project_revision: admission.project_revision.clone(),
        title: admission.title.clone(),
        research_question: admission.research_question.clone(),
        generated_at: Utc::now(),
        sources,
        limitations: vec![
            "The dossier proves the bundled bytes and recorded provenance, not the truth of every scientific claim.".into(),
            "Offline-fixture connector runs are reproducibility evidence, not live-endpoint proof.".into(),
            "This research output is not medical advice or clinical certification.".into(),
        ],
    };

    for (path, bytes, mime, preview, expected_sha) in source_payloads {
        let copied = store.put_artifact(
            &ticket.project_id,
            &ticket.run_id,
            &ticket.owner_id,
            ticket.call_id.clone(),
            &path,
            &bytes,
            mime,
            preview,
        )?;
        written_paths.push(path);
        if copied.sha256 != expected_sha {
            return Err(ScienceError::Invalid(
                "bundled artifact digest changed while copying".into(),
            ));
        }
    }

    let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
    if manifest_bytes.len() > MAX_MANIFEST_BYTES {
        return Err(ScienceError::Invalid(format!(
            "dossier manifest exceeds the {MAX_MANIFEST_BYTES}-byte cap"
        )));
    }
    let manifest_artifact = store.put_artifact(
        &ticket.project_id,
        &ticket.run_id,
        &ticket.owner_id,
        ticket.call_id.clone(),
        Path::new("dossier.json"),
        &manifest_bytes,
        "application/json",
        "evidence-dossier-manifest",
    )?;
    written_paths.push(PathBuf::from("dossier.json"));
    let markdown = render_markdown(&manifest);
    let report_artifact = store.put_artifact(
        &ticket.project_id,
        &ticket.run_id,
        &ticket.owner_id,
        ticket.call_id.clone(),
        Path::new("dossier.md"),
        markdown.as_bytes(),
        "text/markdown",
        "evidence-dossier-report",
    )?;
    written_paths.push(PathBuf::from("dossier.md"));

    for source in &manifest.sources {
        for artifact in &source.artifacts {
            store.add_evidence(Evidence {
                run_id: ticket.run_id.clone(),
                claim: format!(
                    "Bundled source artifact {} from run {} was byte-verified.",
                    artifact.source_relative_path.display(),
                    source.run_id.0
                ),
                source: format!(
                    "lumen-science://run/{}/artifact/{}",
                    source.run_id.0,
                    artifact.source_relative_path.display()
                ),
                artifact_sha256: Some(artifact.sha256.clone()),
                verified_at: Utc::now(),
            })?;
            store.add_provenance(Provenance {
                run_id: ticket.run_id.clone(),
                source_uri: format!("lumen-science://run/{}", source.run_id.0),
                source_commit: None,
                source_path: Some(artifact.source_relative_path.display().to_string()),
                license: source
                    .provenance
                    .iter()
                    .find(|item| item.input_sha256 == artifact.sha256)
                    .map_or_else(
                        || "recorded-by-source-run".into(),
                        |item| item.license.clone(),
                    ),
                retrieved_at: Utc::now(),
                input_sha256: artifact.sha256.clone(),
                tool: admission.tool_identity.clone(),
                environment: BTreeMap::from([
                    ("dossier_schema".into(), DOSSIER_SCHEMA_VERSION.to_string()),
                    ("source_run_id".into(), source.run_id.0.clone()),
                    (
                        "project_revision".into(),
                        admission.project_revision.clone(),
                    ),
                    ("admission_sha256".into(), admission_sha256.to_owned()),
                ]),
            })?;
        }
    }
    for (artifact, claim, source_path) in [
        (
            &manifest_artifact,
            format!(
                "Dossier manifest binds {} source run(s) and {} source artifact(s).",
                manifest.sources.len(),
                total_artifacts
            ),
            None,
        ),
        (
            &report_artifact,
            format!(
                "Dossier report was rendered from byte-verified manifest {}.",
                manifest_artifact.sha256
            ),
            Some("dossier.json".to_owned()),
        ),
    ] {
        store.add_evidence(Evidence {
            run_id: ticket.run_id.clone(),
            claim,
            source: format!("lumen-science://run/{}", ticket.run_id.0),
            artifact_sha256: Some(artifact.sha256.clone()),
            verified_at: Utc::now(),
        })?;
        store.add_provenance(Provenance {
            run_id: ticket.run_id.clone(),
            source_uri: format!("lumen-science://run/{}", ticket.run_id.0),
            source_commit: None,
            source_path,
            license: "Lumen-generated-output; project policy applies".into(),
            retrieved_at: Utc::now(),
            input_sha256: artifact.sha256.clone(),
            tool: admission.tool_identity.clone(),
            environment: BTreeMap::from([
                ("dossier_schema".into(), DOSSIER_SCHEMA_VERSION.to_string()),
                (
                    "project_revision".into(),
                    admission.project_revision.clone(),
                ),
                ("admission_sha256".into(), admission_sha256.to_owned()),
            ]),
        })?;
    }
    store.append_recoverable_commit_event(
        &ticket.run_id,
        "SessionActor",
        "dossier.committed",
        serde_json::json!({
            "manifest_sha256": manifest_artifact.sha256,
            "admission_sha256": admission_sha256,
            "source_runs": admission.source_run_ids.iter().map(|run| &run.0).collect::<Vec<_>>(),
            "source_artifacts": total_artifacts,
            "bundled_bytes": total_bytes,
        }),
    )?;
    store.append_recoverable_commit_event(
        &ticket.run_id,
        "HostVerification",
        "dossier.verified",
        serde_json::json!({"operation": "evidence_dossier"}),
    )?;
    // Load every response component before making the terminal state visible.
    // A metadata read failure must still be able to roll the run back to
    // Failed; after Succeeded no transition is legal.
    let artifacts = store.artifacts(&ticket.run_id)?;
    let evidence = store.evidence(&ticket.run_id)?;
    let provenance = store.provenance(&ticket.run_id)?;
    let approvals = store.approvals(&ticket.run_id)?;
    let events = store.events_after(&ticket.run_id, 0, 1_000)?;
    let run = store.transition_succeeded_verified(&ticket.run_id)?;
    Ok(DossierResult {
        artifacts,
        evidence,
        provenance,
        approvals,
        replay_after: events.last().map_or(0, |event| event.seq),
        run,
        manifest,
    })
}

fn validate_request(
    source_run_ids: &[RunId],
    project_revision: &str,
    title: &str,
    research_question: &str,
) -> Result<()> {
    if source_run_ids.is_empty() || source_run_ids.len() > MAX_SOURCE_RUNS {
        return Err(ScienceError::Invalid(format!(
            "dossier requires 1..={MAX_SOURCE_RUNS} source runs"
        )));
    }
    if project_revision.len() != 64
        || !project_revision
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(ScienceError::Invalid(
            "project revision must be a lowercase SHA-256 digest".into(),
        ));
    }
    for (value, name) in [(title, "title"), (research_question, "research question")] {
        if value.trim().is_empty() || value.len() > 4096 || value.contains('\0') {
            return Err(ScienceError::Invalid(format!(
                "dossier {name} must be 1..=4096 bytes without NUL"
            )));
        }
    }
    Ok(())
}

fn remaining_metadata_bytes(consumed: usize) -> Result<u64> {
    MAX_SOURCE_METADATA_BYTES
        .checked_sub(consumed)
        .map(|remaining| remaining as u64)
        .ok_or_else(|| {
            ScienceError::Invalid(format!(
                "dossier source metadata exceeds the {MAX_SOURCE_METADATA_BYTES}-byte cap"
            ))
        })
}

fn charge_metadata<T: Serialize>(consumed: &mut usize, value: &T) -> Result<()> {
    let bytes = serde_json::to_vec(value)?.len();
    *consumed = consumed
        .checked_add(bytes)
        .ok_or_else(|| ScienceError::Invalid("dossier metadata size overflow".into()))?;
    if *consumed > MAX_SOURCE_METADATA_BYTES {
        return Err(ScienceError::Invalid(format!(
            "dossier source metadata exceeds the {MAX_SOURCE_METADATA_BYTES}-byte cap"
        )));
    }
    Ok(())
}

fn validate_source_text(
    artifacts: &[Artifact],
    evidence: &[Evidence],
    provenance: &[Provenance],
) -> Result<()> {
    let valid = |value: &str| value.len() <= MAX_TEXT_FIELD_BYTES && !value.contains('\0');
    if artifacts
        .iter()
        .any(|item| !valid(&item.mime) || !valid(&item.preview))
        || evidence
            .iter()
            .any(|item| !valid(&item.claim) || !valid(&item.source))
        || provenance.iter().any(|item| {
            !valid(&item.source_uri)
                || item
                    .source_commit
                    .as_deref()
                    .is_some_and(|value| !valid(value))
                || item
                    .source_path
                    .as_deref()
                    .is_some_and(|value| !valid(value))
                || !valid(&item.license)
                || !valid(&item.input_sha256)
                || !valid(&item.tool)
                || item
                    .environment
                    .iter()
                    .any(|(key, value)| !valid(key) || !valid(value))
        })
    {
        return Err(ScienceError::Invalid(format!(
            "dossier source text fields must be at most {MAX_TEXT_FIELD_BYTES} bytes without NUL"
        )));
    }
    Ok(())
}

fn bundled_path(source_index: usize, artifact_index: usize, original: &Path) -> PathBuf {
    let extension = original
        .extension()
        .and_then(|value| value.to_str())
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 12
                && value.bytes().all(|byte| byte.is_ascii_alphanumeric())
        })
        .unwrap_or("bin");
    PathBuf::from(format!(
        "source_{:03}_artifact_{:03}.{extension}",
        source_index + 1,
        artifact_index + 1
    ))
}

fn render_markdown(manifest: &DossierManifest) -> String {
    let mut out = format!(
        "# {}\n\n## Research question\n\n{}\n\n## Evidence inventory\n\n",
        markdown_inline_text(&manifest.title),
        markdown_inline_text(&manifest.research_question)
    );
    for source in &manifest.sources {
        out.push_str(&format!(
            "### Run `{}` ({})\n\n",
            source.run_id.0,
            markdown_inline_text(&source.tool_profile)
        ));
        for artifact in &source.artifacts {
            out.push_str(&format!(
                "- `{}` → `{}` — SHA-256 `{}`, {} bytes\n",
                markdown_code_text(&artifact.source_relative_path.display().to_string()),
                markdown_code_text(&artifact.bundled_relative_path.display().to_string()),
                artifact.sha256,
                artifact.bytes
            ));
        }
        out.push('\n');
    }
    out.push_str("## Limitations\n\n");
    for limitation in &manifest.limitations {
        out.push_str(&format!("- {}\n", markdown_inline_text(limitation)));
    }
    out
}

fn markdown_inline_text(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\n' | '\r' | '\t' => escaped.push(' '),
            character if character.is_control() => escaped.push(' '),
            '\\' | '`' | '*' | '_' | '{' | '}' | '[' | ']' | '<' | '>' | '(' | ')' | '#' | '+'
            | '-' | '.' | '!' | '|' => {
                escaped.push('\\');
                escaped.push(character);
            }
            character => escaped.push(character),
        }
    }
    escaped
}

fn markdown_code_text(value: &str) -> String {
    value
        .chars()
        .map(|character| match character {
            '\n' | '\r' | '\t' => ' ',
            '<' => '‹',
            '>' => '›',
            '`' => 'ˋ',
            character if character.is_control() => ' ',
            character => character,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn context(root: &Path, run: &str, project: &str, owner: &str, session: &str) -> RunContext {
        RunContext {
            run_id: RunId::new(run),
            project_id: ProjectId::new(project),
            session_id: session.into(),
            owner_id: owner.into(),
            workspace_root: root.to_path_buf(),
            provider: "offline-deterministic".into(),
            approval_policy: "test".into(),
            tool_profile: "science-connector-v1".into(),
            artifact_root: root.to_path_buf(),
            environment: BTreeMap::from([("network".into(), "disabled".into())]),
        }
    }

    fn source_run(store: &ScienceStore, context: RunContext, bytes: &[u8]) -> RunId {
        source_run_at(store, context, Path::new("response.json"), bytes)
    }

    fn source_run_at(
        store: &ScienceStore,
        context: RunContext,
        relative_path: &Path,
        bytes: &[u8],
    ) -> RunId {
        let ticket = ScienceRunTicket {
            project_id: context.project_id.clone(),
            run_id: context.run_id.clone(),
            owner_id: context.owner_id.clone(),
            call_id: CallId::new("source"),
        };
        store.create_run(context).unwrap();
        store
            .request_approval(Approval {
                project_id: ticket.project_id.clone(),
                run_id: ticket.run_id.clone(),
                call_id: ticket.call_id.clone(),
                owner_id: ticket.owner_id.clone(),
                decision: ApprovalDecision::Pending,
                decided_at: None,
            })
            .unwrap();
        store
            .transition(&ticket.run_id, RunState::AwaitingApproval, None)
            .unwrap();
        crate::csv::mark_allowed(store, &ticket).unwrap();
        let artifact = store
            .put_artifact(
                &ticket.project_id,
                &ticket.run_id,
                &ticket.owner_id,
                ticket.call_id,
                relative_path,
                bytes,
                "application/json",
                "records",
            )
            .unwrap();
        store
            .add_evidence(Evidence {
                run_id: ticket.run_id.clone(),
                claim: "source record".into(),
                source: "https://example.invalid/record".into(),
                artifact_sha256: Some(artifact.sha256.clone()),
                verified_at: Utc::now(),
            })
            .unwrap();
        store
            .add_provenance(Provenance {
                run_id: ticket.run_id.clone(),
                source_uri: "https://example.invalid/record".into(),
                source_commit: None,
                source_path: None,
                license: "CC0-1.0".into(),
                retrieved_at: Utc::now(),
                input_sha256: artifact.sha256,
                tool: "test-source".into(),
                environment: BTreeMap::new(),
            })
            .unwrap();
        store.transition_succeeded_verified(&ticket.run_id).unwrap();
        ticket.run_id
    }

    fn allowed_dossier(
        store: &ScienceStore,
        context: RunContext,
        source_run_ids: Vec<RunId>,
        project_revision: String,
        title: &str,
        research_question: &str,
    ) -> (ScienceRunTicket, DossierAdmission) {
        let source_snapshots = capture_source_snapshots(store, &source_run_ids).unwrap();
        let admission = DossierAdmission::new(
            &context,
            source_snapshots,
            project_revision,
            title.into(),
            research_question.into(),
            "SessionActor/evidence-dossier-v1".into(),
        )
        .unwrap();
        let ticket = begin_dossier(store, context, &admission).unwrap();
        crate::csv::mark_allowed(store, &ticket).unwrap();
        (ticket, admission)
    }

    #[test]
    fn dossier_copies_verified_bytes_and_builds_manifest() {
        let temp = TempDir::new().unwrap();
        let store = ScienceStore::new(temp.path());
        let source = source_run(
            &store,
            context(temp.path(), "source-1", "project-1", "owner-1", "session-1"),
            br#"{"records":[{"id":"PMID-1"}]}"#,
        );
        let (ticket, admission) = allowed_dossier(
            &store,
            context(
                temp.path(),
                "dossier-1",
                "project-1",
                "owner-1",
                "session-1",
            ),
            vec![source],
            "a".repeat(64),
            "Biomedical evidence",
            "What evidence supports target X?",
        );
        let result = finish_dossier(&store, ticket, admission).unwrap();

        assert_eq!(result.run.state, RunState::Succeeded);
        assert_eq!(
            result
                .run
                .context
                .environment
                .get(DOSSIER_ADMISSION_ENV_KEY),
            Some(&result.manifest.admission_sha256)
        );
        assert_eq!(result.manifest.sources.len(), 1);
        assert_eq!(result.artifacts.len(), 3);
        for artifact in &result.artifacts {
            assert!(
                result.evidence.iter().any(|evidence| {
                    evidence.artifact_sha256.as_deref() == Some(&artifact.sha256)
                }),
                "artifact {} lacks evidence binding",
                artifact.relative_path.display()
            );
            assert!(
                result
                    .provenance
                    .iter()
                    .any(|provenance| provenance.input_sha256 == artifact.sha256),
                "artifact {} lacks provenance binding",
                artifact.relative_path.display()
            );
        }
        let bundled = &result.manifest.sources[0].artifacts[0];
        assert_eq!(
            store
                .artifact_bytes(
                    &result.run.context.project_id,
                    &result.run.context.run_id,
                    &result.run.context.owner_id,
                    &bundled.bundled_relative_path,
                )
                .unwrap(),
            br#"{"records":[{"id":"PMID-1"}]}"#
        );
    }

    #[test]
    fn dossier_rejects_cross_session_source_and_leaves_no_outputs() {
        let temp = TempDir::new().unwrap();
        let store = ScienceStore::new(temp.path());
        let source = source_run(
            &store,
            context(
                temp.path(),
                "source-1",
                "project-1",
                "owner-1",
                "session-other",
            ),
            br#"{"records":[]}"#,
        );
        let (ticket, admission) = allowed_dossier(
            &store,
            context(
                temp.path(),
                "dossier-1",
                "project-1",
                "owner-1",
                "session-1",
            ),
            vec![source],
            "b".repeat(64),
            "Biomedical evidence",
            "Question?",
        );
        let dossier_run = ticket.run_id.clone();
        let error = finish_dossier(&store, ticket, admission).unwrap_err();

        assert!(matches!(error, ScienceError::Invalid(_)));
        assert_eq!(
            store.load_run(&dossier_run).unwrap().state,
            RunState::Failed
        );
        assert!(store.artifacts(&dossier_run).unwrap().is_empty());
    }

    #[test]
    fn dossier_rejects_tampered_source_bytes_and_leaves_no_outputs() {
        let temp = TempDir::new().unwrap();
        let store = ScienceStore::new(temp.path());
        let source = source_run(
            &store,
            context(temp.path(), "source-1", "project-1", "owner-1", "session-1"),
            br#"{"records":[1]}"#,
        );
        std::fs::write(
            temp.path().join("runs/source-1/artifacts/response.json"),
            br#"{"records":[2]}"#,
        )
        .unwrap();
        let (ticket, admission) = allowed_dossier(
            &store,
            context(
                temp.path(),
                "dossier-1",
                "project-1",
                "owner-1",
                "session-1",
            ),
            vec![source],
            "c".repeat(64),
            "Biomedical evidence",
            "Question?",
        );
        let dossier_run = ticket.run_id.clone();
        let error = finish_dossier(&store, ticket, admission).unwrap_err();

        assert!(error.to_string().contains("hash/length"));
        assert_eq!(
            store.load_run(&dossier_run).unwrap().state,
            RunState::Failed
        );
        assert!(store.artifacts(&dossier_run).unwrap().is_empty());
    }

    #[test]
    fn dossier_rejects_unbound_source_artifact() {
        let temp = TempDir::new().unwrap();
        let store = ScienceStore::new(temp.path());
        let source = source_run(
            &store,
            context(temp.path(), "source-1", "project-1", "owner-1", "session-1"),
            br#"{"records":[1]}"#,
        );
        std::fs::write(temp.path().join("runs/source-1/provenance.json"), b"[]").unwrap();
        let (ticket, admission) = allowed_dossier(
            &store,
            context(
                temp.path(),
                "dossier-1",
                "project-1",
                "owner-1",
                "session-1",
            ),
            vec![source],
            "d".repeat(64),
            "Biomedical evidence",
            "Question?",
        );
        let dossier_run = ticket.run_id.clone();
        let error = finish_dossier(&store, ticket, admission).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("artifact, evidence, and provenance")
        );
        assert_eq!(
            store.load_run(&dossier_run).unwrap().state,
            RunState::Failed
        );
        assert!(store.artifacts(&dossier_run).unwrap().is_empty());
    }

    #[test]
    fn dossier_rejects_source_registry_changed_after_admission() {
        let temp = TempDir::new().unwrap();
        let store = ScienceStore::new(temp.path());
        let source = source_run(
            &store,
            context(temp.path(), "source-1", "project-1", "owner-1", "session-1"),
            br#"{"records":[1]}"#,
        );
        let (ticket, admission) = allowed_dossier(
            &store,
            context(
                temp.path(),
                "dossier-1",
                "project-1",
                "owner-1",
                "session-1",
            ),
            vec![source.clone()],
            "1".repeat(64),
            "Biomedical evidence",
            "Question?",
        );
        let dossier_run = ticket.run_id.clone();
        let mut evidence = store.evidence(&source).unwrap();
        evidence.push(Evidence {
            run_id: source,
            claim: "not part of the approved snapshot".into(),
            source: "https://example.invalid/late".into(),
            artifact_sha256: None,
            verified_at: Utc::now(),
        });
        std::fs::write(
            temp.path().join("runs/source-1/evidence.json"),
            serde_json::to_vec_pretty(&evidence).unwrap(),
        )
        .unwrap();

        let error = finish_dossier(&store, ticket, admission).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("changed after dossier admission")
        );
        assert_eq!(
            store.load_run(&dossier_run).unwrap().state,
            RunState::Failed
        );
        assert!(store.artifacts(&dossier_run).unwrap().is_empty());
        assert!(store.evidence(&dossier_run).unwrap().is_empty());
        assert!(store.provenance(&dossier_run).unwrap().is_empty());
    }

    #[test]
    fn late_publish_failure_rolls_back_every_registered_output() {
        let temp = TempDir::new().unwrap();
        let store = ScienceStore::new(temp.path());
        let source = source_run(
            &store,
            context(temp.path(), "source-1", "project-1", "owner-1", "session-1"),
            br#"{"records":[1]}"#,
        );
        let (ticket, admission) = allowed_dossier(
            &store,
            context(
                temp.path(),
                "dossier-1",
                "project-1",
                "owner-1",
                "session-1",
            ),
            vec![source],
            "e".repeat(64),
            "Biomedical evidence",
            "Question?",
        );
        let dossier_run = ticket.run_id.clone();
        std::fs::write(
            temp.path().join("runs/dossier-1/artifacts/dossier.md"),
            b"pre-existing unregistered file",
        )
        .unwrap();
        let error = finish_dossier(&store, ticket, admission).unwrap_err();

        assert!(error.to_string().contains("failed closed"));
        assert_eq!(
            store.load_run(&dossier_run).unwrap().state,
            RunState::Failed
        );
        assert!(store.artifacts(&dossier_run).unwrap().is_empty());
        assert!(
            !temp
                .path()
                .join("runs/dossier-1/artifacts/source_001_artifact_001.json")
                .exists()
        );
        assert!(
            !temp
                .path()
                .join("runs/dossier-1/artifacts/dossier.json")
                .exists()
        );
    }

    #[test]
    fn markdown_report_neutralizes_untrusted_source_path_markup() {
        let temp = TempDir::new().unwrap();
        let store = ScienceStore::new(temp.path());
        let mut source_context =
            context(temp.path(), "source-1", "project-1", "owner-1", "session-1");
        source_context.tool_profile =
            "tool ![profile](https://example.invalid/tool) # heading".into();
        let source = source_run_at(
            &store,
            source_context,
            Path::new("evil`\n![remote](pixel).json"),
            br#"{"records":[1]}"#,
        );
        let (ticket, admission) = allowed_dossier(
            &store,
            context(
                temp.path(),
                "dossier-1",
                "project-1",
                "owner-1",
                "session-1",
            ),
            vec![source],
            "f".repeat(64),
            "Biomedical ![title](https://example.invalid/title) # injected",
            "[question](https://example.invalid/question) > quote",
        );
        let result = finish_dossier(&store, ticket, admission).unwrap();
        let markdown = String::from_utf8(
            store
                .artifact_bytes(
                    &result.run.context.project_id,
                    &result.run.context.run_id,
                    &result.run.context.owner_id,
                    Path::new("dossier.md"),
                )
                .unwrap(),
        )
        .unwrap();

        assert!(!markdown.contains("evil`"));
        assert!(!markdown.contains("\n![remote]"));
        assert!(markdown.contains("evilˋ ![remote](pixel).json"));
        assert!(!markdown.contains("![title]("));
        assert!(!markdown.contains("[question]("));
        assert!(!markdown.contains("![profile]("));
        assert!(markdown.contains(r"\!\[title\]\(https://example\.invalid/title\)"));
        assert!(markdown.contains(r"\[question\]\(https://example\.invalid/question\)"));
        assert!(markdown.contains(r"\!\[profile\]\(https://example\.invalid/tool\)"));
    }
}
