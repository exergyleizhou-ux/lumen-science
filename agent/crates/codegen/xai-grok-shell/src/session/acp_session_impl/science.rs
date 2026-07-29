//! Lumen Science product dispatch. Seam contract: S2 and S4.

use super::*;
use crate::session::commands::{
    PreparedScienceCsv, PreparedScienceEvidenceDossier, PreparedScienceFetch,
    PreparedScienceImport, PreparedScienceKernelAdmission, PreparedScienceProjectMutation,
    PreparedScienceSeqAnalyze, PreparedScienceSkillQuarantine, PreparedScienceSshScpAdmission,
    PreparedScienceWorkflowExecution, ScienceSeqOperationLease,
};
use sha2::Digest as _;

/// Fetch transit tool: copies each staged input to its staged output. The
/// kernel re-parses the output bytes as connector responses, so a fetch is
/// recorded only when the formal tool path preserved every exchange.
const FETCH_TOOL_SCRIPT: &str = r#"import sys
from pathlib import Path
args = sys.argv[1:]
for index in range(0, len(args), 2):
    Path(args[index + 1]).write_bytes(Path(args[index]).read_bytes())
"#;

/// Import transit tool: copies staged input bytes to a staged output. The
/// kernel then re-derives the preview from the output bytes, so the artifact
/// is registered only when the formal tool path preserved the input exactly.
const IMPORT_TOOL_SCRIPT: &str = r#"import sys
from pathlib import Path
source, target = map(Path, sys.argv[1:3])
target.write_bytes(source.read_bytes())
"#;

const WORKFLOW_ADMISSION_SHA256_ENV: &str = "workflow_admission_sha256";
const WORKFLOW_OPERATION_ID_ENV: &str = "workflow_operation_id";
const SEQ_UNDELIVERED_BEGIN_REASON: &str =
    "sequence analysis Begin response receiver closed before delivery";

fn interrupt_undelivered_seq_authority(
    store: &xai_grok_science::ScienceStore,
    ticket: &xai_grok_science::csv::ScienceRunTicket,
) -> xai_grok_science::Result<()> {
    let terminal = xai_grok_science::seqbench::finish_without_execution_recoverable(
        store,
        ticket,
        xai_grok_science::ApprovalDecision::Interrupted,
        SEQ_UNDELIVERED_BEGIN_REASON,
    )?;
    if terminal.state != xai_grok_science::RunState::Interrupted
        || terminal.terminal_reason.as_deref() != Some(SEQ_UNDELIVERED_BEGIN_REASON)
    {
        return Err(xai_grok_science::ScienceError::Invalid(
            "undelivered sequence analysis Begin did not reach exact Interrupted terminal".into(),
        ));
    }
    Ok(())
}

const CSV_TOOL_SCRIPT: &str = r#"import csv, html, sys
from collections import defaultdict
from pathlib import Path

source, summary_path, svg_path = map(Path, sys.argv[1:4])
groups = defaultdict(list)
with source.open(newline='', encoding='utf-8') as handle:
    reader = csv.DictReader(handle)
    if reader.fieldnames != ['sample_id', 'condition', 'value']:
        raise SystemExit('unexpected CSV header')
    for row in reader:
        groups[row['condition']].append(float(row['value']))
if not groups:
    raise SystemExit('CSV has no rows')
rows = []
bars = []
for index, name in enumerate(sorted(groups)):
    values = groups[name]
    mean = sum(values) / len(values)
    rows.append(f'{name},{len(values)},{mean:.3f}')
    x = 30 + index * 90
    height = max(0, min(160, round(mean * 10)))
    y = 180 - height
    escaped = html.escape(name, quote=True)
    bars.append(f'<rect x="{x}" y="{y}" width="50" height="{height}"/><text x="{x}" y="198">{escaped}</text>')
summary_path.write_text('condition,count,mean\n' + '\n'.join(rows) + '\n', encoding='utf-8')
svg_path.write_text('<svg xmlns="http://www.w3.org/2000/svg" width="400" height="210" viewBox="0 0 400 210"><title>Condition means</title>' + ''.join(bars) + '</svg>\n', encoding='utf-8')
"#;

fn quote(value: &str) -> xai_grok_science::Result<String> {
    shlex::try_quote(value)
        .map(|quoted| quoted.into_owned())
        .map_err(|_| xai_grok_science::ScienceError::Invalid("NUL in science tool path".into()))
}

fn validate_project_mutation_actor_roots(
    actor_workspace: &std::path::Path,
    store: &xai_grok_science::ScienceStore,
    project_root: &std::path::Path,
    context: &xai_grok_science::RunContext,
) -> xai_grok_science::Result<()> {
    let actor_workspace = dunce::canonicalize(actor_workspace)?;
    let project_root_canonical = dunce::canonicalize(project_root)?;
    let store_root = dunce::canonicalize(store.root())?;
    let artifact_root = dunce::canonicalize(&context.artifact_root)?;
    if context.workspace_root != actor_workspace
        || project_root != project_root_canonical
        || project_root_canonical != store_root
        || !project_root_canonical.starts_with(&actor_workspace)
        || context.artifact_root != artifact_root
        || artifact_root != project_root_canonical.join("runs")
    {
        return Err(xai_grok_science::ScienceError::Invalid(
            "project mutation store or paths do not belong to this SessionActor workspace".into(),
        ));
    }
    Ok(())
}

fn finish_replayed_project_mutation(
    prepared: &PreparedScienceProjectMutation,
    outcome: &xai_grok_science::project::MutationOutcome,
    decision: xai_grok_science::ApprovalDecision,
) -> xai_grok_science::Result<xai_grok_science::project::MutationOutcome> {
    if decision != xai_grok_science::ApprovalDecision::Allow
        || prepared.permission_grant.is_some()
        || prepared.resume_allowed
    {
        return Err(xai_grok_science::ScienceError::Invalid(
            "project mutation replay cannot carry a fresh decision or permission grant".into(),
        ));
    }
    match &prepared.request.mutation {
        xai_grok_science::project::ProjectMutation::ProjectMigrate { .. } => {
            verify_migration_replay(
                &prepared.store,
                &prepared.project_store,
                &prepared.request,
                outcome,
            )?;
        }
        xai_grok_science::project::ProjectMutation::ReviewRecord { .. } => {
            commit_review_authority_success(
                &prepared.store,
                &prepared.project_store,
                &prepared.ticket,
                &prepared.expected_context,
                &prepared.request,
                outcome,
            )?;
        }
        _ => {}
    }
    Ok(outcome.clone())
}

fn validate_kernel_admission_actor_roots(
    actor_workspace: &std::path::Path,
    store: &xai_grok_science::ScienceStore,
    project_root: &std::path::Path,
    context: &xai_grok_science::RunContext,
) -> xai_grok_science::Result<std::path::PathBuf> {
    validate_project_mutation_actor_roots(actor_workspace, store, project_root, context)?;
    if context.project_id.0.trim().is_empty() || context.owner_id.trim().is_empty() {
        return Err(xai_grok_science::ScienceError::Invalid(
            "kernel admission requires project and owner ids".into(),
        ));
    }
    Ok(dunce::canonicalize(actor_workspace)?)
}

fn validate_kernel_project_binding(
    project: &xai_grok_science::project::ResearchProject,
    context: &xai_grok_science::RunContext,
) -> xai_grok_science::Result<()> {
    if project.project_id.0 != context.project_id.0 {
        return Err(xai_grok_science::ScienceError::Invalid(
            "kernel admission project does not match its run context".into(),
        ));
    }
    if project.owner_id.0 != context.owner_id {
        return Err(xai_grok_science::ScienceError::Ownership);
    }
    Ok(())
}

fn validate_kernel_session_binding(
    actor_session: &str,
    context: &xai_grok_science::RunContext,
) -> xai_grok_science::Result<()> {
    if context.session_id != actor_session {
        return Err(xai_grok_science::ScienceError::Invalid(
            "kernel admission session does not match this SessionActor".into(),
        ));
    }
    Ok(())
}

impl SessionActor {
    /// P5 completion remains inside the sole actor. A successful response is
    /// not returned until Goal and Expert snapshots have crossed the
    /// persistence queue's durability barrier.
    pub(super) async fn verify_science_goal(
        &self,
        store: xai_grok_science::ScienceStore,
        run_id: xai_grok_science::RunId,
    ) -> Result<
        xai_grok_science::review::HostVerificationReport,
        crate::session::science_goal::ScienceGoalReviewError,
    > {
        let current_session_tokens = self.chat_state_handle.get_total_tokens().await as i64;
        let (report, expert_snapshot) = {
            let mut state = self.state.lock().await;
            let mut goal = self.goal_tracker.lock();
            let review = crate::session::science_goal::ScienceGoalReview::bind(
                &goal,
                &state.expert,
                run_id,
            )?;
            let report = review.host_verify_and_complete(&mut goal, &mut state.expert, &store)?;
            (report, state.expert.clone())
        };

        let (tokens_used, finished_marginal) = self.goal_tokens(current_session_tokens);
        self.goal_notify_sender().emit_goal_updated(
            &mut self.goal_tracker.lock(),
            tokens_used,
            finished_marginal,
        );
        let goal_snapshot =
            self.goal_tracker.lock().snapshot().cloned().ok_or(
                crate::session::science_goal::ScienceGoalReviewError::GoalCompletionRejected,
            )?;
        self.notifications
            .persistence_tx
            .send(PersistenceMsg::GoalModeState(goal_snapshot))
            .map_err(|_| {
                crate::session::science_goal::ScienceGoalReviewError::AuditPersistenceFailed
            })?;
        self.notifications
            .persistence_tx
            .send(PersistenceMsg::ExpertModeState(expert_snapshot))
            .map_err(|_| {
                crate::session::science_goal::ScienceGoalReviewError::AuditPersistenceFailed
            })?;
        let (respond_to, response) = tokio::sync::oneshot::channel();
        self.notifications
            .persistence_tx
            .send(PersistenceMsg::FlushAndAck { respond_to })
            .map_err(|_| {
                crate::session::science_goal::ScienceGoalReviewError::AuditPersistenceFailed
            })?;
        response.await.map_err(|_| {
            crate::session::science_goal::ScienceGoalReviewError::AuditPersistenceFailed
        })?;
        Ok(report)
    }

    /// P4 admission runs inside the sole Lumen session actor. It is called
    /// before the handle asks the existing permission manager, and the Science
    /// crate itself performs no I/O outside its durable local store.
    pub(super) fn prepare_science_ssh_scp_admission(
        &self,
        store: xai_grok_science::ScienceStore,
        context: xai_grok_science::RunContext,
        policy: xai_grok_science::connector::ConnectorPolicy,
        request: xai_grok_science::connector::ConnectorRequest,
    ) -> xai_grok_science::Result<Option<PreparedScienceSshScpAdmission>> {
        match xai_grok_science::connector::start_ssh_scp_admission(
            &store, context, &policy, &request,
        )? {
            xai_grok_science::connector::AdmissionStart::Ready(ticket) => {
                Ok(Some(PreparedScienceSshScpAdmission {
                    store,
                    ticket: *ticket,
                }))
            }
            xai_grok_science::connector::AdmissionStart::Denied => Ok(None),
        }
    }

    pub(super) fn finish_science_ssh_scp_admission(
        &self,
        prepared: PreparedScienceSshScpAdmission,
        decision: xai_grok_science::ApprovalDecision,
    ) -> xai_grok_science::Result<Option<xai_grok_science::connector::AdmissionTicket>> {
        xai_grok_science::connector::finish_ssh_scp_admission(
            &prepared.store,
            prepared.ticket,
            decision,
        )
    }

    pub(super) fn execute_science_ssh_scp_offline_transport(
        &self,
        store: xai_grok_science::ScienceStore,
        ticket: xai_grok_science::connector::AdmissionTicket,
        outcome: xai_grok_science::connector::OfflineTransportOutcome,
    ) -> xai_grok_science::Result<xai_grok_science::connector::OfflineTransportReceipt> {
        xai_grok_science::connector::execute_offline_transport(&store, ticket, outcome)
    }

    pub(super) fn execute_science_ssh_scp_transport(
        &self,
        store: xai_grok_science::ScienceStore,
        ticket: xai_grok_science::connector::AdmissionTicket,
        operation: xai_grok_science::transport::ScpOperation,
        config: xai_grok_science::transport::ScpExecutionConfig,
    ) -> xai_grok_science::Result<xai_grok_science::transport::TransportReceipt> {
        xai_grok_science::transport::execute_scp(&store, ticket, operation, &config)
    }

    pub(super) fn prepare_science_csv(
        &self,
        store: xai_grok_science::ScienceStore,
        context: xai_grok_science::RunContext,
        fixture_path: std::path::PathBuf,
        fixture: Vec<u8>,
    ) -> xai_grok_science::Result<PreparedScienceCsv> {
        let ticket = xai_grok_science::csv::begin_fixture(&store, context.clone())?;
        let staging = context
            .artifact_root
            .join(&ticket.run_id.0)
            .join("tool-staging");
        std::fs::create_dir_all(&staging)?;
        let input_path = staging.join("input.csv");
        let summary_path = staging.join("summary.csv");
        let svg_path = staging.join("means.svg");
        std::fs::write(&input_path, &fixture)?;
        let command = format!(
            "python3 -c {} {} {} {}",
            quote(CSV_TOOL_SCRIPT)?,
            quote(&input_path.to_string_lossy())?,
            quote(&summary_path.to_string_lossy())?,
            quote(&svg_path.to_string_lossy())?,
        );
        Ok(PreparedScienceCsv {
            store,
            ticket,
            fixture_path,
            fixture,
            command,
            summary_path,
            svg_path,
        })
    }

    /// S2 phase one inside the sole session actor: begin the durable import
    /// run and stage the bytes for the formal execute-tool transit.
    pub(super) fn prepare_science_import(
        &self,
        store: xai_grok_science::ScienceStore,
        context: xai_grok_science::RunContext,
        source_path: std::path::PathBuf,
        bytes: Vec<u8>,
    ) -> xai_grok_science::Result<PreparedScienceImport> {
        let ticket = xai_grok_science::import::begin_import(&store, context.clone())?;
        let staging = context
            .artifact_root
            .join(&ticket.run_id.0)
            .join("tool-staging");
        std::fs::create_dir_all(&staging)?;
        let input_path = staging.join("input.bin");
        let output_path = staging.join("output.bin");
        std::fs::write(&input_path, &bytes)?;
        let command = format!(
            "python3 -c {} {} {}",
            quote(IMPORT_TOOL_SCRIPT)?,
            quote(&input_path.to_string_lossy())?,
            quote(&output_path.to_string_lossy())?,
        );
        Ok(PreparedScienceImport {
            store,
            ticket,
            source_path,
            bytes,
            command,
            output_path,
        })
    }

    /// Admit deterministic sequence analysis inside the sole SessionActor.
    /// The adapter may resolve and read a confined source, but it cannot open
    /// a durable run or commit output. Those authorities start here.
    pub(super) fn prepare_science_seq_analyze(
        &self,
        store: xai_grok_science::ScienceStore,
        mut context: xai_grok_science::RunContext,
        options: xai_grok_science::seqbench::SeqAnalyzeOptions,
        source_path: std::path::PathBuf,
        source_bytes: Vec<u8>,
    ) -> xai_grok_science::Result<PreparedScienceSeqAnalyze> {
        let actor_session = self.session_info.id.0.as_ref();
        let actor_workspace = dunce::canonicalize(&self.session_info.cwd)?;
        let canonical_source = dunce::canonicalize(&source_path)?;
        if context.session_id != actor_session {
            return Err(xai_grok_science::ScienceError::Invalid(
                "sequence analysis session does not match this SessionActor".into(),
            ));
        }
        if context.workspace_root != actor_workspace
            || canonical_source != source_path
            || !canonical_source.starts_with(&actor_workspace)
            || !canonical_source.is_file()
            || context.artifact_root != actor_workspace.join("science-store")
            || dunce::canonicalize(store.root())? != context.artifact_root
        {
            return Err(xai_grok_science::ScienceError::Invalid(
                "sequence analysis store or paths do not belong to this SessionActor workspace"
                    .into(),
            ));
        }
        self.science_feature_gates
            .require(xai_grok_science::features::ScienceFeature::ResearchProject)?;
        let project_store =
            xai_grok_science::project::ProjectStore::new_confined(store.root(), &actor_workspace)?
                .with_gates(self.science_feature_gates.clone());
        if !store.shares_root_capability_with(&project_store)? {
            return Err(xai_grok_science::ScienceError::Invalid(
                "sequence analysis project and ScienceStore pin different root identities".into(),
            ));
        }
        let project_id = xai_grok_science::project::ProjectId(context.project_id.0.clone());
        let current_project_revision = project_store.with_owned_project_revision(
            &project_id,
            &context.owner_id,
            |_project, revision| Ok(revision.to_owned()),
        )?;
        let operation_id = context
            .environment
            .get(xai_grok_science::seqbench::OPERATION_ENV)
            .cloned()
            .ok_or_else(|| {
                xai_grok_science::ScienceError::Invalid(
                    "sequence analysis operationId binding is missing".into(),
                )
            })?;
        xai_grok_science::seqbench::validate_operation_id(&operation_id)?;
        let source_relative = xai_grok_science::seqbench::source_relative_binding(
            &actor_workspace,
            &canonical_source,
        )?;
        let source_sha256 = xai_grok_science::seqbench::hex_sha256(&source_bytes);
        let request_sha256 =
            xai_grok_science::seqbench::request_sha256(&source_relative, &source_bytes, &options)?;
        if context.run_id != xai_grok_science::seqbench::operation_run_id(&operation_id)
            || context.provider != "offline-deterministic"
            || context.approval_policy != "production-session-permission"
            || context.tool_profile != "science-seqbench-v4"
            || context.environment.get("network").map(String::as_str) != Some("disabled")
            || context.environment.get("locale").map(String::as_str) != Some("C")
            || context
                .environment
                .get(xai_grok_science::seqbench::SOURCE_RELATIVE_PATH_ENV)
                != Some(&source_relative)
            || context
                .environment
                .get(xai_grok_science::seqbench::SOURCE_SHA256_ENV)
                != Some(&source_sha256)
            || context
                .environment
                .get(xai_grok_science::seqbench::SOURCE_BYTES_ENV)
                != Some(&source_bytes.len().to_string())
            || context
                .environment
                .get(xai_grok_science::seqbench::REQUEST_SHA256_ENV)
                != Some(&request_sha256)
        {
            return Err(xai_grok_science::ScienceError::Invalid(
                "sequence analysis request bindings were not derived by this SessionActor".into(),
            ));
        }
        context.environment.insert(
            xai_grok_science::seqbench::PROJECT_REVISION_ENV.into(),
            current_project_revision.clone(),
        );
        let mut expected_context = context.clone();
        expected_context.environment.insert(
            "translation_table_id".into(),
            options.translation_table_id.to_string(),
        );
        expected_context.environment.insert(
            "translation_table_name".into(),
            xai_grok_science::seqbench::translation_table_name(options.translation_table_id)
                .ok_or_else(|| {
                    xai_grok_science::ScienceError::Invalid(
                        "unsupported sequence translation table".into(),
                    )
                })?
                .into(),
        );
        expected_context.environment.insert(
            "restriction_topology".into(),
            options.topology.as_str().into(),
        );
        expected_context.environment.insert(
            "restriction_digest_enzymes".into(),
            options.restriction_digest_enzymes.join(","),
        );
        let operation_lease = ScienceSeqOperationLease::claim(&store, &expected_context.run_id)
            .map_err(|error| match error {
                xai_grok_science::ScienceError::Invalid(message)
                    if message.contains("already active") =>
                {
                    xai_grok_science::ScienceError::Invalid(format!(
                        "sequence operation {operation_id} ({}) is already active: {message}",
                        expected_context.run_id.0
                    ))
                }
                error => error,
            })?;
        let admission = xai_grok_science::seqbench::replay_or_recover_existing(
            &store,
            &expected_context,
            &source_path,
            &source_bytes,
            &options,
        )?;
        let (
            ticket,
            replayed,
            recovery_grant,
            allowed_witness,
            project_revision,
            expected_context,
            created_event,
            created_event_from_recovery,
        ) = match admission {
            xai_grok_science::seqbench::SeqAnalyzeAdmission::New => {
                let (ticket, created_event) =
                    xai_grok_science::seqbench::begin_analysis_with_options_witnessed(
                        &store, context, &options,
                    )?;
                if store.load_run(&ticket.run_id)?.context != expected_context {
                    let _ = store.recover_interrupted(&ticket.run_id);
                    return Err(xai_grok_science::ScienceError::Invalid(
                        "sequence analysis durable Begin context did not match actor admission"
                            .into(),
                    ));
                }
                (
                    ticket,
                    None,
                    None,
                    None,
                    current_project_revision,
                    expected_context,
                    created_event,
                    false,
                )
            }
            xai_grok_science::seqbench::SeqAnalyzeAdmission::AwaitingApproval(ticket) => {
                let durable = store.load_run(&ticket.run_id)?;
                let durable_revision = durable
                    .context
                    .environment
                    .get(xai_grok_science::seqbench::PROJECT_REVISION_ENV)
                    .filter(|revision| !revision.is_empty())
                    .cloned()
                    .ok_or_else(|| {
                        xai_grok_science::ScienceError::Invalid(
                            "sequence analysis durable project revision is missing".into(),
                        )
                    })?;
                if durable_revision != current_project_revision {
                    let reason =
                        "project changed before sequence approval could be resumed after restart";
                    let terminal =
                        xai_grok_science::seqbench::finish_without_execution_recoverable(
                            &store,
                            &ticket,
                            xai_grok_science::ApprovalDecision::Interrupted,
                            reason,
                        )?;
                    return Err(xai_grok_science::ScienceError::Invalid(format!(
                        "science run {} finished {:?}: {reason}",
                        ticket.run_id.0, terminal.state
                    )));
                }
                let created_event = store
                    .events_after(&ticket.run_id, 0, 1_000)?
                    .into_iter()
                    .next()
                    .ok_or_else(|| {
                        xai_grok_science::ScienceError::Invalid(
                            "recovered sequence Begin lost its created event".into(),
                        )
                    })?;
                (
                    ticket,
                    None,
                    None,
                    None,
                    durable_revision,
                    durable.context,
                    created_event,
                    true,
                )
            }
            xai_grok_science::seqbench::SeqAnalyzeAdmission::ResumeAllowed(ticket) => {
                let durable = store.load_run(&ticket.run_id)?;
                let durable_revision = durable
                    .context
                    .environment
                    .get(xai_grok_science::seqbench::PROJECT_REVISION_ENV)
                    .filter(|revision| !revision.is_empty())
                    .cloned()
                    .ok_or_else(|| {
                        xai_grok_science::ScienceError::Invalid(
                            "sequence recovery durable project revision is missing".into(),
                        )
                    })?;
                let grant = ScienceSeqAnalyzeRecoveryGrant::new(
                    &ticket,
                    &durable.context,
                    &durable_revision,
                    &source_path,
                    &source_bytes,
                    &options,
                );
                let allowed_witness =
                    xai_grok_science::seqbench::recover_allowed_witness(&store, &ticket)?;
                let created_event = store
                    .events_after(&ticket.run_id, 0, 1_000)?
                    .into_iter()
                    .next()
                    .ok_or_else(|| {
                        xai_grok_science::ScienceError::Invalid(
                            "recovered sequence Allow lost its created event".into(),
                        )
                    })?;
                (
                    ticket,
                    None,
                    Some(grant),
                    Some(allowed_witness),
                    durable_revision,
                    durable.context,
                    created_event,
                    true,
                )
            }
            xai_grok_science::seqbench::SeqAnalyzeAdmission::Replay(result) => {
                let durable_context = result.run.context.clone();
                let durable_revision = durable_context
                    .environment
                    .get(xai_grok_science::seqbench::PROJECT_REVISION_ENV)
                    .filter(|revision| !revision.is_empty())
                    .cloned()
                    .ok_or_else(|| {
                        xai_grok_science::ScienceError::Invalid(
                            "sequence replay durable project revision is missing".into(),
                        )
                    })?;
                let created_event = store
                    .events_after(&durable_context.run_id, 0, 1_000)?
                    .into_iter()
                    .next()
                    .ok_or_else(|| {
                        xai_grok_science::ScienceError::Invalid(
                            "replayed sequence authority lost its created event".into(),
                        )
                    })?;
                (
                    xai_grok_science::csv::ScienceRunTicket {
                        project_id: durable_context.project_id.clone(),
                        run_id: durable_context.run_id.clone(),
                        owner_id: durable_context.owner_id.clone(),
                        call_id: xai_grok_science::CallId::new("science_seq_analyze"),
                    },
                    Some(*result),
                    None,
                    None,
                    durable_revision,
                    durable_context,
                    created_event,
                    true,
                )
            }
        };
        let target = expected_context
            .artifact_root
            .join("runs")
            .join(&ticket.run_id.0)
            .join("artifacts")
            .display()
            .to_string();
        Ok(PreparedScienceSeqAnalyze {
            store,
            project_store,
            project_revision,
            expected_context,
            ticket,
            created_event,
            created_event_from_recovery,
            allowed_witness,
            options,
            source_path,
            source_bytes,
            target,
            replayed,
            recovery_grant,
            operation_lease,
        })
    }

    pub(super) fn finish_science_seq_analyze(
        &self,
        mut prepared: PreparedScienceSeqAnalyze,
        decision: xai_grok_science::ApprovalDecision,
        reason: String,
        permission_grant: Option<crate::session::handle::ScienceSeqAnalyzePermissionGrant>,
    ) -> xai_grok_science::Result<xai_grok_science::seqbench::SeqAnalyzeResult> {
        if prepared.replayed.is_some() {
            return Err(xai_grok_science::ScienceError::Invalid(
                "replayed sequence analysis cannot enter a fresh Finish".into(),
            ));
        }
        let durable = prepared.store.load_run(&prepared.ticket.run_id)?;
        if durable.context != prepared.expected_context
            || prepared.expected_context.run_id != prepared.ticket.run_id
            || prepared.expected_context.project_id != prepared.ticket.project_id
            || prepared.expected_context.owner_id != prepared.ticket.owner_id
            || !prepared
                .store
                .shares_root_capability_with(&prepared.project_store)?
        {
            return Err(xai_grok_science::ScienceError::Ownership);
        }
        let fresh_authorized = permission_grant
            .as_ref()
            .is_some_and(|grant| grant.authorizes(&prepared));
        let recovery_authorized = prepared
            .recovery_grant
            .as_ref()
            .is_some_and(|grant| grant.authorizes(&prepared));
        if decision == xai_grok_science::ApprovalDecision::Allow
            && (fresh_authorized == recovery_authorized)
        {
            if durable.state == xai_grok_science::RunState::AwaitingApproval
                && prepared.recovery_grant.is_none()
            {
                let terminal = xai_grok_science::seqbench::finish_without_execution_recoverable(
                    &prepared.store,
                    &prepared.ticket,
                    xai_grok_science::ApprovalDecision::Deny,
                    "missing or mismatched actor permission grant",
                )?;
                return Err(xai_grok_science::ScienceError::Invalid(format!(
                    "science run {} finished {:?}: actor permission grant rejected",
                    prepared.ticket.run_id.0, terminal.state
                )));
            }
            return Err(xai_grok_science::ScienceError::Invalid(
                "sequence Allow requires exactly one fresh or recovery actor grant".into(),
            ));
        }
        if decision != xai_grok_science::ApprovalDecision::Allow
            && (permission_grant.is_some() || prepared.recovery_grant.is_some())
        {
            return Err(xai_grok_science::ScienceError::Invalid(
                "non-Allow sequence analysis carried an actor authority grant".into(),
            ));
        }
        if decision != xai_grok_science::ApprovalDecision::Allow {
            let terminal = xai_grok_science::seqbench::finish_without_execution_recoverable(
                &prepared.store,
                &prepared.ticket,
                decision,
                reason,
            )?;
            return Err(xai_grok_science::ScienceError::Invalid(format!(
                "science run {} finished {:?}",
                prepared.ticket.run_id.0, terminal.state
            )));
        }
        let mut recovery_witness = prepared.allowed_witness.take();
        if recovery_authorized {
            let approvals = prepared.store.approvals(&prepared.ticket.run_id)?;
            let [approval] = approvals.as_slice() else {
                return Err(xai_grok_science::ScienceError::Invalid(
                    "sequence recovery requires exactly one durable approval".into(),
                ));
            };
            if durable.state != xai_grok_science::RunState::Running
                || durable.terminal_reason.is_some()
                || approval.project_id != prepared.ticket.project_id
                || approval.run_id != prepared.ticket.run_id
                || approval.owner_id != prepared.ticket.owner_id
                || approval.call_id != prepared.ticket.call_id
                || approval.decision != xai_grok_science::ApprovalDecision::Allow
                || approval.decided_at.is_none()
            {
                return Err(xai_grok_science::ScienceError::Invalid(
                    "sequence recovery grant lost its durable Running Allow".into(),
                ));
            }
        }
        let project_id = xai_grok_science::project::ProjectId(prepared.ticket.project_id.0.clone());
        prepared.project_store.with_owned_project_revision(
            &project_id,
            &prepared.ticket.owner_id,
            |_project, revision| {
                if revision != prepared.project_revision {
                    let reason = "project changed while sequence analysis approval was pending";
                    if fresh_authorized {
                        // The operator really granted Allow. Preserve that
                        // audit fact, then fail the now-stale execution only
                        // after the exact zero-output cleanup has succeeded.
                        if prepared.created_event_from_recovery {
                            xai_grok_science::seqbench::mark_allowed_recoverable_after_reprompt(
                                &prepared.store,
                                &prepared.ticket,
                                &prepared.created_event,
                            )?;
                        } else {
                            xai_grok_science::seqbench::mark_allowed_recoverable_fresh(
                                &prepared.store,
                                &prepared.ticket,
                                &prepared.created_event,
                            )?;
                        }
                    }
                    let terminal = xai_grok_science::seqbench::fail_allowed_analysis_recoverably(
                        &prepared.store,
                        &prepared.ticket,
                        reason,
                    )?;
                    return Err(xai_grok_science::ScienceError::Invalid(format!(
                        "science run {} finished {:?}: {reason}",
                        prepared.ticket.run_id.0, terminal.state
                    )));
                }
                let allowed_witness = if fresh_authorized {
                    if prepared.created_event_from_recovery {
                        xai_grok_science::seqbench::mark_allowed_recoverable_after_reprompt(
                            &prepared.store,
                            &prepared.ticket,
                            &prepared.created_event,
                        )?
                    } else {
                        xai_grok_science::seqbench::mark_allowed_recoverable_fresh(
                            &prepared.store,
                            &prepared.ticket,
                            &prepared.created_event,
                        )?
                    }
                } else {
                    recovery_witness.take().ok_or_else(|| {
                        xai_grok_science::ScienceError::Invalid(
                            "sequence Running recovery lost its sealed authority witness".into(),
                        )
                    })?
                };
                xai_grok_science::seqbench::finish_analysis_authorized_with_options(
                    &prepared.store,
                    prepared.ticket.clone(),
                    &prepared.source_path,
                    &prepared.source_bytes,
                    &prepared.options,
                    allowed_witness,
                )
            },
        )
    }

    /// Close a durable sequence-analysis Begin whose response receiver
    /// disappeared before the prepared actor capability could be delivered.
    ///
    /// No analysis is run and no output registry is touched. The retained
    /// store marks the pending approval Interrupted and closes the run through
    /// the same actor loop that created it.
    pub(super) fn interrupt_undelivered_science_seq_analyze_begin(
        &self,
        prepared: PreparedScienceSeqAnalyze,
    ) -> xai_grok_science::Result<()> {
        if prepared.replayed.is_some() {
            return Ok(());
        }
        if prepared.recovery_grant.is_some() {
            // The Allow was already durable before this Begin. Losing the
            // response must not rewrite that decision; dropping `prepared`
            // releases the process lease and the same operation can resume.
            return Ok(());
        }
        interrupt_undelivered_seq_authority(&prepared.store, &prepared.ticket)
    }

    /// Inspect and durably admit an uploaded skill archive without writing its
    /// bytes anywhere. The retained store and actor-derived workspace are the
    /// only possible authority roots.
    pub(super) fn prepare_science_skill_quarantine(
        &self,
        store: xai_grok_science::ScienceStore,
        context: xai_grok_science::RunContext,
        request: xai_grok_science::skill_quarantine::SkillQuarantineRequest,
        archive_bytes: Vec<u8>,
    ) -> xai_grok_science::Result<PreparedScienceSkillQuarantine> {
        let actor_session = self.session_info.id.0.as_ref();
        let actor_workspace = dunce::canonicalize(&self.session_info.cwd)?;
        if context.session_id != actor_session
            || context.workspace_root != actor_workspace
            || !context.artifact_root.starts_with(&actor_workspace)
            || dunce::canonicalize(store.root())? != context.artifact_root
        {
            return Err(xai_grok_science::ScienceError::Invalid(
                "skill quarantine context or store does not belong to this SessionActor".into(),
            ));
        }
        let admission = xai_grok_science::skill_quarantine::inspect_archive(
            &archive_bytes,
            &request,
            Default::default(),
        )?;
        let (ticket, replayed) = match store.load_run_optional(&context.run_id)? {
            Some(run) => {
                if run.context.project_id != context.project_id
                    || run.context.owner_id != context.owner_id
                    || run.context.session_id != context.session_id
                    || run.context.workspace_root != context.workspace_root
                    || run.context.artifact_root != context.artifact_root
                    || run.context.environment.get("skill_archive_sha256")
                        != Some(&admission.archive_sha256().to_owned())
                    || run.context.environment.get("skill_admission_sha256")
                        != Some(&admission.sha256().to_owned())
                    || run.context.environment.get("skill_operation_id")
                        != Some(&admission.operation_id().to_owned())
                {
                    return Err(xai_grok_science::ScienceError::Invalid(
                        "skill quarantine operation id was reused with different authority bindings"
                            .into(),
                    ));
                }
                if run.state != xai_grok_science::RunState::Succeeded {
                    return Err(xai_grok_science::ScienceError::Invalid(format!(
                        "skill quarantine operation already ended or remains active as {:?}",
                        run.state
                    )));
                }
                let ticket = xai_grok_science::csv::ScienceRunTicket {
                    project_id: context.project_id.clone(),
                    run_id: context.run_id.clone(),
                    owner_id: context.owner_id.clone(),
                    call_id: xai_grok_science::CallId::new("science_skill_quarantine_import"),
                };
                let replayed = xai_grok_science::skill_quarantine::aggregate(
                    &store,
                    run,
                    admission.operation_id().to_owned(),
                )?;
                (ticket, Some(replayed))
            }
            None => (
                xai_grok_science::skill_quarantine::begin_quarantine(
                    &store,
                    context.clone(),
                    &admission,
                )?,
                None,
            ),
        };
        let target = context
            .artifact_root
            .join("runs")
            .join(&ticket.run_id.0)
            .join("artifacts")
            .join("quarantine")
            .display()
            .to_string();
        Ok(PreparedScienceSkillQuarantine {
            store,
            ticket,
            admission,
            target,
            replayed,
        })
    }

    pub(super) fn finish_science_skill_quarantine(
        &self,
        prepared: PreparedScienceSkillQuarantine,
        decision: xai_grok_science::ApprovalDecision,
        reason: String,
        permission_grant: Option<crate::session::handle::ScienceSkillQuarantinePermissionGrant>,
    ) -> xai_grok_science::Result<xai_grok_science::skill_quarantine::SkillQuarantineResult> {
        if decision != xai_grok_science::ApprovalDecision::Allow {
            let terminal = xai_grok_science::csv::finish_without_execution(
                &prepared.store,
                &prepared.ticket,
                decision,
                reason,
            )?;
            return Err(xai_grok_science::ScienceError::Invalid(format!(
                "science run {} finished {:?}",
                prepared.ticket.run_id.0, terminal.state
            )));
        }
        if permission_grant
            .as_ref()
            .is_none_or(|grant| !grant.authorizes(&prepared))
        {
            return Err(xai_grok_science::ScienceError::Invalid(
                "skill quarantine Allow is missing its bound production permission grant".into(),
            ));
        }
        xai_grok_science::csv::mark_allowed(&prepared.store, &prepared.ticket)?;
        xai_grok_science::skill_quarantine::finish_quarantine(
            &prepared.store,
            prepared.ticket,
            prepared.admission,
        )
    }

    /// Admit a dossier composition without exposing source artifact bytes to
    /// the ACP request task. Source runs and the project revision are checked
    /// before the durable approval begins, then checked again at commit.
    pub(super) fn prepare_science_evidence_dossier(
        &self,
        store: xai_grok_science::ScienceStore,
        project_root: std::path::PathBuf,
        context: xai_grok_science::RunContext,
        source_run_ids: Vec<xai_grok_science::RunId>,
    ) -> xai_grok_science::Result<PreparedScienceEvidenceDossier> {
        if context.session_id != self.session_info.id.0.as_ref() {
            return Err(xai_grok_science::ScienceError::Invalid(
                "evidence dossier session does not match this SessionActor".into(),
            ));
        }
        self.science_feature_gates.require_all(&[
            xai_grok_science::features::ScienceFeature::ResearchProject,
            xai_grok_science::features::ScienceFeature::EvidenceGraph,
        ])?;
        let actor_workspace = dunce::canonicalize(&self.session_info.cwd)?;
        validate_project_mutation_actor_roots(&actor_workspace, &store, &project_root, &context)?;
        let canonical_project_root = dunce::canonicalize(&project_root)?;
        let canonical_store_root = dunce::canonicalize(store.root())?;
        if source_run_ids.is_empty()
            || source_run_ids.len() > xai_grok_science::dossier::MAX_SOURCE_RUNS
        {
            return Err(xai_grok_science::ScienceError::Invalid(format!(
                "evidence dossier requires 1..={} source runs",
                xai_grok_science::dossier::MAX_SOURCE_RUNS
            )));
        }
        let unique_source_runs = source_run_ids
            .iter()
            .collect::<std::collections::BTreeSet<_>>();
        if unique_source_runs.len() != source_run_ids.len()
            || source_run_ids
                .iter()
                .any(|run_id| run_id == &context.run_id)
        {
            return Err(xai_grok_science::ScienceError::Invalid(
                "evidence dossier source runs must be unique and cannot include the dossier run"
                    .into(),
            ));
        }

        let project_store =
            xai_grok_science::project::ProjectStore::new_confined(&project_root, &actor_workspace)?
                .with_gates(self.science_feature_gates.clone());
        if !store.shares_root_capability_with(&project_store)? {
            return Err(xai_grok_science::ScienceError::Invalid(
                "evidence dossier science and project stores pin different root identities".into(),
            ));
        }
        let project_id = xai_grok_science::project::ProjectId(context.project_id.0.clone());
        let (project, project_revision) = project_store.with_owned_project_revision(
            &project_id,
            &context.owner_id,
            |project, revision| Ok((project.clone(), revision.to_owned())),
        )?;

        for source_run_id in &source_run_ids {
            let source = store.load_run_bounded(
                source_run_id,
                xai_grok_science::dossier::MAX_SOURCE_METADATA_BYTES as u64,
            )?;
            if source.state != xai_grok_science::RunState::Succeeded
                || source.context.project_id != context.project_id
                || source.context.owner_id != context.owner_id
                || source.context.session_id != context.session_id
                || source.context.workspace_root != context.workspace_root
                || source.context.artifact_root != context.artifact_root
            {
                return Err(xai_grok_science::ScienceError::Ownership);
            }
        }

        let source_snapshots =
            xai_grok_science::dossier::capture_source_snapshots(&store, &source_run_ids)?;
        let admission = xai_grok_science::dossier::DossierAdmission::new(
            &context,
            source_snapshots,
            project_revision,
            project.title.clone(),
            project.research_question.clone(),
            "SessionActor/evidence-dossier-v1".into(),
        )?;
        let ticket = xai_grok_science::dossier::begin_dossier(&store, context, &admission)?;
        let target = canonical_store_root
            .join("runs")
            .join(&ticket.run_id.0)
            .join("artifacts")
            .display()
            .to_string();
        Ok(PreparedScienceEvidenceDossier {
            store,
            project_store,
            ticket,
            project,
            admission,
            target,
        })
    }

    pub(super) fn finish_science_evidence_dossier(
        &self,
        prepared: PreparedScienceEvidenceDossier,
        decision: xai_grok_science::ApprovalDecision,
        reason: String,
        permission_grant: Option<crate::session::handle::ScienceDossierPermissionGrant>,
    ) -> xai_grok_science::Result<xai_grok_science::dossier::DossierResult> {
        if decision == xai_grok_science::ApprovalDecision::Allow
            && permission_grant
                .as_ref()
                .is_none_or(|grant| !grant.authorizes(&prepared).unwrap_or(false))
        {
            let terminal = xai_grok_science::csv::finish_without_execution(
                &prepared.store,
                &prepared.ticket,
                xai_grok_science::ApprovalDecision::Deny,
                "missing or mismatched actor permission grant",
            )?;
            return Err(xai_grok_science::ScienceError::Invalid(format!(
                "science run {} finished {:?}: actor permission grant rejected",
                prepared.ticket.run_id.0, terminal.state
            )));
        }
        if decision != xai_grok_science::ApprovalDecision::Allow {
            let terminal = xai_grok_science::csv::finish_without_execution(
                &prepared.store,
                &prepared.ticket,
                decision,
                reason,
            )?;
            return Err(xai_grok_science::ScienceError::Invalid(format!(
                "science run {} finished {:?}",
                prepared.ticket.run_id.0, terminal.state
            )));
        }
        xai_grok_science::csv::mark_allowed(&prepared.store, &prepared.ticket)?;
        let project_id = prepared.project.project_id.clone();
        let result = prepared.project_store.with_owned_project_revision(
            &project_id,
            &prepared.ticket.owner_id,
            |current_project, current_revision| {
                if current_revision != prepared.admission.project_revision()
                    || current_project.project_id != prepared.project.project_id
                    || current_project.owner_id.0 != prepared.project.owner_id.0
                    || current_project.title != prepared.project.title
                    || current_project.research_question != prepared.project.research_question
                {
                    return Err(xai_grok_science::ScienceError::Invalid(
                        "project changed while evidence dossier approval was pending".into(),
                    ));
                }
                xai_grok_science::dossier::finish_dossier(
                    &prepared.store,
                    prepared.ticket.clone(),
                    prepared.admission.clone(),
                )
            },
        );
        if let Err(error) = &result
            && prepared
                .store
                .load_run(&prepared.ticket.run_id)
                .is_ok_and(|run| run.state == xai_grok_science::RunState::Running)
        {
            let _ = xai_grok_science::csv::fail_running(
                &prepared.store,
                &prepared.ticket,
                error.to_string(),
            );
        }
        result
    }

    pub(super) async fn finish_science_import(
        &self,
        prepared: PreparedScienceImport,
        decision: xai_grok_science::ApprovalDecision,
        reason: String,
    ) -> xai_grok_science::Result<xai_grok_science::import::ImportResult> {
        if decision != xai_grok_science::ApprovalDecision::Allow {
            let terminal = xai_grok_science::csv::finish_without_execution(
                &prepared.store,
                &prepared.ticket,
                decision,
                reason,
            )?;
            return Err(xai_grok_science::ScienceError::Invalid(format!(
                "science run {} finished {:?}",
                prepared.ticket.run_id.0, terminal.state
            )));
        }

        xai_grok_science::csv::mark_allowed(&prepared.store, &prepared.ticket)?;
        let tool_name = self
            .agent
            .borrow()
            .tool_bridge()
            .toolset()
            .tool_name_for_kind(xai_grok_tools::types::tool::ToolKind::Execute)
            .ok_or_else(|| {
                xai_grok_science::ScienceError::Invalid(
                    "session toolset has no execute tool".into(),
                )
            })?;
        prepared.store.append_event(
            &prepared.ticket.run_id,
            "LumenToolDispatch",
            "tool.started",
            serde_json::json!({
                "tool": tool_name,
                "call_id": prepared.ticket.call_id.0,
                "dispatch": "WorkspaceOps::call_tool"
            }),
        )?;
        let args = serde_json::to_value(BashToolInput {
            command: prepared.command.clone(),
            timeout: Some(30_000),
            description: "Transit Lumen Science import bytes through the formal workspace tool"
                .into(),
            is_background: false,
        })
        .map_err(xai_grok_science::ScienceError::Serde)?;
        let dispatched = self
            .workspace_ops
            .call_tool(
                &tool_name,
                args,
                &prepared.ticket.call_id.0,
                Some(&self.session_info.id.0),
            )
            .await;
        let output = match dispatched {
            Ok(output) => output,
            Err(error) => {
                let reason = format!("formal tool dispatch failed: {error}");
                let _ = xai_grok_science::csv::fail_running(
                    &prepared.store,
                    &prepared.ticket,
                    reason.clone(),
                );
                return Err(xai_grok_science::ScienceError::Invalid(reason));
            }
        };
        match output.output {
            ToolsToolOutput::Bash(ref bash) if bash.exit_code == 0 && !bash.timed_out => {}
            ToolsToolOutput::Bash(ref bash) => {
                let reason = format!(
                    "science import transit tool failed: exit={} timed_out={}",
                    bash.exit_code, bash.timed_out
                );
                let _ = xai_grok_science::csv::fail_running(
                    &prepared.store,
                    &prepared.ticket,
                    reason.clone(),
                );
                return Err(xai_grok_science::ScienceError::Invalid(reason));
            }
            _ => {
                let reason = "science execute tool returned a non-bash output".to_string();
                let _ = xai_grok_science::csv::fail_running(
                    &prepared.store,
                    &prepared.ticket,
                    reason.clone(),
                );
                return Err(xai_grok_science::ScienceError::Invalid(reason));
            }
        }
        let transited = std::fs::read(&prepared.output_path)?;
        if transited != prepared.bytes {
            let reason = "formal tool bytes diverge from import input".to_string();
            let _ = xai_grok_science::csv::fail_running(
                &prepared.store,
                &prepared.ticket,
                reason.clone(),
            );
            return Err(xai_grok_science::ScienceError::Invalid(reason));
        }
        xai_grok_science::import::finish_import(
            &prepared.store,
            prepared.ticket,
            &prepared.source_path,
            &transited,
            format!("{tool_name} via WorkspaceOps::call_tool"),
        )
    }

    /// S3 phase one inside the sole session actor: begin the durable fetch
    /// run and stage the offline response bytes for formal tool transit.
    pub(super) fn prepare_science_fetch(
        &self,
        store: xai_grok_science::ScienceStore,
        context: xai_grok_science::RunContext,
        connector_id: String,
        query: String,
        requests: Vec<xai_grok_science::connectors::ValidatedRequest>,
        fixture_bytes: Vec<Vec<u8>>,
        capability_provenance: Option<
            xai_grok_science::connectors::fetch::CapabilitySourceProvenance,
        >,
    ) -> xai_grok_science::Result<PreparedScienceFetch> {
        if context.session_id != self.session_info.id.0.as_ref() {
            return Err(xai_grok_science::ScienceError::Invalid(
                "connector fetch session does not match this SessionActor".into(),
            ));
        }
        if requests.len() != fixture_bytes.len() || requests.is_empty() {
            return Err(xai_grok_science::ScienceError::Invalid(
                "fetch requires one staged response per request".into(),
            ));
        }
        let actor_workspace = dunce::canonicalize(&self.session_info.cwd)?;
        let project_root = store.root().to_path_buf();
        validate_project_mutation_actor_roots(&actor_workspace, &store, &project_root, &context)?;
        let project_store =
            xai_grok_science::project::ProjectStore::new_confined(&project_root, &actor_workspace)?
                .with_gates(self.science_feature_gates.clone());
        if !store.shares_root_capability_with(&project_store)? {
            return Err(xai_grok_science::ScienceError::Invalid(
                "connector fetch science and project stores pin different root identities".into(),
            ));
        }
        let project_id = xai_grok_science::project::ProjectId(context.project_id.0.clone());
        let project = project_store.load_project(&project_id)?;
        if project.owner_id.0 != context.owner_id {
            return Err(xai_grok_science::ScienceError::Ownership);
        }
        let ticket = xai_grok_science::connectors::fetch::begin_fetch(&store, context.clone())?;
        let staging = context
            .artifact_root
            .join(&ticket.run_id.0)
            .join("tool-staging");
        let mut command = format!("python3 -c {}", quote(FETCH_TOOL_SCRIPT)?);
        let mut output_paths = Vec::with_capacity(fixture_bytes.len());
        for index in 0..fixture_bytes.len() {
            let input_path = staging.join(format!("input_{index}.bin"));
            let output_path = staging.join(format!("output_{index}.bin"));
            command.push_str(&format!(
                " {} {}",
                quote(&input_path.to_string_lossy())?,
                quote(&output_path.to_string_lossy())?,
            ));
            output_paths.push(output_path);
        }
        Ok(PreparedScienceFetch {
            store,
            ticket,
            connector_id,
            query,
            requests,
            fixture_bytes,
            capability_provenance,
            command,
            output_paths,
        })
    }

    pub(super) async fn finish_science_fetch(
        &self,
        prepared: PreparedScienceFetch,
        decision: xai_grok_science::ApprovalDecision,
        reason: String,
    ) -> xai_grok_science::Result<xai_grok_science::connectors::fetch::FetchResult> {
        if decision != xai_grok_science::ApprovalDecision::Allow {
            let terminal = xai_grok_science::csv::finish_without_execution(
                &prepared.store,
                &prepared.ticket,
                decision,
                reason,
            )?;
            return Err(xai_grok_science::ScienceError::Invalid(format!(
                "science run {} finished {:?}",
                prepared.ticket.run_id.0, terminal.state
            )));
        }

        xai_grok_science::csv::mark_allowed(&prepared.store, &prepared.ticket)?;
        let staging_result = (|| -> std::io::Result<()> {
            let staging = prepared
                .output_paths
                .first()
                .and_then(|path| path.parent())
                .ok_or_else(|| std::io::Error::other("fetch has no staging directory"))?;
            std::fs::create_dir_all(staging)?;
            for (index, bytes) in prepared.fixture_bytes.iter().enumerate() {
                std::fs::write(staging.join(format!("input_{index}.bin")), bytes)?;
            }
            Ok(())
        })();
        if let Err(error) = staging_result {
            let reason = format!("failed to stage allowed connector bytes: {error}");
            let _ = xai_grok_science::csv::fail_running(
                &prepared.store,
                &prepared.ticket,
                reason.clone(),
            );
            return Err(xai_grok_science::ScienceError::Invalid(reason));
        }
        let tool_name = self
            .agent
            .borrow()
            .tool_bridge()
            .toolset()
            .tool_name_for_kind(xai_grok_tools::types::tool::ToolKind::Execute)
            .ok_or_else(|| {
                xai_grok_science::ScienceError::Invalid(
                    "session toolset has no execute tool".into(),
                )
            })?;
        prepared.store.append_event(
            &prepared.ticket.run_id,
            "LumenToolDispatch",
            "tool.started",
            serde_json::json!({
                "tool": tool_name,
                "call_id": prepared.ticket.call_id.0,
                "dispatch": "WorkspaceOps::call_tool"
            }),
        )?;
        let args = serde_json::to_value(BashToolInput {
            command: prepared.command.clone(),
            timeout: Some(30_000),
            description: "Transit Lumen Science connector bytes through the formal workspace tool"
                .into(),
            is_background: false,
        })
        .map_err(xai_grok_science::ScienceError::Serde)?;
        let dispatched = self
            .workspace_ops
            .call_tool(
                &tool_name,
                args,
                &prepared.ticket.call_id.0,
                Some(&self.session_info.id.0),
            )
            .await;
        let output = match dispatched {
            Ok(output) => output,
            Err(error) => {
                let reason = format!("formal tool dispatch failed: {error}");
                let _ = xai_grok_science::csv::fail_running(
                    &prepared.store,
                    &prepared.ticket,
                    reason.clone(),
                );
                return Err(xai_grok_science::ScienceError::Invalid(reason));
            }
        };
        match output.output {
            ToolsToolOutput::Bash(ref bash) if bash.exit_code == 0 && !bash.timed_out => {}
            ToolsToolOutput::Bash(ref bash) => {
                let reason = format!(
                    "science fetch transit tool failed: exit={} timed_out={}",
                    bash.exit_code, bash.timed_out
                );
                let _ = xai_grok_science::csv::fail_running(
                    &prepared.store,
                    &prepared.ticket,
                    reason.clone(),
                );
                return Err(xai_grok_science::ScienceError::Invalid(reason));
            }
            _ => {
                let reason = "science execute tool returned a non-bash output".to_string();
                let _ = xai_grok_science::csv::fail_running(
                    &prepared.store,
                    &prepared.ticket,
                    reason.clone(),
                );
                return Err(xai_grok_science::ScienceError::Invalid(reason));
            }
        }
        let mut exchanges = Vec::with_capacity(prepared.requests.len());
        for (index, request) in prepared.requests.into_iter().enumerate() {
            let transited = std::fs::read(&prepared.output_paths[index])?;
            if transited != prepared.fixture_bytes[index] {
                let reason = format!("formal tool bytes diverge on fetch exchange {index}");
                let _ = xai_grok_science::csv::fail_running(
                    &prepared.store,
                    &prepared.ticket,
                    reason.clone(),
                );
                return Err(xai_grok_science::ScienceError::Invalid(reason));
            }
            exchanges.push(xai_grok_science::connectors::fetch::FetchExchange {
                request,
                response: transited,
            });
        }
        xai_grok_science::connectors::fetch::finish_fetch(
            &prepared.store,
            prepared.ticket,
            &prepared.connector_id,
            &prepared.query,
            exchanges,
            format!("{tool_name} via WorkspaceOps::call_tool"),
            prepared.capability_provenance,
        )
    }

    pub(super) async fn finish_science_csv(
        &self,
        prepared: PreparedScienceCsv,
        decision: xai_grok_science::ApprovalDecision,
        reason: String,
    ) -> xai_grok_science::Result<xai_grok_science::csv::ResearchResult> {
        if decision != xai_grok_science::ApprovalDecision::Allow {
            let terminal = xai_grok_science::csv::finish_without_execution(
                &prepared.store,
                &prepared.ticket,
                decision,
                reason,
            )?;
            return Err(xai_grok_science::ScienceError::Invalid(format!(
                "science run {} finished {:?}",
                prepared.ticket.run_id.0, terminal.state
            )));
        }

        xai_grok_science::csv::mark_allowed(&prepared.store, &prepared.ticket)?;
        let tool_name = self
            .agent
            .borrow()
            .tool_bridge()
            .toolset()
            .tool_name_for_kind(xai_grok_tools::types::tool::ToolKind::Execute)
            .ok_or_else(|| {
                xai_grok_science::ScienceError::Invalid(
                    "session toolset has no execute tool".into(),
                )
            })?;
        prepared.store.append_event(
            &prepared.ticket.run_id,
            "LumenToolDispatch",
            "tool.started",
            serde_json::json!({
                "tool": tool_name,
                "call_id": prepared.ticket.call_id.0,
                "dispatch": "WorkspaceOps::call_tool"
            }),
        )?;
        let args = serde_json::to_value(BashToolInput {
            command: prepared.command.clone(),
            timeout: Some(30_000),
            description: "Compute deterministic Lumen Science CSV summary and SVG".into(),
            is_background: false,
        })
        .map_err(xai_grok_science::ScienceError::Serde)?;
        let dispatched = self
            .workspace_ops
            .call_tool(
                &tool_name,
                args,
                &prepared.ticket.call_id.0,
                Some(&self.session_info.id.0),
            )
            .await;
        let output = match dispatched {
            Ok(output) => output,
            Err(error) => {
                let reason = format!("formal tool dispatch failed: {error}");
                let _ = xai_grok_science::csv::fail_running(
                    &prepared.store,
                    &prepared.ticket,
                    reason.clone(),
                );
                return Err(xai_grok_science::ScienceError::Invalid(reason));
            }
        };
        match output.output {
            ToolsToolOutput::Bash(ref bash) if bash.exit_code == 0 && !bash.timed_out => {}
            ToolsToolOutput::Bash(ref bash) => {
                let reason = format!(
                    "science compute tool failed: exit={} timed_out={}",
                    bash.exit_code, bash.timed_out
                );
                let _ = xai_grok_science::csv::fail_running(
                    &prepared.store,
                    &prepared.ticket,
                    reason.clone(),
                );
                return Err(xai_grok_science::ScienceError::Invalid(reason));
            }
            _ => {
                let reason = "science execute tool returned a non-bash output".to_string();
                let _ = xai_grok_science::csv::fail_running(
                    &prepared.store,
                    &prepared.ticket,
                    reason.clone(),
                );
                return Err(xai_grok_science::ScienceError::Invalid(reason));
            }
        }
        let summary = std::fs::read(&prepared.summary_path)?;
        let svg = std::fs::read(&prepared.svg_path)?;
        xai_grok_science::csv::finish_from_tool_output(
            &prepared.store,
            prepared.ticket,
            &prepared.fixture_path,
            &prepared.fixture,
            &summary,
            &svg,
            format!("{tool_name} via WorkspaceOps::call_tool"),
        )
    }

    // ── WP-2 project mutations ────────────────────────────────────

    /// WP-2 phase one inside the sole session actor: bind the mutation to this
    /// session, refuse it if the operation id or the project belongs to
    /// someone else, and open the durable run that its approval will finish.
    ///
    /// Nothing is written to the project store here. An operation id that has
    /// already been applied short-circuits: it returns the recorded outcome
    /// without opening a run or asking for permission again, which is what
    /// makes a retry idempotent rather than a second prompt.
    pub(super) fn prepare_science_project_mutation(
        &self,
        store: xai_grok_science::ScienceStore,
        project_root: std::path::PathBuf,
        mut context: xai_grok_science::RunContext,
        mut request: xai_grok_science::project::MutationRequest,
    ) -> xai_grok_science::Result<PreparedScienceProjectMutation> {
        // Session binding: the actor only mutates on behalf of its own session.
        if request.session_id != self.session_info.id.0.as_ref()
            || context.session_id != self.session_info.id.0.as_ref()
        {
            return Err(xai_grok_science::ScienceError::Invalid(
                "science mutation session does not match this SessionActor".into(),
            ));
        }
        if request.owner_id != context.owner_id {
            return Err(xai_grok_science::ScienceError::Invalid(
                "science mutation owner does not match its run context".into(),
            ));
        }
        self.science_feature_gates
            .require_all(request.mutation.required_features())?;
        validate_project_mutation_actor_roots(
            std::path::Path::new(&self.session_info.cwd),
            &store,
            &project_root,
            &context,
        )?;
        let gates = self.science_feature_gates.clone();
        let project_store = xai_grok_science::project::ProjectStore::new_confined(
            &project_root,
            std::path::Path::new(&self.session_info.cwd),
        )?
        .with_gates(gates.clone());
        if !store.shares_root_capability_with(&project_store)? {
            return Err(xai_grok_science::ScienceError::Invalid(
                "science and project stores do not retain the same root capability".into(),
            ));
        }

        // Project binding: the run context must name the project actually
        // being mutated, so the durable record cannot point at another one.
        // Capture the actual pre-permission revision even when the caller
        // intentionally omitted optimistic CAS. Review admission replaces
        // this snapshot below with the value captured under its write guard.
        let mut project_revision = None;
        if let Some(target) = request.mutation.target_project() {
            if context.project_id.0 != target.0 {
                return Err(xai_grok_science::ScienceError::Invalid(
                    "science mutation project does not match its run context".into(),
                ));
            }
            let project = project_store.load_project(target)?;
            if project.owner_id.0 != request.owner_id {
                return Err(xai_grok_science::ScienceError::Ownership);
            }
            project_revision = Some(project_store.project_revision(target)?);
        }
        if let xai_grok_science::project::ProjectMutation::ReviewRecord {
            source_run_id,
            authority_run_id,
            reviewer_id,
            ..
        } = &request.mutation
        {
            let expected_authority = format!("review-authority-{}", request.replay_fingerprint()?);
            if authority_run_id != &expected_authority
                || context.run_id.0 != expected_authority
                || context.project_id.0
                    != request
                        .mutation
                        .target_project()
                        .expect("review has a target project")
                        .0
            {
                return Err(xai_grok_science::ScienceError::Ownership);
            }
            if reviewer_id != &request.owner_id {
                return Err(xai_grok_science::ScienceError::Ownership);
            }
            if source_run_id.is_empty()
                || source_run_id.len() > 128
                || !source_run_id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
            {
                return Err(xai_grok_science::ScienceError::Invalid(
                    "review source run id is invalid".into(),
                ));
            }
            let source = store.load_run(&xai_grok_science::RunId::new(source_run_id))?;
            if source.state != xai_grok_science::RunState::Succeeded {
                return Err(xai_grok_science::ScienceError::Invalid(
                    "review source run must be succeeded".into(),
                ));
            }
            if source.context.project_id.0 != context.project_id.0
                || source.context.owner_id != context.owner_id
                || source.context.session_id != context.session_id
            {
                return Err(xai_grok_science::ScienceError::Ownership);
            }
            if source.context.workspace_root != context.workspace_root
                || source.context.artifact_root != context.artifact_root
                || source.context.artifact_root != project_root.join("runs")
            {
                return Err(xai_grok_science::ScienceError::Invalid(
                    "review source run is outside this SessionActor workspace/store".into(),
                ));
            }
            xai_grok_science::review::verify_for_goal_completion(
                &store,
                &xai_grok_science::RunId::new(source_run_id),
            )?;
        }

        // A completed (or post-ledger interrupted) review must be recovered
        // before capturing a fresh admission: the durable review itself moves
        // the project revision, so recapturing against the post-review tree
        // would manufacture a different authority snapshot.
        if matches!(
            request.mutation,
            xai_grok_science::project::ProjectMutation::ReviewRecord { .. }
        ) && let Some(record) = project_store.lookup_operation(&request.operation_id)?
        {
            record.verify_replay(&request)?;
            if record.kind != "review_record" {
                return Err(xai_grok_science::ScienceError::Invalid(
                    "review operation id belongs to another mutation kind".into(),
                ));
            }
            let review: xai_grok_science::project::ReviewRecord =
                serde_json::from_value(record.result.clone())?;
            validate_review_replay_request(&request, &review)?;
            let expected_authority = format!("review-authority-{}", request.replay_fingerprint()?);
            if review.authority_run_id != expected_authority
                || context.run_id.0 != expected_authority
            {
                return Err(xai_grok_science::ScienceError::Ownership);
            }
            recover_interrupted_review_commit(&store, &project_store, &record, &review)?;
            project_store.verify_review_record_with_store(&store, &review)?;
            let authority =
                store.load_run(&xai_grok_science::RunId::new(&review.authority_run_id))?;
            let outcome = xai_grok_science::project::MutationOutcome {
                operation_id: record.operation_id,
                kind: record.kind,
                project_id: record.project_id,
                revision: record.revision,
                result: record.result,
                replayed: true,
            };
            let replay_target = format!(
                "{}/projects/{} (verified review replay)",
                project_root.display(),
                outcome.project_id.0
            );
            return Ok(PreparedScienceProjectMutation {
                store,
                project_store,
                ticket: xai_grok_science::csv::ScienceRunTicket {
                    project_id: xai_grok_science::ProjectId::new(outcome.project_id.0.clone()),
                    run_id: xai_grok_science::RunId::new(&review.authority_run_id),
                    owner_id: review.owner_id.clone(),
                    call_id: xai_grok_science::CallId::new("science_project_mutation"),
                },
                expected_context: authority.context,
                request,
                project_revision: Some(review.review_project_revision.clone()),
                project_root,
                review_admission: None,
                migration_admission: None,
                target: replay_target,
                replayed: Some(outcome),
                resume_allowed: false,
                permission_grant: None,
            });
        }

        // A completed migration replay is verified entirely from the
        // target-owned authority run, project records and commit journal. It
        // must not depend on the legacy source still being present after the
        // migration has made an independent copy.
        if matches!(
            request.mutation,
            xai_grok_science::project::ProjectMutation::ProjectMigrate { .. }
        ) && let Some(record) = project_store.lookup_operation(&request.operation_id)?
        {
            record.verify_replay(&request)?;
            let outcome = xai_grok_science::project::MutationOutcome {
                operation_id: record.operation_id,
                kind: record.kind,
                project_id: record.project_id,
                revision: record.revision,
                result: record.result,
                replayed: true,
            };
            let outcome =
                recover_migration_authority_if_needed(&store, &project_store, &request, &outcome)?;
            let result: xai_grok_science::project::MigrationResult =
                serde_json::from_value(outcome.result.clone())?;
            let authority =
                store.load_run(&xai_grok_science::RunId::new(&result.authority_run_id))?;
            let replay_target = format!(
                "{}/projects/{} (verified migration replay)",
                project_root.display(),
                outcome.project_id.0
            );
            return Ok(PreparedScienceProjectMutation {
                store,
                project_store,
                ticket: xai_grok_science::csv::ScienceRunTicket {
                    project_id: xai_grok_science::ProjectId::new(outcome.project_id.0.clone()),
                    run_id: xai_grok_science::RunId::new(result.authority_run_id),
                    owner_id: request.owner_id.clone(),
                    call_id: xai_grok_science::CallId::new("science_project_mutation"),
                },
                expected_context: authority.context,
                request,
                project_revision: None,
                project_root,
                review_admission: None,
                migration_admission: None,
                target: replay_target,
                replayed: Some(outcome),
                resume_allowed: false,
                permission_grant: None,
            });
        }

        // The project journal is written before the generic operation ledger.
        // If a process stopped in that window, recover the original
        // target/authority run without opening a second run or permission
        // prompt.
        if matches!(
            request.mutation,
            xai_grok_science::project::ProjectMutation::ProjectMigrate { .. }
        ) && let Some(commit) = project_store.lookup_migration_commit(&request.operation_id)?
        {
            commit.verify()?;
            if commit.request_sha256 != request.replay_fingerprint()? {
                return Err(xai_grok_science::ScienceError::Invalid(
                    "migration recovery journal does not match this request".into(),
                ));
            }
            let result = xai_grok_science::project::MigrationResult::from_commit(&commit)?;
            let xai_grok_science::project::ProjectMutation::ProjectMigrate {
                authority_run_id, ..
            } = &mut request.mutation
            else {
                unreachable!("migration journal branch checked above");
            };
            *authority_run_id = result.authority_run_id.clone();
            let authority_id = xai_grok_science::RunId::new(&result.authority_run_id);
            let authority = store.load_run(&authority_id)?;
            let mut expected_context = context.clone();
            expected_context.environment.insert(
                "project_migration_admission_sha256".into(),
                commit.admission.sha256()?,
            );
            let ticket = xai_grok_science::csv::ScienceRunTicket {
                project_id: authority.context.project_id.clone(),
                run_id: authority_id.clone(),
                owner_id: authority.context.owner_id.clone(),
                call_id: xai_grok_science::CallId::new("science_project_mutation"),
            };
            if authority.context != expected_context
                || authority
                    .context
                    .environment
                    .get("project_migration_admission_sha256")
                    != Some(&commit.admission.sha256()?)
            {
                return Err(xai_grok_science::ScienceError::Ownership);
            }
            if authority.state == xai_grok_science::RunState::Running {
                validate_migration_authority_event_prefix(
                    &store,
                    &ticket,
                    &expected_context,
                    &request,
                    true,
                )?;
            }
            if xai_grok_science::project::MigrationRecoveryGrant::verify(&store, &commit).is_err() {
                if authority.state != xai_grok_science::RunState::Running {
                    return Err(xai_grok_science::ScienceError::Invalid(
                        "incomplete migration journal is not recoverable from a terminal authority run"
                            .into(),
                    ));
                }
                let bundle = commit
                    .admission
                    .authorize_after_allow(&store, &authority.context)?;
                ensure_migration_target_artifacts(&store, &ticket, &bundle)?;
            }
            let outcome = xai_grok_science::project::MutationOutcome {
                operation_id: request.operation_id.clone(),
                kind: "project_migrate".into(),
                project_id: result.target_project_id.clone(),
                revision: String::new(),
                result: serde_json::to_value(&result)?,
                replayed: true,
            };
            let outcome =
                recover_migration_authority_if_needed(&store, &project_store, &request, &outcome)?;
            let authority =
                store.load_run(&xai_grok_science::RunId::new(&result.authority_run_id))?;
            let recovery_target = format!(
                "{}/projects/{} (recovered migration commit)",
                project_root.display(),
                outcome.project_id.0
            );
            return Ok(PreparedScienceProjectMutation {
                store,
                project_store,
                ticket: xai_grok_science::csv::ScienceRunTicket {
                    project_id: xai_grok_science::ProjectId::new(outcome.project_id.0.clone()),
                    run_id: xai_grok_science::RunId::new(result.authority_run_id),
                    owner_id: request.owner_id.clone(),
                    call_id: xai_grok_science::CallId::new("science_project_mutation"),
                },
                expected_context: authority.context,
                request,
                project_revision: None,
                project_root,
                review_admission: None,
                migration_admission: None,
                target: recovery_target,
                replayed: Some(outcome),
                resume_allowed: false,
                permission_grant: None,
            });
        }

        let migration_admission = match &request.mutation {
            xai_grok_science::project::ProjectMutation::ProjectMigrate {
                source_run_id,
                title,
                research_question,
                authority_run_id,
            } => {
                let target = request.migration_target_project_id()?.ok_or_else(|| {
                    xai_grok_science::ScienceError::Invalid(
                        "project migration has no deterministic target".into(),
                    )
                })?;
                if context.project_id.0 != target.0 || context.run_id.0 != *authority_run_id {
                    return Err(xai_grok_science::ScienceError::Ownership);
                }
                let admission = xai_grok_science::project::MigrationAdmission::capture(
                    &store,
                    &context,
                    xai_grok_science::RunId::new(source_run_id),
                    request.operation_id.clone(),
                    target,
                    xai_grok_science::RunId::new(authority_run_id),
                    title,
                    research_question,
                )?;
                context.environment.insert(
                    "project_migration_admission_sha256".into(),
                    admission.sha256()?,
                );
                Some(admission)
            }
            _ => None,
        };

        let mut target = match request.mutation.target_project() {
            Some(project_id) => format!("{}/projects/{}", project_root.display(), project_id.0),
            None => format!("{}/projects", project_root.display()),
        };
        if let Some(admission) = &migration_admission {
            let digest = admission.sha256()?;
            target = format!(
                "{} (migrate run {} as project {}; admission sha256:{})",
                target,
                admission.source_run_id().0,
                admission.target_project_id().0,
                digest
            );
        }

        // Idempotent replay: already applied, so no run and no second prompt.
        if let Some(record) = project_store.lookup_operation(&request.operation_id)? {
            record.verify_replay(&request)?;
            if record.kind == "review_record" {
                let review: xai_grok_science::project::ReviewRecord =
                    serde_json::from_value(record.result.clone())?;
                validate_review_replay_request(&request, &review)?;
                if project_store
                    .verify_review_record_with_store(&store, &review)
                    .is_err()
                {
                    recover_interrupted_review_commit(&store, &project_store, &record, &review)?;
                }
                project_store.verify_review_record_with_store(&store, &review)?;
            }
            let outcome = xai_grok_science::project::MutationOutcome {
                operation_id: record.operation_id,
                kind: record.kind,
                project_id: record.project_id,
                revision: record.revision,
                result: record.result,
                replayed: true,
            };
            if outcome.kind == "project_migrate" {
                verify_migration_replay(&store, &project_store, &request, &outcome)?;
            }
            let replay_run_id = if outcome.kind == "project_migrate" {
                let migration: xai_grok_science::project::MigrationResult =
                    serde_json::from_value(outcome.result.clone())?;
                xai_grok_science::RunId::new(migration.authority_run_id)
            } else {
                context.run_id.clone()
            };
            return Ok(PreparedScienceProjectMutation {
                store,
                project_store,
                ticket: xai_grok_science::csv::ScienceRunTicket {
                    // The run ticket uses the kernel's ProjectId; the record
                    // carries the project-model one.
                    project_id: xai_grok_science::ProjectId::new(outcome.project_id.0.clone()),
                    run_id: replay_run_id,
                    owner_id: request.owner_id.clone(),
                    call_id: xai_grok_science::CallId::new("science_project_mutation"),
                },
                expected_context: context,
                request,
                project_revision: None,
                project_root,
                review_admission: None,
                migration_admission: None,
                target,
                replayed: Some(outcome),
                resume_allowed: false,
                permission_grant: None,
            });
        }

        // Crash recovery before the operation record exists. `record_review_inner`
        // writes the immutable review ledger before the generic operation ledger;
        // if the process dies between those atomic writes, reuse the ledger's
        // original Running+Allow authority run instead of opening a second run.
        let orphan_review = match &request.mutation {
            xai_grok_science::project::ProjectMutation::ReviewRecord { project_id, .. } => {
                project_store.lookup_review_record(project_id, &request.operation_id)?
            }
            _ => None,
        };
        if let Some(review) = orphan_review {
            let expected_authority = format!("review-authority-{}", request.replay_fingerprint()?);
            if review.authority_run_id != expected_authority
                || context.run_id.0 != expected_authority
            {
                return Err(xai_grok_science::ScienceError::Ownership);
            }
            let grant = xai_grok_science::project::ReviewRecoveryGrant::verify(
                &project_store,
                &store,
                &request,
            )?;
            let outcome = project_store.recover_actor_review_operation(&request, &grant)?;
            let operation = project_store
                .lookup_operation(&request.operation_id)?
                .ok_or_else(|| {
                    xai_grok_science::ScienceError::Invalid(
                        "review orphan recovery did not create its operation record".into(),
                    )
                })?;
            recover_interrupted_review_commit(&store, &project_store, &operation, &review)?;
            let authority =
                store.load_run(&xai_grok_science::RunId::new(&review.authority_run_id))?;
            return Ok(PreparedScienceProjectMutation {
                store,
                project_store,
                ticket: xai_grok_science::csv::ScienceRunTicket {
                    project_id: xai_grok_science::ProjectId::new(review.project_id.0.clone()),
                    run_id: xai_grok_science::RunId::new(&review.authority_run_id),
                    owner_id: review.owner_id.clone(),
                    call_id: xai_grok_science::CallId::new("science_project_mutation"),
                },
                expected_context: authority.context,
                request,
                project_revision: Some(review.review_project_revision.clone()),
                project_root,
                review_admission: None,
                migration_admission: None,
                target,
                replayed: Some(xai_grok_science::project::MutationOutcome {
                    replayed: true,
                    ..outcome
                }),
                resume_allowed: false,
                permission_grant: None,
            });
        }

        let review_admission = match &request.mutation {
            xai_grok_science::project::ProjectMutation::ReviewRecord { source_run_id, .. } => {
                let admission = xai_grok_science::project::ReviewAdmission::capture(
                    &project_store,
                    &store,
                    &request,
                )?;
                if admission.authority_run_id() != context.run_id.0
                    || admission.source_run_id() != source_run_id
                    || admission.project_id().0 != context.project_id.0
                    || admission.owner_id() != context.owner_id
                    || admission.session_id() != context.session_id
                {
                    return Err(xai_grok_science::ScienceError::Ownership);
                }
                for (key, expected) in admission.authority_environment() {
                    if let Some(supplied) = context.environment.get(&key)
                        && supplied != &expected
                    {
                        return Err(xai_grok_science::ScienceError::Invalid(format!(
                            "review context supplied conflicting {key}"
                        )));
                    }
                    context.environment.insert(key, expected);
                }
                project_revision = Some(admission.project_revision().to_string());
                target = format!(
                    "{} (review run {}; admission sha256:{})",
                    target,
                    admission.source_run_id(),
                    admission.sha256()
                );
                Some(admission)
            }
            _ => None,
        };

        let mut resumed_ticket = None;
        let mut resume_allowed = false;
        if let Some(admission) = review_admission.as_ref() {
            match prepare_or_recover_review_authority(
                &store,
                &project_store,
                &context,
                &request,
                admission,
            )? {
                ReviewAuthorityPreparation::AwaitPermission(ticket) => {
                    resumed_ticket = Some(ticket);
                }
                ReviewAuthorityPreparation::ResumeAllowed(ticket) => {
                    resumed_ticket = Some(ticket);
                    resume_allowed = true;
                }
            }
        }
        if migration_admission.is_some()
            && let Some(existing) = store.load_run_optional(&context.run_id)?
        {
            use xai_grok_science::{Approval, ApprovalDecision, CallId, RunState, ScienceError};

            if existing.context != context {
                return Err(ScienceError::Ownership);
            }
            let ticket = xai_grok_science::csv::ScienceRunTicket {
                project_id: context.project_id.clone(),
                run_id: context.run_id.clone(),
                owner_id: context.owner_id.clone(),
                call_id: CallId::new("science_project_mutation"),
            };
            let mut approvals = store.approvals(&ticket.run_id)?;
            if existing.state == RunState::Created {
                ensure_created_migration_begin_event(
                    &store,
                    &project_store,
                    &ticket,
                    &context,
                    &request,
                )?;
                if approvals.is_empty() {
                    store.request_approval(Approval {
                        project_id: ticket.project_id.clone(),
                        run_id: ticket.run_id.clone(),
                        call_id: ticket.call_id.clone(),
                        owner_id: ticket.owner_id.clone(),
                        decision: ApprovalDecision::Pending,
                        decided_at: None,
                    })?;
                    approvals = store.approvals(&ticket.run_id)?;
                }
            }
            let [approval] = approvals.as_slice() else {
                return Err(ScienceError::Invalid(
                    "resumed migration authority requires exactly one approval".into(),
                ));
            };
            if approval.project_id != ticket.project_id
                || approval.run_id != ticket.run_id
                || approval.owner_id != ticket.owner_id
                || approval.call_id != ticket.call_id
            {
                return Err(ScienceError::Ownership);
            }
            match existing.state {
                RunState::Created if approval.decision == ApprovalDecision::Pending => {
                    store.transition(&ticket.run_id, RunState::AwaitingApproval, None)?;
                    resumed_ticket = Some(ticket);
                }
                RunState::AwaitingApproval
                    if approval.decision == ApprovalDecision::Pending
                        && approval.decided_at.is_none() =>
                {
                    ensure_empty_project_mutation_outputs(&store, &ticket.run_id)?;
                    if project_store
                        .lookup_operation(&request.operation_id)?
                        .is_some()
                        || project_store
                            .lookup_migration_commit(&request.operation_id)?
                            .is_some()
                    {
                        return Err(ScienceError::Invalid(
                            "pending migration authority already has a project or commit ledger"
                                .into(),
                        ));
                    }
                    validate_migration_authority_event_prefix(
                        &store, &ticket, &context, &request, false,
                    )?;
                    resumed_ticket = Some(ticket);
                }
                RunState::AwaitingApproval
                    if approval.decision == ApprovalDecision::Allow
                        && approval.decided_at.is_some() =>
                {
                    ensure_empty_project_mutation_outputs(&store, &ticket.run_id)?;
                    if project_store
                        .lookup_operation(&request.operation_id)?
                        .is_some()
                        || project_store
                            .lookup_migration_commit(&request.operation_id)?
                            .is_some()
                    {
                        return Err(ScienceError::Invalid(
                            "pre-Running migration Allow already has a project or commit ledger"
                                .into(),
                        ));
                    }
                    ensure_migration_allowed_event(&store, &ticket, &context, &request)?;
                    store.transition(&ticket.run_id, RunState::Running, None)?;
                    resumed_ticket = Some(ticket);
                    resume_allowed = true;
                }
                RunState::AwaitingApproval
                    if matches!(
                        approval.decision.clone(),
                        ApprovalDecision::Deny
                            | ApprovalDecision::Timeout
                            | ApprovalDecision::Cancel
                            | ApprovalDecision::Interrupted
                    ) && approval.decided_at.is_some() =>
                {
                    let state = recover_migration_terminal_decision(
                        &store,
                        &project_store,
                        &ticket,
                        &context,
                        &request,
                        approval.decision.clone(),
                    )?;
                    return Err(ScienceError::Invalid(format!(
                        "migration authority {} recovered terminal {state:?}",
                        ticket.run_id.0
                    )));
                }
                RunState::Running
                    if approval.decision == ApprovalDecision::Allow
                        && approval.decided_at.is_some() =>
                {
                    validate_migration_authority_event_prefix(
                        &store, &ticket, &context, &request, true,
                    )?;
                    resumed_ticket = Some(ticket);
                    resume_allowed = true;
                }
                state => {
                    return Err(ScienceError::Invalid(format!(
                        "migration authority {} cannot resume from {state:?}/{:?}",
                        ticket.run_id.0, approval.decision
                    )));
                }
            }
        }

        let expected_context = context.clone();
        let ticket = match resumed_ticket {
            Some(ticket) => ticket,
            None => begin_project_mutation_run(
                &store,
                context,
                request.mutation.kind(),
                &request.operation_id,
            )?,
        };
        Ok(PreparedScienceProjectMutation {
            store,
            project_store,
            ticket,
            expected_context,
            request,
            project_revision,
            project_root,
            review_admission,
            migration_admission,
            target,
            replayed: None,
            resume_allowed,
            permission_grant: None,
        })
    }

    /// Close a durable project-mutation Begin whose prepared capability could
    /// not be delivered to the SessionHandle. Replay and internally resumed
    /// authorities did not open a new pending approval and are left intact.
    pub(super) fn interrupt_undelivered_science_project_mutation_begin(
        &self,
        prepared: PreparedScienceProjectMutation,
    ) -> xai_grok_science::Result<()> {
        if prepared.replayed.is_some() || prepared.resume_allowed {
            return Ok(());
        }
        interrupt_pending_project_mutation_authority(
            &prepared,
            "project mutation Begin response receiver closed before delivery",
        )
    }

    /// WP-2 phase two: apply the mutation only on an allow decision, and only
    /// through `ProjectStore::apply_mutation`, which re-checks identity,
    /// ownership and the expected revision while holding the store write lock.
    pub(super) fn finish_science_project_mutation(
        &self,
        prepared: PreparedScienceProjectMutation,
        decision: xai_grok_science::ApprovalDecision,
        reason: String,
    ) -> xai_grok_science::Result<xai_grok_science::project::MutationOutcome> {
        if let Some(outcome) = prepared.replayed.as_ref() {
            return finish_replayed_project_mutation(&prepared, outcome, decision);
        }
        if prepared.resume_allowed {
            if decision != xai_grok_science::ApprovalDecision::Allow
                || prepared.permission_grant.is_some()
            {
                return Err(xai_grok_science::ScienceError::Invalid(
                    "project mutation recovery requires its original durable Allow".into(),
                ));
            }
            validate_running_project_mutation_authority(&prepared)?;
        } else if decision == xai_grok_science::ApprovalDecision::Allow {
            require_exact_project_mutation_allow_grant(&prepared)?;
        } else {
            validate_pending_project_mutation_authority(&prepared)?;
            if prepared.permission_grant.is_some() {
                let terminal = xai_grok_science::csv::finish_without_execution(
                    &prepared.store,
                    &prepared.ticket,
                    xai_grok_science::ApprovalDecision::Deny,
                    "non-Allow project mutation carried an actor Allow grant",
                )?;
                return Err(xai_grok_science::ScienceError::Invalid(format!(
                    "science run {} finished {:?}: inconsistent permission grant",
                    prepared.ticket.run_id.0, terminal.state
                )));
            }
        }
        if decision != xai_grok_science::ApprovalDecision::Allow {
            let terminal = xai_grok_science::csv::finish_without_execution(
                &prepared.store,
                &prepared.ticket,
                decision,
                reason,
            )?;
            return Err(xai_grok_science::ScienceError::Invalid(format!(
                "science run {} finished {:?}",
                prepared.ticket.run_id.0, terminal.state
            )));
        }
        if !prepared.resume_allowed {
            xai_grok_science::csv::mark_allowed(&prepared.store, &prepared.ticket)?;
        }
        if matches!(
            prepared.request.mutation,
            xai_grok_science::project::ProjectMutation::ReviewRecord { .. }
        ) {
            validate_review_authority_event_prefix(
                &prepared.store,
                &prepared.ticket,
                &prepared.expected_context,
                &prepared.request,
                true,
            )?;
            let admission = prepared.review_admission.as_ref().ok_or_else(|| {
                xai_grok_science::ScienceError::Invalid(
                    "review Finish is missing its immutable admission".into(),
                )
            })?;
            if let Err(error) = admission.verify_after_allow(
                &prepared.project_store,
                &prepared.store,
                &prepared.request,
            ) {
                return Err(terminalize_project_mutation_failure(
                    &prepared.store,
                    &prepared.ticket,
                    format!("review admission changed after Allow: {error}"),
                    error,
                ));
            }
        }
        let mut migrated_paths = Vec::new();
        let migration_bundle = match prepared.migration_admission.as_ref() {
            Some(admission) => {
                let authority = prepared.store.load_run(&prepared.ticket.run_id)?;
                if authority
                    .context
                    .environment
                    .get("project_migration_admission_sha256")
                    != Some(&admission.sha256()?)
                {
                    let error = xai_grok_science::ScienceError::Invalid(
                        "migration authority run is not bound to its admitted source digest".into(),
                    );
                    return Err(terminalize_project_mutation_failure(
                        &prepared.store,
                        &prepared.ticket,
                        error.to_string(),
                        error,
                    ));
                }
                let bundle =
                    match admission.authorize_after_allow(&prepared.store, &authority.context) {
                        Ok(bundle) => bundle,
                        Err(error) => {
                            return Err(terminalize_project_mutation_failure(
                                &prepared.store,
                                &prepared.ticket,
                                format!("migration source revalidation failed: {error}"),
                                error,
                            ));
                        }
                    };
                if let Err(error) = prepared.project_store.admit_actor_migration(
                    &prepared.request,
                    admission,
                    &bundle,
                ) {
                    if migration_apply_error_may_have_committed(
                        &prepared.project_store,
                        &prepared.request,
                    ) {
                        return Err(error);
                    }
                    return Err(terminalize_project_mutation_failure(
                        &prepared.store,
                        &prepared.ticket,
                        format!("migration journal admission failed: {error}"),
                        error,
                    ));
                }
                migrated_paths = match ensure_migration_target_artifacts(
                    &prepared.store,
                    &prepared.ticket,
                    &bundle,
                ) {
                    Ok(paths) => paths,
                    Err(error) => return Err(error),
                };
                Some(bundle)
            }
            None => None,
        };
        let outcome = match migration_bundle.as_ref() {
            Some(bundle) => prepared.project_store.apply_actor_migration(
                &prepared.request,
                prepared
                    .migration_admission
                    .as_ref()
                    .expect("migration bundle requires its admission"),
                bundle,
            ),
            None if matches!(
                prepared.request.mutation,
                xai_grok_science::project::ProjectMutation::ReviewRecord { .. }
            ) =>
            {
                prepared.project_store.apply_actor_review(
                    &prepared.request,
                    &prepared.store,
                    prepared.review_admission.as_ref().ok_or_else(|| {
                        xai_grok_science::ScienceError::Invalid(
                            "review apply is missing its immutable admission".into(),
                        )
                    })?,
                )
            }
            None => prepared.project_store.apply_mutation(&prepared.request),
        };
        let outcome = match outcome {
            Ok(outcome) => outcome,
            Err(error) => {
                if review_apply_error_may_have_committed(
                    &prepared.store,
                    &prepared.project_store,
                    &prepared.request,
                ) {
                    // The immutable review ledger is itself the recovery
                    // journal. Keep the authority Running+Allow, but never add
                    // an out-of-protocol event that a later successful atomic
                    // completion would have to bless.
                    return Err(error);
                }
                if migration_apply_error_may_have_committed(
                    &prepared.project_store,
                    &prepared.request,
                ) {
                    return Err(error);
                }
                rollback_migration_outputs(&prepared.store, &prepared.ticket, &migrated_paths);
                return Err(terminalize_project_mutation_failure(
                    &prepared.store,
                    &prepared.ticket,
                    format!("project mutation rejected: {error}"),
                    error,
                ));
            }
        };
        if migration_bundle.is_some()
            && let Err(error) = persist_migration_mutation_evidence(
                &prepared.store,
                &prepared.project_store,
                &prepared.ticket,
                &prepared.request,
                &outcome,
            )
        {
            return Err(error);
        }
        if outcome.kind == "review_record" {
            commit_review_authority_success(
                &prepared.store,
                &prepared.project_store,
                &prepared.ticket,
                &prepared.expected_context,
                &prepared.request,
                &outcome,
            )?;
        } else if outcome.kind == "project_migrate" {
            append_project_mutation_applied_once(
                &prepared.store,
                &prepared.ticket.run_id,
                &outcome,
            )?;
            commit_migration_authority_success(
                &prepared.store,
                &prepared.project_store,
                &prepared.ticket,
                &prepared.expected_context,
                &prepared.request,
                &outcome,
            )?;
        } else {
            append_project_mutation_applied_once(
                &prepared.store,
                &prepared.ticket.run_id,
                &outcome,
            )?;
            prepared
                .store
                .transition_succeeded_verified(&prepared.ticket.run_id)?;
        }
        Ok(outcome)
    }

    // ── LS5-K1 kernel admission ───────────────────────────────────

    /// Open the durable run and pending approval inside the sole actor.
    ///
    /// This phase validates all identity and path bindings, but deliberately
    /// does not hash or execute the interpreter. The permission manager must
    /// decide first.
    pub(super) fn prepare_science_kernel_admission(
        &self,
        store: xai_grok_science::ScienceStore,
        project_root: std::path::PathBuf,
        context: xai_grok_science::RunContext,
        mut request: xai_grok_science::workflow::KernelAdmissionRequest,
    ) -> xai_grok_science::Result<PreparedScienceKernelAdmission> {
        use xai_grok_science::ScienceError;

        validate_kernel_session_binding(self.session_info.id.0.as_ref(), &context)?;
        self.science_feature_gates.require_all(&[
            xai_grok_science::features::ScienceFeature::ResearchProject,
            xai_grok_science::features::ScienceFeature::ComputeEnvironment,
            xai_grok_science::features::ScienceFeature::MultiKernel,
        ])?;
        let actor_workspace = validate_kernel_admission_actor_roots(
            std::path::Path::new(&self.session_info.cwd),
            &store,
            &project_root,
            &context,
        )?;
        let project_store = xai_grok_science::project::ProjectStore::new_confined(
            &project_root,
            std::path::Path::new(&self.session_info.cwd),
        )?
        .with_gates(self.science_feature_gates.clone());
        let project_id = xai_grok_science::project::ProjectId(context.project_id.0.clone());
        let project = project_store.load_project(&project_id)?;
        validate_kernel_project_binding(&project, &context)?;

        if !request.interpreter_path.is_absolute() {
            return Err(ScienceError::Invalid(
                "interpreter path must be absolute; a kernel is never resolved from PATH".into(),
            ));
        }
        let interpreter = dunce::canonicalize(&request.interpreter_path)?;
        if !std::fs::metadata(&interpreter)?.is_file() {
            return Err(ScienceError::Invalid(
                "kernel interpreter must be a regular file".into(),
            ));
        }
        request.interpreter_path = interpreter.clone();

        if let Some(allowed_root) = request.allowed_root.as_ref() {
            if !allowed_root.is_absolute() {
                return Err(ScienceError::Invalid(
                    "kernel allowed root must be absolute".into(),
                ));
            }
            let allowed_root = dunce::canonicalize(allowed_root)?;
            if !interpreter.starts_with(&allowed_root) {
                return Err(ScienceError::Invalid(
                    "kernel interpreter is outside its allowed root".into(),
                ));
            }
            request.allowed_root = Some(allowed_root);
        }

        if let Some(lock_path) = request.package_lock_path.as_ref() {
            if !lock_path.is_absolute() {
                return Err(ScienceError::Invalid(
                    "package lock path must be absolute".into(),
                ));
            }
            let lock_path = dunce::canonicalize(lock_path)?;
            if !std::fs::metadata(&lock_path)?.is_file() || !lock_path.starts_with(&actor_workspace)
            {
                return Err(ScienceError::Invalid(
                    "package lock must be a regular file inside the SessionActor workspace".into(),
                ));
            }
            request.package_lock_path = Some(lock_path);
        }

        request.admitted_by = format!("SessionActor:{}", self.session_info.id.0);
        let target = format!(
            "{} ({})",
            request.kernel_id,
            request.interpreter_path.display()
        );
        let ticket = xai_grok_science::workflow::begin_kernel_admission(&store, context)?;
        Ok(PreparedScienceKernelAdmission {
            store,
            ticket,
            request,
            target,
        })
    }

    /// Persist the permission terminal and execute only after durable Allow.
    pub(super) fn finish_science_kernel_admission(
        &self,
        prepared: PreparedScienceKernelAdmission,
        decision: xai_grok_science::ApprovalDecision,
        reason: String,
    ) -> xai_grok_science::Result<xai_grok_science::workflow::KernelAdmissionResult> {
        if decision != xai_grok_science::ApprovalDecision::Allow {
            let terminal = xai_grok_science::csv::finish_without_execution(
                &prepared.store,
                &prepared.ticket,
                decision,
                reason,
            )?;
            return Err(xai_grok_science::ScienceError::Invalid(format!(
                "science run {} finished {:?}",
                prepared.ticket.run_id.0, terminal.state
            )));
        }
        xai_grok_science::csv::mark_allowed(&prepared.store, &prepared.ticket)?;
        xai_grok_science::workflow::finish_kernel_admission(
            &prepared.store,
            prepared.ticket,
            &prepared.request,
        )
    }

    // ── LS5-K8 workflow execution ─────────────────────────────────

    /// Phase one inside the sole session actor: bind the execution to this
    /// session and open the durable run its approval will finish.
    ///
    /// NOTHING is executed here and no kernel is probed — a probe runs the
    /// interpreter, which is work a caller has not yet been permitted to do.
    /// The only writes are the durable run record and its pending approval,
    /// which exist precisely so that a deny has something to close.
    ///
    /// An operation id that already ran short-circuits: it returns the recorded
    /// report without opening a run, without prompting again, and without
    /// spawning anything. That is what makes a retry a retry rather than a
    /// second execution.
    pub(super) fn prepare_science_workflow_execution(
        &self,
        store: xai_grok_science::ScienceStore,
        mut context: xai_grok_science::RunContext,
        mut binding: crate::session::commands::ScienceWorkflowBinding,
    ) -> xai_grok_science::Result<PreparedScienceWorkflowExecution> {
        use xai_grok_science::ScienceError;

        // Session binding: the actor only executes on behalf of its own session.
        if binding.execution.session_id != self.session_info.id.0.as_ref()
            || context.session_id != self.session_info.id.0.as_ref()
        {
            return Err(ScienceError::Invalid(
                "workflow execution session does not match this SessionActor".into(),
            ));
        }
        if binding.execution.owner_id != context.owner_id {
            return Err(ScienceError::Invalid(
                "workflow execution owner does not match its run context".into(),
            ));
        }
        if binding.execution.owner_id.is_empty() {
            return Err(ScienceError::Invalid(
                "workflow execution requires an owner id".into(),
            ));
        }
        self.science_feature_gates.require_all(&[
            xai_grok_science::features::ScienceFeature::WorkflowDag,
            xai_grok_science::features::ScienceFeature::ComputeEnvironment,
        ])?;
        if binding
            .execution
            .spec
            .steps
            .iter()
            .any(|step| step.kind == xai_grok_science::workflow::StepKind::NotebookCell)
        {
            self.science_feature_gates
                .require(xai_grok_science::features::ScienceFeature::MultiKernel)?;
        }
        if !binding.interpreter_path.is_absolute() {
            return Err(ScienceError::Invalid(
                "interpreter path must be absolute; a kernel is never resolved from PATH".into(),
            ));
        }
        binding.interpreter_path =
            dunce::canonicalize(&binding.interpreter_path).map_err(|error| {
                ScienceError::Invalid(format!("cannot resolve workflow interpreter: {error}"))
            })?;
        if !std::fs::metadata(&binding.interpreter_path)?.is_file() {
            return Err(ScienceError::Invalid(
                "workflow interpreter must be a regular file".into(),
            ));
        }
        validate_project_mutation_actor_roots(
            std::path::Path::new(&self.session_info.cwd),
            &store,
            &binding.executor_root,
            &context,
        )?;
        if context.project_id.0.trim().is_empty()
            || binding.execution.spec.project_id.0.trim().is_empty()
            || context.project_id.0 != binding.execution.spec.project_id.0
        {
            return Err(ScienceError::Invalid(
                "workflow project does not match its run context".into(),
            ));
        }
        let project_store = xai_grok_science::project::ProjectStore::new_confined(
            &binding.executor_root,
            std::path::Path::new(&self.session_info.cwd),
        )?
        .with_gates(self.science_feature_gates.clone());
        if !store.shares_root_capability_with(&project_store)? {
            return Err(ScienceError::Invalid(
                "workflow project and ScienceStore retained different root capabilities".into(),
            ));
        }
        let project_revision = project_store.with_owned_project_revision(
            &binding.execution.spec.project_id,
            &binding.execution.owner_id,
            |_project, revision| Ok(revision.to_owned()),
        )?;

        // Structural spec faults are pure to detect, so detect them before the
        // user is asked to approve a run that could never have executed.
        binding
            .execution
            .spec
            .validate_dag()
            .map_err(ScienceError::Invalid)?;
        binding
            .execution
            .spec
            .topological_order()
            .map_err(ScienceError::Invalid)?;

        // Idempotent replay. The executor is the authority on what a replay
        // returns, so ask it — with no runner bound, so that even if this were
        // somehow not a replay nothing could execute behind the caller's back.
        let io = xai_grok_science::workflow::WorkflowIoCapability::open_existing_confined(
            &binding.executor_root,
            std::path::Path::new(&self.session_info.cwd),
        )?;
        if !store.shares_root_capability_with_workflow_io(&io)? {
            return Err(ScienceError::Invalid(
                "workflow I/O and ScienceStore retained different root capabilities".into(),
            ));
        }
        let executable = std::sync::Arc::new(
            xai_grok_science::workflow::PinnedExecutable::pin(&binding.interpreter_path).map_err(
                |error| {
                    ScienceError::Invalid(format!(
                        "cannot retain workflow interpreter bytes: {error}"
                    ))
                },
            )?,
        );
        let target = workflow_permission_target(&binding, &executable);
        if let Some(supplied) = context.environment.get(WORKFLOW_OPERATION_ID_ENV)
            && supplied != &binding.execution.operation_id
        {
            return Err(ScienceError::Invalid(
                "workflow context supplied a conflicting operation id".into(),
            ));
        }
        context.environment.insert(
            WORKFLOW_OPERATION_ID_ENV.into(),
            binding.execution.operation_id.clone(),
        );
        let admission_sha256 =
            workflow_admission_sha256(&context, &binding, &executable, &project_revision, &target)?;
        if let Some(supplied) = context.environment.get(WORKFLOW_ADMISSION_SHA256_ENV)
            && supplied != &admission_sha256
        {
            return Err(ScienceError::Invalid(
                "workflow context supplied a conflicting admission digest".into(),
            ));
        }
        context
            .environment
            .insert(WORKFLOW_ADMISSION_SHA256_ENV.into(), admission_sha256);
        let ledger = xai_grok_science::workflow::WorkflowExecutor::from_io(
            &binding.executor_root,
            &io,
            workflow_compute_environment(&binding, Some(executable.sha256())),
        )
        .with_policy(workflow_execution_policy(&binding));
        let authority =
            prepare_or_recover_workflow_authority(&store, &context, &binding, &io, &ledger)?;
        let (ticket, replayed, resume_allowed) = match authority {
            WorkflowAuthorityPreparation::AwaitPermission(ticket) => (ticket, None, false),
            WorkflowAuthorityPreparation::ResumeAllowed(ticket) => (ticket, None, true),
            WorkflowAuthorityPreparation::Replay { ticket, report } => {
                (ticket, Some(*report), false)
            }
        };
        Ok(PreparedScienceWorkflowExecution {
            store,
            project_store,
            project_revision,
            ticket,
            expected_context: context,
            binding,
            io,
            executor: ledger,
            executable,
            target,
            replayed,
            resume_allowed,
            permission_grant: None,
        })
    }

    /// Phase two: build the executor and run the workflow, but ONLY on an allow
    /// decision.
    ///
    /// Everything that touches the filesystem or spawns a process lives on this
    /// side of the gate — staging cell sources, probing the pinned kernel, and
    /// the run itself. The exec-loop bytes are compiled into the Rust binary
    /// and passed with `python -c`; there is no runtime file to swap. A denied,
    /// cancelled or
    /// timed-out request therefore leaves no execution record, no attempt and
    /// no artifact commit; only the closed Science run that says it was refused.
    ///
    /// This blocks the actor for the duration of the run, exactly as
    /// `execute_science_ssh_scp_transport` does: the point of routing here is
    /// that the actor holds execution authority, and handing the work to some
    /// other task would give that authority away again.
    pub(super) fn finish_science_workflow_execution(
        &self,
        prepared: PreparedScienceWorkflowExecution,
        decision: xai_grok_science::ApprovalDecision,
        reason: String,
    ) -> xai_grok_science::Result<xai_grok_science::workflow::WorkflowRunReport> {
        use xai_grok_science::ScienceError;
        use xai_grok_science::workflow::{
            AdmissionStatus, KernelAdmissionRequest, KernelManifest, PythonLoopRunner, StepKind,
            probe_pinned_kernel,
        };

        if prepared.replayed.is_some() && prepared.resume_allowed {
            return Err(ScienceError::Invalid(
                "workflow preparation cannot be both replay and internal recovery".into(),
            ));
        }
        if prepared.replayed.is_some() {
            if decision != xai_grok_science::ApprovalDecision::Allow
                || prepared.permission_grant.is_some()
            {
                return Err(ScienceError::Invalid(
                    "workflow replay must not carry a new permission grant or decision".into(),
                ));
            }
        } else if prepared.resume_allowed {
            if decision != xai_grok_science::ApprovalDecision::Allow
                || prepared.permission_grant.is_some()
            {
                return Err(ScienceError::Invalid(
                    "workflow internal recovery requires its original durable Allow".into(),
                ));
            }
            validate_running_allowed_workflow_authority(
                &prepared.store,
                &prepared.ticket,
                &prepared.binding,
                &prepared.io,
                &prepared.expected_context,
                &prepared.executor,
            )?;
        } else {
            if decision == xai_grok_science::ApprovalDecision::Allow {
                require_exact_workflow_allow_grant(&prepared)?;
            } else {
                validate_pending_workflow_authority(
                    &prepared.store,
                    &prepared.ticket,
                    &prepared.binding,
                    &prepared.io,
                    &prepared.expected_context,
                )?;
            }
            if decision != xai_grok_science::ApprovalDecision::Allow
                && prepared.permission_grant.is_some()
            {
                let terminal = finish_unexecuted_workflow_authority(
                    &prepared.store,
                    &prepared.ticket,
                    &prepared.binding,
                    &prepared.io,
                    &prepared.expected_context,
                    xai_grok_science::ApprovalDecision::Deny,
                    "non-Allow workflow carried an actor Allow grant".into(),
                )?;
                return Err(ScienceError::Invalid(format!(
                    "science run {} finished {:?}: inconsistent permission grant",
                    prepared.ticket.run_id.0, terminal.state
                )));
            }
        }

        let PreparedScienceWorkflowExecution {
            store,
            project_store,
            project_revision,
            ticket,
            expected_context,
            binding,
            io,
            executor,
            executable,
            replayed,
            resume_allowed,
            ..
        } = prepared;
        let project_id = binding.execution.spec.project_id.clone();

        if let Some(report) = replayed {
            if !store.shares_root_capability_with(&project_store)? {
                return Err(ScienceError::Ownership);
            }
            return project_store.with_owned_project_revision_guarded(
                &project_id,
                &ticket.owner_id,
                |_project, revision, held| {
                    if revision != project_revision {
                        return Err(ScienceError::Invalid(
                            "project changed while workflow replay was being admitted".into(),
                        ));
                    }
                    executor.with_project_guard(held, |executor| {
                        if let Err(error) = finalize_workflow_authority(
                            &store,
                            &ticket,
                            &binding,
                            &io,
                            &expected_context,
                            executor,
                            &report,
                        ) {
                            return Err(fail_workflow_authority_run(
                                &store,
                                &ticket,
                                workflow_authority_paths(&report).unwrap_or_default(),
                                error,
                            ));
                        }
                        Ok(report)
                    })
                },
            );
        }
        if decision != xai_grok_science::ApprovalDecision::Allow {
            let terminal = finish_unexecuted_workflow_authority(
                &store,
                &ticket,
                &binding,
                &io,
                &expected_context,
                decision,
                reason,
            )?;
            return Err(ScienceError::Invalid(format!(
                "science run {} finished {:?}",
                ticket.run_id.0, terminal.state
            )));
        }
        if resume_allowed {
            validate_running_allowed_workflow_authority(
                &store,
                &ticket,
                &binding,
                &io,
                &expected_context,
                &executor,
            )?;
        } else {
            xai_grok_science::csv::mark_allowed(&store, &ticket)?;
            validate_running_allowed_workflow_authority(
                &store,
                &ticket,
                &binding,
                &io,
                &expected_context,
                &executor,
            )?;
        }

        let failed = |error: ScienceError| -> ScienceError {
            let _ = xai_grok_science::csv::fail_running(
                &store,
                &ticket,
                format!("workflow execution rejected: {error}"),
            );
            error
        };

        let result = project_store.with_owned_project_revision_guarded(
            &project_id,
            &ticket.owner_id,
            |_project, revision, held| {
                if revision != project_revision {
                    return Err(ScienceError::Invalid(
                        "project changed while workflow approval was pending".into(),
                    ));
                }

                // Stage every cell body the spec carries into the
                // content-addressed store through the exact root retained
                // before permission.
                validate_running_allowed_workflow_authority(
                    &store,
                    &ticket,
                    &binding,
                    &io,
                    &expected_context,
                    &executor,
                )?;
                for step in &binding.execution.spec.steps {
                    if step.kind != StepKind::NotebookCell {
                        continue;
                    }
                    let Some(source) = step.notebook_cell.as_ref() else {
                        continue;
                    };
                    let digest = format!("{:x}", sha2::Sha256::digest(source.as_bytes()));
                    if let Err(error) = io.stage_cell(&digest, source.as_bytes()) {
                        return Err(failed(ScienceError::Invalid(format!(
                            "cannot stage the source of step '{}': {error}",
                            step.step_id
                        ))));
                    }
                }

                // Probe the interpreter. This RUNS it, which is why it is here
                // and not in `prepare_*`.
                let admission = probe_pinned_kernel(
                    &KernelAdmissionRequest::new(
                        binding.kernel_id.clone(),
                        binding.kernel_kind,
                        binding.interpreter_path.clone(),
                    )
                    .with_admitted_by(format!("session-actor:{}", self.session_info.id.0))
                    .with_probe_timeout(binding.probe_timeout),
                    &executable,
                )
                .map_err(&failed)?;
                if admission.admission_status != AdmissionStatus::Admitted {
                    return Err(failed(ScienceError::Invalid(format!(
                        "kernel '{}' was not admitted ({:?}); no step may run on it",
                        admission.kernel_id, admission.admission_status
                    ))));
                }

                let runner = PythonLoopRunner::new(io.share(), executable);
                let executor = executor
                    .with_runner(std::sync::Arc::new(runner))
                    .with_kernels(KernelManifest {
                        kernels: vec![admission],
                        default_python: None,
                        default_r: None,
                        default_julia: None,
                    })
                    .map_err(&failed)?;

                executor.with_project_guard(held, |executor| {
                    validate_running_allowed_workflow_authority(
                        &store,
                        &ticket,
                        &binding,
                        &io,
                        &expected_context,
                        executor,
                    )?;
                    let report = executor.execute(&binding.execution).map_err(&failed)?;
                    if let Err(error) = finalize_workflow_authority(
                        &store,
                        &ticket,
                        &binding,
                        &io,
                        &expected_context,
                        executor,
                        &report,
                    ) {
                        return Err(fail_workflow_authority_run(
                            &store,
                            &ticket,
                            workflow_authority_paths(&report).unwrap_or_default(),
                            error,
                        ));
                    }
                    Ok(report)
                })
            },
        );
        if let Err(error) = &result
            && store
                .load_run(&ticket.run_id)
                .is_ok_and(|run| run.state == xai_grok_science::RunState::Running)
        {
            let _ = xai_grok_science::csv::fail_running(
                &store,
                &ticket,
                format!("workflow execution rejected: {error}"),
            );
        }
        result
    }

    /// Close a durable Begin whose response receiver disappeared before the
    /// prepared capability bundle could be delivered.
    ///
    /// The actor loop calls this only after `oneshot::Sender::send` returns the
    /// undelivered value. A replay did not create a pending Science run and
    /// needs no terminalization; a fresh Begin reaches Interrupted without
    /// probing or running the retained executable.
    pub(super) fn interrupt_undelivered_science_workflow_begin(
        &self,
        prepared: PreparedScienceWorkflowExecution,
    ) -> xai_grok_science::Result<()> {
        if prepared.replayed.is_some() || prepared.resume_allowed {
            return Ok(());
        }
        let terminal = finish_unexecuted_workflow_authority(
            &prepared.store,
            &prepared.ticket,
            &prepared.binding,
            &prepared.io,
            &prepared.expected_context,
            xai_grok_science::ApprovalDecision::Interrupted,
            "workflow Begin response receiver closed before delivery".into(),
        )?;
        if terminal.state != xai_grok_science::RunState::Interrupted {
            return Err(xai_grok_science::ScienceError::Invalid(
                "undelivered workflow Begin did not reach Interrupted".into(),
            ));
        }
        Ok(())
    }
}

enum WorkflowAuthorityPreparation {
    AwaitPermission(xai_grok_science::csv::ScienceRunTicket),
    ResumeAllowed(xai_grok_science::csv::ScienceRunTicket),
    Replay {
        ticket: xai_grok_science::csv::ScienceRunTicket,
        report: Box<xai_grok_science::workflow::WorkflowRunReport>,
    },
}

fn workflow_ticket(
    context: &xai_grok_science::RunContext,
) -> xai_grok_science::Result<xai_grok_science::csv::ScienceRunTicket> {
    let expected_run_id = xai_grok_science::workflow::run_id_for_operation(
        context
            .environment
            .get(WORKFLOW_OPERATION_ID_ENV)
            .ok_or_else(|| {
                xai_grok_science::ScienceError::Invalid(
                    "workflow context is missing its operation id".into(),
                )
            })?,
    )?;
    if context.run_id.0 != expected_run_id {
        return Err(xai_grok_science::ScienceError::Ownership);
    }
    Ok(xai_grok_science::csv::ScienceRunTicket {
        project_id: context.project_id.clone(),
        run_id: context.run_id.clone(),
        owner_id: context.owner_id.clone(),
        call_id: xai_grok_science::CallId::new("science_workflow_execute"),
    })
}

fn workflow_admission_sha256(
    context: &xai_grok_science::RunContext,
    binding: &crate::session::commands::ScienceWorkflowBinding,
    executable: &xai_grok_science::workflow::PinnedExecutable,
    project_revision: &str,
    target: &str,
) -> xai_grok_science::Result<String> {
    let admission = serde_json::json!({
        "schema": "lumen.science.workflow.admission.v1",
        "authority": {
            "run_id": context.run_id.0,
            "project_id": context.project_id.0,
            "session_id": context.session_id,
            "owner_id": context.owner_id,
            "workspace_root": context.workspace_root,
            "artifact_root": context.artifact_root,
            "executor_root": binding.executor_root,
            "project_revision": project_revision,
        },
        "execution": {
            "operation_id": binding.execution.operation_id,
            "session_id": binding.execution.session_id,
            "owner_id": binding.execution.owner_id,
            "spec": binding.execution.spec,
        },
        "interpreter": {
            "canonical_path": binding.interpreter_path,
            "sha256": executable.sha256(),
            "backend": executable.backend().to_string(),
        },
        "kernel": {
            "kernel_id": binding.kernel_id,
            "kernel_kind": binding.kernel_kind,
            "probe_timeout_ms": binding.probe_timeout.as_millis(),
            "allow_kernel_steps": binding.allow_kernel_steps,
        },
        "compute_environment": workflow_compute_environment(
            binding,
            Some(executable.sha256()),
        ),
        "execution_policy": workflow_execution_policy(binding),
        "permission_target": target,
    });
    Ok(format!(
        "{:x}",
        sha2::Sha256::digest(serde_json::to_vec(&admission)?)
    ))
}

fn workflow_begin_event_payload(
    context: &xai_grok_science::RunContext,
    binding: &crate::session::commands::ScienceWorkflowBinding,
) -> xai_grok_science::Result<serde_json::Value> {
    let admission_sha256 = context
        .environment
        .get(WORKFLOW_ADMISSION_SHA256_ENV)
        .ok_or_else(|| {
            xai_grok_science::ScienceError::Invalid(
                "workflow context is missing its admission digest".into(),
            )
        })?;
    Ok(serde_json::json!({
        "workflow_id": binding.execution.spec.workflow_id,
        "operation_id": binding.execution.operation_id,
        "steps": binding.execution.spec.steps.len(),
        "allow_kernel_steps": binding.allow_kernel_steps,
        "interpreter": binding.interpreter_path.display().to_string(),
        "admission_sha256": admission_sha256,
    }))
}

fn workflow_events(
    store: &xai_grok_science::ScienceStore,
    run_id: &xai_grok_science::RunId,
) -> xai_grok_science::Result<Vec<xai_grok_science::Event>> {
    let events = store.events_after(run_id, 0, 1_000)?;
    if events.len() == 1_000 {
        return Err(xai_grok_science::ScienceError::Invalid(
            "workflow authority event log is too large to recover safely".into(),
        ));
    }
    Ok(events)
}

type ExpectedWorkflowEvent = (&'static str, &'static str, serde_json::Value);

fn workflow_begin_expected(
    context: &xai_grok_science::RunContext,
    binding: &crate::session::commands::ScienceWorkflowBinding,
) -> xai_grok_science::Result<ExpectedWorkflowEvent> {
    Ok((
        "SessionActor",
        "run.created",
        workflow_begin_event_payload(context, binding)?,
    ))
}

fn workflow_allowed_expected(
    ticket: &xai_grok_science::csv::ScienceRunTicket,
) -> ExpectedWorkflowEvent {
    (
        "LumenApproval",
        "approval.allowed",
        serde_json::json!({"call_id": ticket.call_id.0}),
    )
}

fn workflow_events_match_exactly(
    events: &[xai_grok_science::Event],
    ticket: &xai_grok_science::csv::ScienceRunTicket,
    expected: &[ExpectedWorkflowEvent],
) -> bool {
    events.len() == expected.len()
        && events.iter().zip(expected).enumerate().all(
            |(index, (event, (actor, kind, payload)))| {
                event.schema_version == xai_grok_science::SCHEMA_VERSION
                    && event.run_id == ticket.run_id
                    && event.seq == u64::try_from(index + 1).unwrap_or(0)
                    && event.actor == *actor
                    && event.kind == *kind
                    && event.payload == *payload
            },
        )
}

fn require_exact_workflow_events(
    store: &xai_grok_science::ScienceStore,
    ticket: &xai_grok_science::csv::ScienceRunTicket,
    expected: &[ExpectedWorkflowEvent],
    phase: &str,
) -> xai_grok_science::Result<()> {
    let events = workflow_events(store, &ticket.run_id)?;
    if workflow_events_match_exactly(&events, ticket, expected) {
        return Ok(());
    }
    Err(xai_grok_science::ScienceError::Invalid(format!(
        "workflow {phase} requires exactly [{}] with canonical actor/payload/order",
        expected
            .iter()
            .map(|(_, kind, _)| *kind)
            .collect::<Vec<_>>()
            .join(", ")
    )))
}

fn ensure_exact_next_workflow_event(
    store: &xai_grok_science::ScienceStore,
    ticket: &xai_grok_science::csv::ScienceRunTicket,
    prefix: &[ExpectedWorkflowEvent],
    next: ExpectedWorkflowEvent,
    phase: &str,
) -> xai_grok_science::Result<()> {
    let events = workflow_events(store, &ticket.run_id)?;
    let mut complete = prefix.to_vec();
    complete.push(next.clone());
    if workflow_events_match_exactly(&events, ticket, &complete) {
        return Ok(());
    }
    if !workflow_events_match_exactly(&events, ticket, prefix) {
        return Err(xai_grok_science::ScienceError::Invalid(format!(
            "workflow {phase} has an unknown, duplicate, or out-of-order event prefix"
        )));
    }
    store.append_recoverable_commit_event(&ticket.run_id, next.0, next.1, next.2)?;
    require_exact_workflow_events(store, ticket, &complete, phase)
}

fn require_workflow_finish_shape(
    store: &xai_grok_science::ScienceStore,
    ticket: &xai_grok_science::csv::ScienceRunTicket,
    begin: ExpectedWorkflowEvent,
) -> xai_grok_science::Result<()> {
    let events = workflow_events(store, &ticket.run_id)?;
    let allowed = workflow_allowed_expected(ticket);
    if events.len() != 3
        || !workflow_events_match_exactly(&events[..2], ticket, &[begin, allowed])
        || events[2].schema_version != xai_grok_science::SCHEMA_VERSION
        || events[2].run_id != ticket.run_id
        || events[2].seq != 3
        || events[2].actor != "SessionActor"
        || events[2].kind != "workflow.execution.finished"
    {
        return Err(xai_grok_science::ScienceError::Invalid(
            "terminal workflow requires exactly [run.created, approval.allowed, workflow.execution.finished]"
                .into(),
        ));
    }
    Ok(())
}

fn exact_workflow_approval(
    store: &xai_grok_science::ScienceStore,
    ticket: &xai_grok_science::csv::ScienceRunTicket,
) -> xai_grok_science::Result<Option<xai_grok_science::Approval>> {
    let approvals = store.approvals(&ticket.run_id)?;
    let approval = match approvals.as_slice() {
        [] => return Ok(None),
        [approval] => approval,
        _ => {
            return Err(xai_grok_science::ScienceError::Invalid(
                "workflow authority requires at most one approval during recovery".into(),
            ));
        }
    };
    if approval.project_id != ticket.project_id
        || approval.run_id != ticket.run_id
        || approval.owner_id != ticket.owner_id
        || approval.call_id != ticket.call_id
    {
        return Err(xai_grok_science::ScienceError::Ownership);
    }
    if (approval.decision == xai_grok_science::ApprovalDecision::Pending)
        != approval.decided_at.is_none()
    {
        return Err(xai_grok_science::ScienceError::Invalid(
            "workflow approval decision and timestamp are inconsistent".into(),
        ));
    }
    Ok(Some(approval.clone()))
}

fn validate_exact_workflow_context(
    run: &xai_grok_science::RunRecord,
    expected: &xai_grok_science::RunContext,
) -> xai_grok_science::Result<()> {
    if run.context.run_id != expected.run_id
        || run.context.project_id != expected.project_id
        || run.context.session_id != expected.session_id
        || run.context.owner_id != expected.owner_id
        || run.context.workspace_root != expected.workspace_root
        || run.context.artifact_root != expected.artifact_root
    {
        return Err(xai_grok_science::ScienceError::Ownership);
    }
    if run.context != *expected {
        return Err(xai_grok_science::ScienceError::Invalid(
            "workflow durable context or admission digest differs from this request".into(),
        ));
    }
    Ok(())
}

fn ensure_empty_workflow_authority_outputs(
    store: &xai_grok_science::ScienceStore,
    run_id: &xai_grok_science::RunId,
) -> xai_grok_science::Result<()> {
    if !store.artifacts(run_id)?.is_empty()
        || !store.evidence(run_id)?.is_empty()
        || !store.provenance(run_id)?.is_empty()
        || !store.previews(run_id)?.is_empty()
    {
        return Err(xai_grok_science::ScienceError::Invalid(
            "workflow authority has outputs without a workflow commit ledger".into(),
        ));
    }
    Ok(())
}

fn ensure_workflow_allowed_event(
    store: &xai_grok_science::ScienceStore,
    ticket: &xai_grok_science::csv::ScienceRunTicket,
    context: &xai_grok_science::RunContext,
    binding: &crate::session::commands::ScienceWorkflowBinding,
) -> xai_grok_science::Result<()> {
    let begin = workflow_begin_expected(context, binding)?;
    ensure_exact_next_workflow_event(
        store,
        ticket,
        &[begin],
        workflow_allowed_expected(ticket),
        "Allow recovery",
    )
}

fn require_workflow_allowed_event(
    store: &xai_grok_science::ScienceStore,
    ticket: &xai_grok_science::csv::ScienceRunTicket,
    context: &xai_grok_science::RunContext,
    binding: &crate::session::commands::ScienceWorkflowBinding,
) -> xai_grok_science::Result<()> {
    require_exact_workflow_events(
        store,
        ticket,
        &[
            workflow_begin_expected(context, binding)?,
            workflow_allowed_expected(ticket),
        ],
        "Running+Allow recovery",
    )
}

fn workflow_terminal_state_and_kind(
    decision: xai_grok_science::ApprovalDecision,
) -> xai_grok_science::Result<(xai_grok_science::RunState, &'static str)> {
    use xai_grok_science::{ApprovalDecision, RunState, ScienceError};

    match decision {
        ApprovalDecision::Deny => Ok((RunState::Denied, "approval.denied")),
        ApprovalDecision::Timeout => Ok((RunState::TimedOut, "approval.timed_out")),
        ApprovalDecision::Cancel => Ok((RunState::Cancelled, "approval.cancelled")),
        ApprovalDecision::Interrupted => Ok((RunState::Interrupted, "approval.interrupted")),
        _ => Err(ScienceError::Invalid(
            "workflow terminal recovery requires a non-Allow decision".into(),
        )),
    }
}

fn workflow_terminal_expected(
    ticket: &xai_grok_science::csv::ScienceRunTicket,
    kind: &'static str,
    reason: &str,
) -> ExpectedWorkflowEvent {
    (
        "LumenApproval",
        kind,
        serde_json::json!({
            "call_id": ticket.call_id.0,
            "reason": reason,
        }),
    )
}

fn ensure_or_require_workflow_terminal_event(
    store: &xai_grok_science::ScienceStore,
    ticket: &xai_grok_science::csv::ScienceRunTicket,
    context: &xai_grok_science::RunContext,
    binding: &crate::session::commands::ScienceWorkflowBinding,
    decision: xai_grok_science::ApprovalDecision,
    fallback_reason: &str,
    allow_append: bool,
) -> xai_grok_science::Result<String> {
    let (_, kind) = workflow_terminal_state_and_kind(decision)?;
    let begin = workflow_begin_expected(context, binding)?;
    let events = workflow_events(store, &ticket.run_id)?;
    if workflow_events_match_exactly(&events, ticket, std::slice::from_ref(&begin)) {
        if !allow_append {
            return Err(xai_grok_science::ScienceError::Invalid(
                "terminal workflow authority is missing its terminal approval event".into(),
            ));
        }
        let terminal = workflow_terminal_expected(ticket, kind, fallback_reason);
        store.append_recoverable_commit_event(
            &ticket.run_id,
            terminal.0,
            terminal.1,
            terminal.2,
        )?;
        require_exact_workflow_events(
            store,
            ticket,
            &[
                begin,
                workflow_terminal_expected(ticket, kind, fallback_reason),
            ],
            "terminal decision",
        )?;
        return Ok(fallback_reason.to_owned());
    }
    if events.len() != 2
        || !workflow_events_match_exactly(&events[..1], ticket, std::slice::from_ref(&begin))
        || events[1].schema_version != xai_grok_science::SCHEMA_VERSION
        || events[1].run_id != ticket.run_id
        || events[1].seq != 2
        || events[1].actor != "LumenApproval"
        || events[1].kind != kind
        || events[1].payload.get("call_id")
            != Some(&serde_json::Value::String(ticket.call_id.0.clone()))
    {
        return Err(xai_grok_science::ScienceError::Invalid(
            "workflow terminal authority has an unknown, duplicate, or out-of-order event".into(),
        ));
    }
    let reason = events[1]
        .payload
        .get("reason")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            xai_grok_science::ScienceError::Invalid(
                "workflow terminal approval event is missing its reason".into(),
            )
        })?
        .to_owned();
    require_exact_workflow_events(
        store,
        ticket,
        &[begin, workflow_terminal_expected(ticket, kind, &reason)],
        "terminal decision",
    )?;
    Ok(reason)
}

fn recover_workflow_terminal_decision(
    store: &xai_grok_science::ScienceStore,
    ticket: &xai_grok_science::csv::ScienceRunTicket,
    context: &xai_grok_science::RunContext,
    binding: &crate::session::commands::ScienceWorkflowBinding,
    decision: xai_grok_science::ApprovalDecision,
) -> xai_grok_science::Result<xai_grok_science::RunState> {
    ensure_empty_workflow_authority_outputs(store, &ticket.run_id)?;
    let (state, _) = workflow_terminal_state_and_kind(decision.clone())?;
    let reason = ensure_or_require_workflow_terminal_event(
        store,
        ticket,
        context,
        binding,
        decision,
        "recovered durable workflow permission decision",
        true,
    )?;
    Ok(store.transition(&ticket.run_id, state, Some(reason))?.state)
}

fn workflow_terminal_result(
    ticket: &xai_grok_science::csv::ScienceRunTicket,
    state: xai_grok_science::RunState,
) -> xai_grok_science::Result<WorkflowAuthorityPreparation> {
    Err(xai_grok_science::ScienceError::Invalid(format!(
        "workflow authority {} is already terminal {state:?}",
        ticket.run_id.0
    )))
}

fn prepare_or_recover_workflow_authority(
    store: &xai_grok_science::ScienceStore,
    context: &xai_grok_science::RunContext,
    binding: &crate::session::commands::ScienceWorkflowBinding,
    io: &xai_grok_science::workflow::WorkflowIoCapability,
    executor: &xai_grok_science::workflow::WorkflowExecutor,
) -> xai_grok_science::Result<WorkflowAuthorityPreparation> {
    use xai_grok_science::{Approval, ApprovalDecision, RunState, ScienceError};

    if context.environment.get(WORKFLOW_OPERATION_ID_ENV) != Some(&binding.execution.operation_id) {
        return Err(ScienceError::Invalid(
            "workflow context operation id differs from its execution".into(),
        ));
    }
    let ticket = workflow_ticket(context)?;
    let operation = executor.lookup_operation(&binding.execution.operation_id)?;
    let authority = store.load_run_optional(&ticket.run_id)?;

    let Some(run) = authority else {
        if operation.is_some() {
            return Err(ScienceError::Invalid(
                "workflow ledger exists without its SessionActor authority run".into(),
            ));
        }
        let ticket = begin_workflow_execution_run(store, context.clone(), binding)?;
        return Ok(WorkflowAuthorityPreparation::AwaitPermission(ticket));
    };
    validate_exact_workflow_context(&run, context)?;
    let begin = workflow_begin_expected(context, binding)?;

    if run.state == RunState::Succeeded {
        let Some(_operation) = operation else {
            return Err(ScienceError::Invalid(
                "succeeded workflow authority is missing its workflow ledger".into(),
            ));
        };
        let Some(approval) = exact_workflow_approval(store, &ticket)? else {
            return Err(ScienceError::Invalid(
                "succeeded workflow authority is missing its Allow approval".into(),
            ));
        };
        if approval.decision != ApprovalDecision::Allow || approval.decided_at.is_none() {
            return Err(ScienceError::Invalid(
                "succeeded workflow authority is not bound to durable Allow".into(),
            ));
        }
        require_workflow_finish_shape(store, &ticket, begin)?;
        let report = executor.execute(&binding.execution)?;
        finalize_workflow_authority(store, &ticket, binding, io, context, executor, &report)?;
        return Ok(WorkflowAuthorityPreparation::Replay {
            ticket,
            report: Box::new(report),
        });
    }

    if run.state.terminal() {
        let approval = exact_workflow_approval(store, &ticket)?.ok_or_else(|| {
            ScienceError::Invalid("terminal workflow authority is missing its approval".into())
        })?;
        match run.state {
            RunState::Denied | RunState::TimedOut | RunState::Cancelled | RunState::Interrupted => {
                if operation.is_some() {
                    return Err(ScienceError::Invalid(
                        "non-executed workflow authority unexpectedly has a workflow ledger".into(),
                    ));
                }
                let (expected_state, _) =
                    workflow_terminal_state_and_kind(approval.decision.clone())?;
                if expected_state != run.state || approval.decided_at.is_none() {
                    return Err(ScienceError::Invalid(
                        "terminal workflow run and approval decision disagree".into(),
                    ));
                }
                let event_reason = ensure_or_require_workflow_terminal_event(
                    store,
                    &ticket,
                    context,
                    binding,
                    approval.decision,
                    "",
                    false,
                )?;
                if run.terminal_reason.as_deref() != Some(event_reason.as_str()) {
                    return Err(ScienceError::Invalid(
                        "terminal workflow reason differs from its durable event".into(),
                    ));
                }
            }
            RunState::Failed => {
                if approval.decision != ApprovalDecision::Allow || approval.decided_at.is_none() {
                    return Err(ScienceError::Invalid(
                        "failed workflow authority is not bound to durable Allow".into(),
                    ));
                }
                if operation.is_some() {
                    require_workflow_finish_shape(store, &ticket, begin)?;
                    let report = executor.execute(&binding.execution)?;
                    validate_workflow_finished_event(store, &ticket, binding, context, &report, 0)?;
                } else {
                    require_workflow_allowed_event(store, &ticket, context, binding)?;
                }
            }
            _ => unreachable!("Succeeded returned above"),
        }
        ensure_empty_workflow_authority_outputs(store, &ticket.run_id)?;
        return workflow_terminal_result(&ticket, run.state);
    }

    match run.state {
        RunState::Created => {
            if operation.is_some() {
                return Err(ScienceError::Invalid(
                    "workflow ledger was created before durable approval".into(),
                ));
            }
            ensure_exact_next_workflow_event(store, &ticket, &[], begin, "Created recovery")?;
            match exact_workflow_approval(store, &ticket)? {
                None => {
                    store.request_approval(Approval {
                        project_id: ticket.project_id.clone(),
                        run_id: ticket.run_id.clone(),
                        call_id: ticket.call_id.clone(),
                        owner_id: ticket.owner_id.clone(),
                        decision: ApprovalDecision::Pending,
                        decided_at: None,
                    })?;
                }
                Some(approval)
                    if approval.decision == ApprovalDecision::Pending
                        && approval.decided_at.is_none() => {}
                Some(_) => {
                    return Err(ScienceError::Invalid(
                        "Created workflow authority has a terminal approval".into(),
                    ));
                }
            }
            store.transition(&ticket.run_id, RunState::AwaitingApproval, None)?;
            Ok(WorkflowAuthorityPreparation::AwaitPermission(ticket))
        }
        RunState::AwaitingApproval => {
            let approval = exact_workflow_approval(store, &ticket)?.ok_or_else(|| {
                ScienceError::Invalid(
                    "AwaitingApproval workflow authority is missing its approval".into(),
                )
            })?;
            match approval.decision {
                ApprovalDecision::Pending if approval.decided_at.is_none() => {
                    require_exact_workflow_events(
                        store,
                        &ticket,
                        std::slice::from_ref(&begin),
                        "pending approval",
                    )?;
                    if operation.is_some() {
                        return Err(ScienceError::Invalid(
                            "workflow ledger was created before durable approval".into(),
                        ));
                    }
                    Ok(WorkflowAuthorityPreparation::AwaitPermission(ticket))
                }
                ApprovalDecision::Allow if approval.decided_at.is_some() => {
                    ensure_workflow_allowed_event(store, &ticket, context, binding)?;
                    store.transition(&ticket.run_id, RunState::Running, None)?;
                    if operation.is_some() {
                        let report = executor.execute(&binding.execution)?;
                        Ok(WorkflowAuthorityPreparation::Replay {
                            ticket,
                            report: Box::new(report),
                        })
                    } else {
                        ensure_empty_workflow_authority_outputs(store, &ticket.run_id)?;
                        Ok(WorkflowAuthorityPreparation::ResumeAllowed(ticket))
                    }
                }
                ApprovalDecision::Deny
                | ApprovalDecision::Timeout
                | ApprovalDecision::Cancel
                | ApprovalDecision::Interrupted
                    if approval.decided_at.is_some() =>
                {
                    if operation.is_some() {
                        return Err(ScienceError::Invalid(
                            "workflow ledger exists after a non-Allow decision".into(),
                        ));
                    }
                    let state = recover_workflow_terminal_decision(
                        store,
                        &ticket,
                        context,
                        binding,
                        approval.decision,
                    )?;
                    workflow_terminal_result(&ticket, state)
                }
                _ => Err(ScienceError::Invalid(
                    "workflow approval state cannot be recovered".into(),
                )),
            }
        }
        RunState::Running => {
            let approval = exact_workflow_approval(store, &ticket)?.ok_or_else(|| {
                ScienceError::Invalid("Running workflow authority is missing Allow".into())
            })?;
            if approval.decision != ApprovalDecision::Allow || approval.decided_at.is_none() {
                return Err(ScienceError::Invalid(
                    "Running workflow authority is not bound to durable Allow".into(),
                ));
            }
            if operation.is_some() {
                if require_workflow_allowed_event(store, &ticket, context, binding).is_err() {
                    require_workflow_finish_shape(
                        store,
                        &ticket,
                        workflow_begin_expected(context, binding)?,
                    )?;
                }
                let report = executor.execute(&binding.execution)?;
                Ok(WorkflowAuthorityPreparation::Replay {
                    ticket,
                    report: Box::new(report),
                })
            } else {
                require_workflow_allowed_event(store, &ticket, context, binding)?;
                ensure_empty_workflow_authority_outputs(store, &ticket.run_id)?;
                Ok(WorkflowAuthorityPreparation::ResumeAllowed(ticket))
            }
        }
        _ => unreachable!("terminal workflow states returned above"),
    }
}

#[derive(Debug)]
struct WorkflowAuthorityOutput {
    artifact: xai_grok_science::Artifact,
    evidence: xai_grok_science::Evidence,
    provenance: xai_grok_science::Provenance,
    bytes: Vec<u8>,
}

fn workflow_authority_path(
    commit: &xai_grok_science::workflow::ArtifactCommit,
    artifact_name: &str,
) -> xai_grok_science::Result<std::path::PathBuf> {
    let name = std::path::Path::new(artifact_name);
    if artifact_name.is_empty()
        || !name
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
    {
        return Err(xai_grok_science::ScienceError::Invalid(format!(
            "workflow artifact name '{artifact_name}' is not a confined relative path"
        )));
    }
    Ok(std::path::PathBuf::from("workflow")
        .join(&commit.step_id)
        .join(name))
}

fn workflow_artifact_mime(artifact_name: &str) -> &'static str {
    match std::path::Path::new(artifact_name)
        .extension()
        .and_then(std::ffi::OsStr::to_str)
    {
        Some("json") => "application/json",
        Some("md") => "text/markdown; charset=utf-8",
        Some("txt") | Some("csv") | Some("tsv") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn workflow_authority_paths(
    report: &xai_grok_science::workflow::WorkflowRunReport,
) -> xai_grok_science::Result<Vec<std::path::PathBuf>> {
    let mut paths = Vec::new();
    for commit in &report.commits {
        for artifact_name in commit.output_manifest.keys() {
            paths.push(workflow_authority_path(commit, artifact_name)?);
        }
    }
    paths.sort();
    if paths.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(xai_grok_science::ScienceError::Invalid(
            "workflow commits resolve to duplicate authority artifact paths".into(),
        ));
    }
    Ok(paths)
}

fn collect_workflow_authority_outputs(
    ticket: &xai_grok_science::csv::ScienceRunTicket,
    executor: &xai_grok_science::workflow::WorkflowExecutor,
    report: &xai_grok_science::workflow::WorkflowRunReport,
) -> xai_grok_science::Result<Vec<WorkflowAuthorityOutput>> {
    let mut outputs = Vec::new();
    for reported_commit in &report.commits {
        let commit = executor
            .load_commit(&reported_commit.commit_key)?
            .ok_or_else(|| {
                xai_grok_science::ScienceError::Invalid(format!(
                    "workflow commit {} disappeared before authority registration",
                    reported_commit.commit_key
                ))
            })?;
        if commit != *reported_commit {
            return Err(xai_grok_science::ScienceError::Invalid(
                "workflow report commit differs from the confined commit ledger".into(),
            ));
        }
        for (artifact_name, digest) in &commit.output_manifest {
            let relative_path = workflow_authority_path(&commit, artifact_name)?;
            let bytes = executor.committed_artifact_bytes(&commit.commit_key, artifact_name)?;
            let actual = format!("{:x}", sha2::Sha256::digest(&bytes));
            if actual != *digest {
                return Err(xai_grok_science::ScienceError::Invalid(format!(
                    "workflow artifact '{artifact_name}' changed before authority registration"
                )));
            }
            let preview = format!(
                "Workflow '{}' step '{}' output '{}'",
                report.run.workflow_id, commit.step_id, artifact_name
            );
            let artifact = xai_grok_science::Artifact {
                run_id: ticket.run_id.clone(),
                call_id: ticket.call_id.clone(),
                relative_path: relative_path.clone(),
                sha256: digest.clone(),
                bytes: bytes.len() as u64,
                mime: workflow_artifact_mime(artifact_name).into(),
                preview,
            };
            let evidence = xai_grok_science::Evidence {
                run_id: ticket.run_id.clone(),
                claim: format!(
                    "Workflow '{}' step '{}' committed byte-verified output '{}'.",
                    report.run.workflow_id, commit.step_id, artifact_name
                ),
                source: format!("lumen-workflow-cas:{}:{}", commit.commit_key, artifact_name),
                artifact_sha256: Some(digest.clone()),
                verified_at: commit.committed_at,
            };
            let provenance = xai_grok_science::Provenance {
                run_id: ticket.run_id.clone(),
                source_uri: format!("lumen-workflow-cas://{}/{}", commit.commit_key, digest),
                source_commit: Some(commit.commit_key.clone()),
                source_path: Some(relative_path.display().to_string()),
                license: "Lumen-Science-Derived-Output".into(),
                retrieved_at: commit.committed_at,
                input_sha256: digest.clone(),
                tool: "SessionActor/science_workflow_execute-v1".into(),
                environment: std::collections::BTreeMap::from([
                    ("operation_id".into(), report.run.operation_id.clone()),
                    ("workflow_id".into(), report.run.workflow_id.clone()),
                    ("step_id".into(), commit.step_id.clone()),
                    ("network".into(), "disabled".into()),
                ]),
            };
            outputs.push(WorkflowAuthorityOutput {
                artifact,
                evidence,
                provenance,
                bytes,
            });
        }
    }
    outputs.sort_by(|left, right| {
        left.artifact
            .relative_path
            .cmp(&right.artifact.relative_path)
    });
    if outputs
        .windows(2)
        .any(|pair| pair[0].artifact.relative_path == pair[1].artifact.relative_path)
    {
        return Err(xai_grok_science::ScienceError::Invalid(
            "workflow commits resolve to duplicate authority artifact paths".into(),
        ));
    }
    Ok(outputs)
}

/// Validate the private capability minted by the production permission bridge.
///
/// Any missing/mismatched grant is not merely an in-memory error: the actor
/// durably resolves the pending approval as Deny before returning, and no
/// workflow ledger, attempt, cell, output, or authority artifact is created.
fn require_exact_workflow_allow_grant(
    prepared: &PreparedScienceWorkflowExecution,
) -> xai_grok_science::Result<()> {
    if let Err(validation_error) = validate_pending_workflow_authority(
        &prepared.store,
        &prepared.ticket,
        &prepared.binding,
        &prepared.io,
        &prepared.expected_context,
    ) {
        return Err(fail_pending_workflow_authority_tamper(
            prepared,
            validation_error,
        ));
    }
    let exact_root = prepared
        .store
        .shares_root_capability_with(&prepared.project_store)?;
    let permission_granted = match prepared.permission_grant.as_ref() {
        Some(grant) => grant.authorizes(prepared)?,
        None => false,
    };
    if exact_root && permission_granted {
        return Ok(());
    }
    let terminal = finish_unexecuted_workflow_authority(
        &prepared.store,
        &prepared.ticket,
        &prepared.binding,
        &prepared.io,
        &prepared.expected_context,
        xai_grok_science::ApprovalDecision::Deny,
        "workflow Allow lacked the exact actor permission grant".into(),
    )?;
    Err(xai_grok_science::ScienceError::Invalid(format!(
        "science run {} finished {:?}: actor permission grant rejected",
        prepared.ticket.run_id.0, terminal.state
    )))
}

fn fail_pending_workflow_authority_tamper(
    prepared: &PreparedScienceWorkflowExecution,
    validation_error: xai_grok_science::ScienceError,
) -> xai_grok_science::ScienceError {
    let reason = format!("workflow pending authority changed before Allow: {validation_error}");
    let terminalized = (|| -> xai_grok_science::Result<()> {
        prepared.store.discard_pending_unauthorized_outputs(
            &prepared.ticket.project_id,
            &prepared.ticket.run_id,
            &prepared.ticket.owner_id,
            &prepared.ticket.call_id,
        )?;
        validate_pending_workflow_authority(
            &prepared.store,
            &prepared.ticket,
            &prepared.binding,
            &prepared.io,
            &prepared.expected_context,
        )?;
        prepared.store.decide_approval(
            &prepared.ticket.project_id,
            &prepared.ticket.run_id,
            &prepared.ticket.owner_id,
            &prepared.ticket.call_id,
            xai_grok_science::ApprovalDecision::Deny,
        )?;
        ensure_exact_next_workflow_event(
            &prepared.store,
            &prepared.ticket,
            &[workflow_begin_expected(
                &prepared.expected_context,
                &prepared.binding,
            )?],
            (
                "SessionActor",
                "workflow.authority.failed",
                serde_json::json!({
                    "call_id": prepared.ticket.call_id.0,
                    "reason": reason,
                }),
            ),
            "pending authority tamper",
        )?;
        prepared.store.transition(
            &prepared.ticket.run_id,
            xai_grok_science::RunState::Failed,
            Some(reason.clone()),
        )?;
        Ok(())
    })();
    match terminalized {
        Ok(()) => validation_error,
        Err(terminal_error) => xai_grok_science::ScienceError::Invalid(format!(
            "{validation_error}; pending authority tamper could not be terminalized: {terminal_error}"
        )),
    }
}

fn validate_running_allowed_workflow_authority(
    store: &xai_grok_science::ScienceStore,
    ticket: &xai_grok_science::csv::ScienceRunTicket,
    binding: &crate::session::commands::ScienceWorkflowBinding,
    io: &xai_grok_science::workflow::WorkflowIoCapability,
    expected_context: &xai_grok_science::RunContext,
    executor: &xai_grok_science::workflow::WorkflowExecutor,
) -> xai_grok_science::Result<()> {
    if store.root() != binding.executor_root.as_path()
        || !store.shares_root_capability_with_workflow_io(io)?
        || expected_context.run_id != ticket.run_id
        || expected_context.project_id != ticket.project_id
        || expected_context.owner_id != ticket.owner_id
        || expected_context.session_id != binding.execution.session_id
        || expected_context.artifact_root != binding.executor_root.join("runs")
        || expected_context.environment.get(WORKFLOW_OPERATION_ID_ENV)
            != Some(&binding.execution.operation_id)
        || xai_grok_science::workflow::run_id_for_operation(&binding.execution.operation_id)?
            != ticket.run_id.0
    {
        return Err(xai_grok_science::ScienceError::Ownership);
    }
    let run = store.load_run(&ticket.run_id)?;
    validate_exact_workflow_context(&run, expected_context)?;
    if run.state != xai_grok_science::RunState::Running {
        return Err(xai_grok_science::ScienceError::Invalid(
            "workflow internal recovery requires its original Running authority".into(),
        ));
    }
    let Some(approval) = exact_workflow_approval(store, ticket)? else {
        return Err(xai_grok_science::ScienceError::Invalid(
            "workflow internal recovery is missing its durable Allow".into(),
        ));
    };
    if approval.decision != xai_grok_science::ApprovalDecision::Allow
        || approval.decided_at.is_none()
    {
        return Err(xai_grok_science::ScienceError::Invalid(
            "workflow internal recovery is not bound to durable Allow".into(),
        ));
    }
    require_workflow_allowed_event(store, ticket, expected_context, binding)?;
    if executor
        .lookup_operation(&binding.execution.operation_id)?
        .is_some()
    {
        return Err(xai_grok_science::ScienceError::Invalid(
            "workflow execution ledger already exists before actor execution".into(),
        ));
    }
    ensure_empty_workflow_authority_outputs(store, &ticket.run_id)
}

fn validate_pending_workflow_authority(
    store: &xai_grok_science::ScienceStore,
    ticket: &xai_grok_science::csv::ScienceRunTicket,
    binding: &crate::session::commands::ScienceWorkflowBinding,
    io: &xai_grok_science::workflow::WorkflowIoCapability,
    expected_context: &xai_grok_science::RunContext,
) -> xai_grok_science::Result<()> {
    if store.root() != binding.executor_root.as_path()
        || !store.shares_root_capability_with_workflow_io(io)?
        || expected_context.run_id != ticket.run_id
        || expected_context.project_id != ticket.project_id
        || expected_context.owner_id != ticket.owner_id
        || expected_context.session_id != binding.execution.session_id
        || expected_context.artifact_root != binding.executor_root.join("runs")
        || xai_grok_science::workflow::run_id_for_operation(&binding.execution.operation_id)?
            != ticket.run_id.0
    {
        return Err(xai_grok_science::ScienceError::Ownership);
    }
    let run = store.load_run(&ticket.run_id)?;
    if run.context != *expected_context || run.state != xai_grok_science::RunState::AwaitingApproval
    {
        return Err(xai_grok_science::ScienceError::Invalid(
            "non-Allow workflow is not bound to its pending authority run".into(),
        ));
    }
    let approvals = store.approvals(&ticket.run_id)?;
    let [approval] = approvals.as_slice() else {
        return Err(xai_grok_science::ScienceError::Invalid(
            "non-Allow workflow requires exactly one pending approval".into(),
        ));
    };
    if approval.project_id != ticket.project_id
        || approval.run_id != ticket.run_id
        || approval.owner_id != ticket.owner_id
        || approval.call_id != ticket.call_id
        || approval.decision != xai_grok_science::ApprovalDecision::Pending
        || approval.decided_at.is_some()
    {
        return Err(xai_grok_science::ScienceError::Invalid(
            "non-Allow workflow approval binding changed before terminalization".into(),
        ));
    }
    require_exact_workflow_events(
        store,
        ticket,
        &[workflow_begin_expected(expected_context, binding)?],
        "pending authority",
    )?;
    if !store.artifacts(&ticket.run_id)?.is_empty()
        || !store.evidence(&ticket.run_id)?.is_empty()
        || !store.provenance(&ticket.run_id)?.is_empty()
        || !store.previews(&ticket.run_id)?.is_empty()
    {
        return Err(xai_grok_science::ScienceError::Invalid(
            "non-Allow workflow authority contains unexpected outputs".into(),
        ));
    }
    Ok(())
}

fn finish_unexecuted_workflow_authority(
    store: &xai_grok_science::ScienceStore,
    ticket: &xai_grok_science::csv::ScienceRunTicket,
    binding: &crate::session::commands::ScienceWorkflowBinding,
    io: &xai_grok_science::workflow::WorkflowIoCapability,
    expected_context: &xai_grok_science::RunContext,
    decision: xai_grok_science::ApprovalDecision,
    reason: String,
) -> xai_grok_science::Result<xai_grok_science::RunRecord> {
    if decision == xai_grok_science::ApprovalDecision::Allow
        || decision == xai_grok_science::ApprovalDecision::Pending
    {
        return Err(xai_grok_science::ScienceError::Invalid(
            "unexecuted workflow terminalization requires a non-Allow decision".into(),
        ));
    }
    validate_pending_workflow_authority(store, ticket, binding, io, expected_context)?;
    let (state, kind) = workflow_terminal_state_and_kind(decision.clone())?;
    store.decide_approval(
        &ticket.project_id,
        &ticket.run_id,
        &ticket.owner_id,
        &ticket.call_id,
        decision,
    )?;
    let begin = workflow_begin_expected(expected_context, binding)?;
    ensure_exact_next_workflow_event(
        store,
        ticket,
        &[begin],
        workflow_terminal_expected(ticket, kind, &reason),
        "unexecuted terminalization",
    )?;
    store.transition(&ticket.run_id, state, Some(reason))
}

fn validate_workflow_authority_binding(
    store: &xai_grok_science::ScienceStore,
    ticket: &xai_grok_science::csv::ScienceRunTicket,
    binding: &crate::session::commands::ScienceWorkflowBinding,
    io: &xai_grok_science::workflow::WorkflowIoCapability,
    expected_context: &xai_grok_science::RunContext,
    report: &xai_grok_science::workflow::WorkflowRunReport,
) -> xai_grok_science::Result<xai_grok_science::RunRecord> {
    if report.run.run_id != ticket.run_id.0
        || report.run.operation_id != binding.execution.operation_id
        || report.run.session_id != binding.execution.session_id
        || report.run.owner_id != binding.execution.owner_id
        || report.run.project_id != binding.execution.spec.project_id
        || report.run.workflow_id != binding.execution.spec.workflow_id
        || ticket.project_id.0 != binding.execution.spec.project_id.0
        || ticket.owner_id != binding.execution.owner_id
        || store.root() != binding.executor_root
        || xai_grok_science::workflow::run_id_for_operation(&binding.execution.operation_id)?
            != ticket.run_id.0
        || expected_context.run_id != ticket.run_id
        || expected_context.project_id != ticket.project_id
        || expected_context.owner_id != ticket.owner_id
        || expected_context.session_id != binding.execution.session_id
        || expected_context.artifact_root != binding.executor_root.join("runs")
        || !binding
            .executor_root
            .starts_with(&expected_context.workspace_root)
    {
        return Err(xai_grok_science::ScienceError::Ownership);
    }
    if !store.shares_root_capability_with_workflow_io(io)? {
        return Err(xai_grok_science::ScienceError::Ownership);
    }
    let run = store.load_run(&ticket.run_id)?;
    if run.context != *expected_context {
        return Err(xai_grok_science::ScienceError::Ownership);
    }
    let approvals = store.approvals(&ticket.run_id)?;
    let [approval] = approvals.as_slice() else {
        return Err(xai_grok_science::ScienceError::Invalid(
            "workflow authority requires exactly one approval".into(),
        ));
    };
    if approval.project_id != ticket.project_id
        || approval.run_id != ticket.run_id
        || approval.owner_id != ticket.owner_id
        || approval.call_id != ticket.call_id
        || approval.decision != xai_grok_science::ApprovalDecision::Allow
        || approval.decided_at.is_none()
    {
        return Err(xai_grok_science::ScienceError::Invalid(
            "workflow authority is not bound to a terminal Allow".into(),
        ));
    }
    Ok(run)
}

fn sync_workflow_authority_outputs(
    store: &xai_grok_science::ScienceStore,
    ticket: &xai_grok_science::csv::ScienceRunTicket,
    executor: &xai_grok_science::workflow::WorkflowExecutor,
    report: &xai_grok_science::workflow::WorkflowRunReport,
    run_state: xai_grok_science::RunState,
) -> xai_grok_science::Result<usize> {
    let outputs = collect_workflow_authority_outputs(ticket, executor, report)?;
    let expected_artifacts = outputs
        .iter()
        .map(|output| output.artifact.clone())
        .collect::<Vec<_>>();
    if !store.previews(&ticket.run_id)?.is_empty() {
        return Err(xai_grok_science::ScienceError::Invalid(
            "workflow authority does not permit an out-of-band preview registry".into(),
        ));
    }
    let existing = store.artifacts(&ticket.run_id)?;
    if existing.len() > expected_artifacts.len() || existing != expected_artifacts[..existing.len()]
    {
        return Err(xai_grok_science::ScienceError::Invalid(
            "workflow authority artifact registry conflicts with its commit plan".into(),
        ));
    }

    match run_state {
        xai_grok_science::RunState::Running => {
            for output in &outputs[existing.len()..] {
                let stored = store.put_artifact(
                    &ticket.project_id,
                    &ticket.run_id,
                    &ticket.owner_id,
                    ticket.call_id.clone(),
                    &output.artifact.relative_path,
                    &output.bytes,
                    output.artifact.mime.clone(),
                    output.artifact.preview.clone(),
                )?;
                if stored != output.artifact {
                    return Err(xai_grok_science::ScienceError::Invalid(
                        "workflow authority changed artifact metadata during registration".into(),
                    ));
                }
            }
            for output in &outputs {
                let reopened = store.allowed_running_artifact_bytes(
                    &ticket.project_id,
                    &ticket.run_id,
                    &ticket.owner_id,
                    &ticket.call_id,
                    &output.artifact.relative_path,
                )?;
                if reopened != output.bytes {
                    return Err(xai_grok_science::ScienceError::Invalid(
                        "workflow authority artifact bytes changed during registration".into(),
                    ));
                }
            }
            let expected_evidence = outputs
                .iter()
                .map(|output| output.evidence.clone())
                .collect::<Vec<_>>();
            append_exact_registry_suffix(
                store.evidence(&ticket.run_id)?,
                &expected_evidence,
                |item| store.add_evidence(item),
                "workflow evidence",
            )?;
            let expected_provenance = outputs
                .iter()
                .map(|output| output.provenance.clone())
                .collect::<Vec<_>>();
            append_exact_registry_suffix(
                store.provenance(&ticket.run_id)?,
                &expected_provenance,
                |item| store.add_provenance(item),
                "workflow provenance",
            )?;
        }
        xai_grok_science::RunState::Succeeded => {
            if existing != expected_artifacts
                || store.evidence(&ticket.run_id)?
                    != outputs
                        .iter()
                        .map(|output| output.evidence.clone())
                        .collect::<Vec<_>>()
                || store.provenance(&ticket.run_id)?
                    != outputs
                        .iter()
                        .map(|output| output.provenance.clone())
                        .collect::<Vec<_>>()
            {
                return Err(xai_grok_science::ScienceError::Invalid(
                    "terminal workflow authority registries differ from the commit ledger".into(),
                ));
            }
            for output in &outputs {
                let reopened = store.artifact_bytes(
                    &ticket.project_id,
                    &ticket.run_id,
                    &ticket.owner_id,
                    &output.artifact.relative_path,
                )?;
                if reopened != output.bytes {
                    return Err(xai_grok_science::ScienceError::Invalid(
                        "terminal workflow authority artifact differs from its commit".into(),
                    ));
                }
            }
        }
        _ => {
            return Err(xai_grok_science::ScienceError::Invalid(
                "workflow outputs may only be synchronized while Running or verified after Succeeded"
                    .into(),
            ));
        }
    }
    Ok(outputs.len())
}

fn finalize_workflow_authority(
    store: &xai_grok_science::ScienceStore,
    ticket: &xai_grok_science::csv::ScienceRunTicket,
    binding: &crate::session::commands::ScienceWorkflowBinding,
    io: &xai_grok_science::workflow::WorkflowIoCapability,
    expected_context: &xai_grok_science::RunContext,
    executor: &xai_grok_science::workflow::WorkflowExecutor,
    report: &xai_grok_science::workflow::WorkflowRunReport,
) -> xai_grok_science::Result<()> {
    use xai_grok_science::workflow::WorkflowState;

    let run =
        validate_workflow_authority_binding(store, ticket, binding, io, expected_context, report)?;
    if report.run.state == WorkflowState::Succeeded {
        let registered =
            sync_workflow_authority_outputs(store, ticket, executor, report, run.state)?;
        if run.state == xai_grok_science::RunState::Succeeded {
            validate_workflow_finished_event(
                store,
                ticket,
                binding,
                expected_context,
                report,
                registered,
            )?;
            return Ok(());
        }
        if run.state != xai_grok_science::RunState::Running {
            return Err(xai_grok_science::ScienceError::Invalid(
                "succeeded workflow is bound to a non-running authority run".into(),
            ));
        }
        ensure_exact_next_workflow_event(
            store,
            ticket,
            &[
                workflow_begin_expected(expected_context, binding)?,
                workflow_allowed_expected(ticket),
            ],
            (
                "SessionActor",
                "workflow.execution.finished",
                workflow_finished_event_payload(ticket, report, registered)?,
            ),
            "successful finish",
        )?;
        validate_workflow_finished_event(
            store,
            ticket,
            binding,
            expected_context,
            report,
            registered,
        )?;
        let artifacts = store.artifacts(&ticket.run_id)?;
        let evidence = store.evidence(&ticket.run_id)?;
        let provenance = store.provenance(&ticket.run_id)?;
        let previews = store.previews(&ticket.run_id)?;
        let events = store.events_after(&ticket.run_id, 0, 1_000)?;
        let final_event = events.last().cloned().ok_or_else(|| {
            xai_grok_science::ScienceError::Invalid(
                "workflow completion event disappeared before atomic commit".into(),
            )
        })?;
        if events.len() == 1_000
            || final_event.actor != "SessionActor"
            || final_event.kind != "workflow.execution.finished"
        {
            return Err(xai_grok_science::ScienceError::Invalid(
                "workflow completion event collection changed before atomic commit".into(),
            ));
        }
        store.transition_succeeded_with_manifest(
            &xai_grok_science::SuccessfulCompletionManifest {
                context: run.context,
                artifacts,
                evidence,
                provenance,
                previews,
                events,
                final_event,
            },
        )?;
        return Ok(());
    }

    match run.state {
        xai_grok_science::RunState::Running => {
            let existing = store.artifacts(&ticket.run_id)?;
            let paths = existing
                .iter()
                .map(|artifact| artifact.relative_path.as_path())
                .collect::<Vec<_>>();
            store.discard_running_outputs(
                &ticket.project_id,
                &ticket.run_id,
                &ticket.owner_id,
                &ticket.call_id,
                &paths,
            )?;
            ensure_exact_next_workflow_event(
                store,
                ticket,
                &[
                    workflow_begin_expected(expected_context, binding)?,
                    workflow_allowed_expected(ticket),
                ],
                (
                    "SessionActor",
                    "workflow.execution.finished",
                    workflow_finished_event_payload(ticket, report, 0)?,
                ),
                "failed finish",
            )?;
            validate_workflow_finished_event(store, ticket, binding, expected_context, report, 0)?;
            store.transition(
                &ticket.run_id,
                xai_grok_science::RunState::Failed,
                report.run.failure.clone(),
            )?;
            Ok(())
        }
        xai_grok_science::RunState::Failed
            if store.artifacts(&ticket.run_id)?.is_empty()
                && store.evidence(&ticket.run_id)?.is_empty()
                && store.provenance(&ticket.run_id)?.is_empty() =>
        {
            validate_workflow_finished_event(store, ticket, binding, expected_context, report, 0)?;
            Ok(())
        }
        _ => Err(xai_grok_science::ScienceError::Invalid(
            "failed workflow is not bound to an empty failed authority run".into(),
        )),
    }
}

#[derive(serde::Serialize)]
struct DurableWorkflowReportProjection<'a> {
    schema: &'static str,
    run: &'a xai_grok_science::workflow::WorkflowRunRecord,
    attempts: &'a [xai_grok_science::workflow::StepAttempt],
    commits: &'a [xai_grok_science::workflow::ArtifactCommit],
    artifacts_committed: usize,
    steps_reused: usize,
    recovered: bool,
}

fn workflow_report_was_durably_recovered(
    report: &xai_grok_science::workflow::WorkflowRunReport,
) -> bool {
    report.run.state_history.iter().any(|transition| {
        transition.state == xai_grok_science::workflow::WorkflowState::Interrupted
            && transition.note.as_deref() == Some("crash recovery")
    })
}

fn workflow_finished_event_payload(
    ticket: &xai_grok_science::csv::ScienceRunTicket,
    report: &xai_grok_science::workflow::WorkflowRunReport,
    authority_artifacts_registered: usize,
) -> xai_grok_science::Result<serde_json::Value> {
    let durably_recovered = workflow_report_was_durably_recovered(report);
    let committed_report = DurableWorkflowReportProjection {
        schema: "lumen.science.workflow.durable-report.v1",
        run: &report.run,
        attempts: &report.attempts,
        commits: &report.commits,
        artifacts_committed: report.artifacts_committed,
        steps_reused: report.steps_reused,
        recovered: durably_recovered,
    };
    let report_sha256 = format!(
        "{:x}",
        sha2::Sha256::digest(serde_json::to_vec(&committed_report)?)
    );
    Ok(serde_json::json!({
        "schema": "lumen.science.workflow.execution.finished.v1",
        "authority_run_id": ticket.run_id.0,
        "project_id": ticket.project_id.0,
        "owner_id": ticket.owner_id,
        "call_id": ticket.call_id.0,
        "session_id": report.run.session_id,
        "operation_id": report.run.operation_id,
        "workflow_run_id": report.run.run_id,
        "workflow_id": report.run.workflow_id,
        "state": format!("{:?}", report.run.state),
        "failure": report.run.failure,
        "attempts": report.attempts.len(),
        "commits": report.commits.len(),
        "artifacts_committed": report.artifacts_committed,
        "authority_artifacts_registered": authority_artifacts_registered,
        "steps_reused": report.steps_reused,
        "recovered": durably_recovered,
        "workflow_report_sha256": report_sha256,
    }))
}

fn validate_workflow_finished_event(
    store: &xai_grok_science::ScienceStore,
    ticket: &xai_grok_science::csv::ScienceRunTicket,
    binding: &crate::session::commands::ScienceWorkflowBinding,
    expected_context: &xai_grok_science::RunContext,
    report: &xai_grok_science::workflow::WorkflowRunReport,
    authority_artifacts_registered: usize,
) -> xai_grok_science::Result<()> {
    require_exact_workflow_events(
        store,
        ticket,
        &[
            workflow_begin_expected(expected_context, binding)?,
            workflow_allowed_expected(ticket),
            (
                "SessionActor",
                "workflow.execution.finished",
                workflow_finished_event_payload(ticket, report, authority_artifacts_registered)?,
            ),
        ],
        "terminal finish",
    )
}

fn fail_workflow_authority_run(
    store: &xai_grok_science::ScienceStore,
    ticket: &xai_grok_science::csv::ScienceRunTicket,
    planned_paths: Vec<std::path::PathBuf>,
    error: xai_grok_science::ScienceError,
) -> xai_grok_science::ScienceError {
    let Ok(run) = store.load_run(&ticket.run_id) else {
        return error;
    };
    if run.state != xai_grok_science::RunState::Running {
        return error;
    }
    let cleanup_paths = planned_paths
        .iter()
        .map(std::path::PathBuf::as_path)
        .collect::<Vec<_>>();
    let cleanup = store.discard_running_outputs(
        &ticket.project_id,
        &ticket.run_id,
        &ticket.owner_id,
        &ticket.call_id,
        &cleanup_paths,
    );
    if let Err(ref cleanup_error) = cleanup {
        return xai_grok_science::ScienceError::Invalid(format!(
            "{error}; authority rollback remains recoverable while Running: {cleanup_error}"
        ));
    }
    let rollback_verified = (|| -> xai_grok_science::Result<bool> {
        Ok(store.artifacts(&ticket.run_id)?.is_empty()
            && store.evidence(&ticket.run_id)?.is_empty()
            && store.provenance(&ticket.run_id)?.is_empty()
            && store.previews(&ticket.run_id)?.is_empty())
    })();
    match rollback_verified {
        Ok(true) => {}
        Ok(false) => {
            return xai_grok_science::ScienceError::Invalid(format!(
                "{error}; authority rollback left registered outputs and remains recoverable while Running"
            ));
        }
        Err(read_error) => {
            return xai_grok_science::ScienceError::Invalid(format!(
                "{error}; authority rollback could not be verified and remains recoverable while Running: {read_error}"
            ));
        }
    }
    let terminal = xai_grok_science::csv::fail_running(
        store,
        ticket,
        format!("workflow authority commit rejected: {error}"),
    );
    match terminal {
        Ok(_) => error,
        Err(terminal_error) => xai_grok_science::ScienceError::Invalid(format!(
            "{error}; authority rollback completed but Failed terminal could not be persisted: {terminal_error}"
        )),
    }
}

#[cfg(test)]
mod seq_undelivered_begin_tests {
    use super::*;

    #[test]
    fn undelivered_seq_begin_records_exact_interrupted_decision_without_outputs() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = dunce::canonicalize(temp.path()).unwrap();
        let source_path = workspace.join("undelivered-seq.fasta");
        let source_bytes = b">undelivered\nACGTACGT\n";
        std::fs::write(&source_path, source_bytes).unwrap();
        let store_root = workspace.join("science-store");
        std::fs::create_dir(&store_root).unwrap();
        let store = xai_grok_science::ScienceStore::new_confined(&store_root, &workspace).unwrap();
        let options = xai_grok_science::seqbench::SeqAnalyzeOptions::default();
        let operation_id = "seq-undelivered-begin-operation-0001";
        let source_relative =
            xai_grok_science::seqbench::source_relative_binding(&workspace, &source_path).unwrap();
        let context = xai_grok_science::RunContext {
            run_id: xai_grok_science::seqbench::operation_run_id(operation_id),
            project_id: xai_grok_science::ProjectId::new("project-undelivered-seq"),
            session_id: "session-undelivered-seq".into(),
            owner_id: "owner-undelivered-seq".into(),
            workspace_root: workspace.clone(),
            provider: "offline-deterministic".into(),
            approval_policy: "production-session-permission".into(),
            tool_profile: "science-seqbench-v4".into(),
            artifact_root: store_root,
            environment: std::collections::BTreeMap::from([
                ("network".into(), "disabled".into()),
                ("locale".into(), "C".into()),
                (
                    xai_grok_science::seqbench::OPERATION_ENV.into(),
                    operation_id.into(),
                ),
                (
                    xai_grok_science::seqbench::REQUEST_SHA256_ENV.into(),
                    xai_grok_science::seqbench::request_sha256(
                        &source_relative,
                        source_bytes,
                        &options,
                    )
                    .unwrap(),
                ),
                (
                    xai_grok_science::seqbench::SOURCE_SHA256_ENV.into(),
                    xai_grok_science::seqbench::hex_sha256(source_bytes),
                ),
                (
                    xai_grok_science::seqbench::SOURCE_BYTES_ENV.into(),
                    source_bytes.len().to_string(),
                ),
                (
                    xai_grok_science::seqbench::SOURCE_RELATIVE_PATH_ENV.into(),
                    source_relative,
                ),
                (
                    xai_grok_science::seqbench::PROJECT_REVISION_ENV.into(),
                    "project-revision-undelivered".into(),
                ),
            ]),
        };
        let (ticket, _) = xai_grok_science::seqbench::begin_analysis_with_options_witnessed(
            &store, context, &options,
        )
        .unwrap();

        interrupt_undelivered_seq_authority(&store, &ticket).unwrap();

        let terminal = store.load_run(&ticket.run_id).unwrap();
        assert_eq!(terminal.state, xai_grok_science::RunState::Interrupted);
        assert_eq!(
            terminal.terminal_reason.as_deref(),
            Some(SEQ_UNDELIVERED_BEGIN_REASON)
        );
        let approvals = store.approvals(&ticket.run_id).unwrap();
        assert_eq!(approvals.len(), 1);
        assert_eq!(
            approvals[0].decision,
            xai_grok_science::ApprovalDecision::Interrupted
        );
        let events = store.events_after(&ticket.run_id, 0, 1_000).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].actor, "SessionActor");
        assert_eq!(events[0].kind, "run.created");
        assert_eq!(events[1].actor, "LumenApproval");
        assert_eq!(events[1].kind, "approval.interrupted");
        assert_eq!(
            events[1].payload,
            serde_json::json!({
                "call_id": ticket.call_id.0,
                "decided_at": approvals[0].decided_at,
                "reason": SEQ_UNDELIVERED_BEGIN_REASON,
            })
        );
        assert!(store.artifacts(&ticket.run_id).unwrap().is_empty());
        assert!(store.evidence(&ticket.run_id).unwrap().is_empty());
        assert!(store.provenance(&ticket.run_id).unwrap().is_empty());
        assert!(store.previews(&ticket.run_id).unwrap().is_empty());
        assert!(
            !store
                .root()
                .join("runs")
                .join(&ticket.run_id.0)
                .join("seq-authority-prefix-seal.json")
                .exists()
        );
    }
}

#[cfg(test)]
mod workflow_authority_failure_tests {
    use super::*;

    fn bind_test_workflow_admission(
        context: &mut xai_grok_science::RunContext,
        binding: &crate::session::commands::ScienceWorkflowBinding,
    ) {
        context.environment.insert(
            WORKFLOW_OPERATION_ID_ENV.into(),
            binding.execution.operation_id.clone(),
        );
        context.environment.insert(
            WORKFLOW_ADMISSION_SHA256_ENV.into(),
            format!(
                "{:x}",
                sha2::Sha256::digest(binding.execution.operation_id.as_bytes())
            ),
        );
    }

    fn pending_prepared_without_grant(
        workspace: &std::path::Path,
    ) -> PreparedScienceWorkflowExecution {
        let workspace = dunce::canonicalize(workspace).unwrap();
        let store_root = workspace.join("science-store");
        std::fs::create_dir(&store_root).unwrap();
        let store = xai_grok_science::ScienceStore::new_confined(&store_root, &workspace).unwrap();
        let project_store =
            xai_grok_science::project::ProjectStore::new_confined(&store_root, &workspace).unwrap();
        let project = project_store
            .create_project(
                "owner-missing-workflow-grant",
                "Missing workflow grant",
                "Can a raw Allow execute without the production permission capability?",
            )
            .unwrap();
        let project_revision = project_store
            .with_owned_project_revision(
                &project.project_id,
                "owner-missing-workflow-grant",
                |_project, revision| Ok(revision.to_owned()),
            )
            .unwrap();
        let operation_id = "operation-missing-workflow-grant";
        let mut context = xai_grok_science::RunContext {
            run_id: xai_grok_science::RunId::new(
                xai_grok_science::workflow::run_id_for_operation(operation_id).unwrap(),
            ),
            project_id: xai_grok_science::ProjectId::new(project.project_id.0.clone()),
            session_id: "session-missing-workflow-grant".into(),
            owner_id: "owner-missing-workflow-grant".into(),
            workspace_root: workspace.clone(),
            provider: "offline-test".into(),
            approval_policy: "production-session-permission".into(),
            tool_profile: "science-workflow-execute-v1".into(),
            artifact_root: store_root.join("runs"),
            environment: std::collections::BTreeMap::new(),
        };
        let binding = crate::session::commands::ScienceWorkflowBinding {
            execution: xai_grok_science::workflow::WorkflowExecutionRequest {
                operation_id: operation_id.into(),
                session_id: context.session_id.clone(),
                owner_id: context.owner_id.clone(),
                spec: xai_grok_science::workflow::WorkflowSpec {
                    workflow_id: "workflow-missing-grant".into(),
                    project_id: project.project_id,
                    name: "missing permission grant".into(),
                    steps: Vec::new(),
                    parameters: std::collections::BTreeMap::new(),
                    permissions: Vec::new(),
                    resources: xai_grok_science::workflow::ResourceLimits {
                        max_concurrent_steps: 1,
                        max_total_duration_secs: 30,
                        max_memory_mb: 128,
                        max_disk_mb: 1,
                    },
                    schema_version:
                        xai_grok_science::workflow::WorkflowSpec::CURRENT_SCHEMA_VERSION,
                },
            },
            executor_root: store_root.clone(),
            kernel_id: "kernel-must-not-probe".into(),
            kernel_kind: xai_grok_science::workflow::KernelKind::Python,
            interpreter_path: std::path::PathBuf::from("/bin/sh"),
            probe_timeout: std::time::Duration::from_secs(1),
            allow_kernel_steps: true,
        };
        bind_test_workflow_admission(&mut context, &binding);
        let ticket = begin_workflow_execution_run(&store, context.clone(), &binding).unwrap();
        let io = xai_grok_science::workflow::WorkflowIoCapability::open_existing_confined(
            &store_root,
            &workspace,
        )
        .unwrap();
        let executable = std::sync::Arc::new(
            xai_grok_science::workflow::PinnedExecutable::pin(&binding.interpreter_path).unwrap(),
        );
        let executor = xai_grok_science::workflow::WorkflowExecutor::from_io(
            &store_root,
            &io,
            workflow_compute_environment(&binding, Some(executable.sha256())),
        )
        .with_policy(workflow_execution_policy(&binding));
        PreparedScienceWorkflowExecution {
            store,
            project_store,
            project_revision,
            ticket,
            expected_context: context,
            binding,
            io,
            executor,
            executable,
            target: "must not execute".into(),
            replayed: None,
            resume_allowed: false,
            permission_grant: None,
        }
    }

    #[test]
    fn raw_allow_without_private_grant_durably_denies_and_creates_no_workflow_state() {
        let temp = tempfile::tempdir().unwrap();
        let prepared = pending_prepared_without_grant(temp.path());
        let run_id = prepared.ticket.run_id.clone();
        let store_root = prepared.binding.executor_root.clone();
        let error = require_exact_workflow_allow_grant(&prepared).unwrap_err();
        assert!(error.to_string().contains("permission grant rejected"));

        assert_eq!(
            prepared.store.load_run(&run_id).unwrap().state,
            xai_grok_science::RunState::Denied
        );
        let approvals = prepared.store.approvals(&run_id).unwrap();
        assert_eq!(approvals.len(), 1);
        assert_eq!(
            approvals[0].decision,
            xai_grok_science::ApprovalDecision::Deny
        );
        assert!(approvals[0].decided_at.is_some());
        assert!(prepared.store.artifacts(&run_id).unwrap().is_empty());
        assert!(prepared.store.evidence(&run_id).unwrap().is_empty());
        assert!(prepared.store.provenance(&run_id).unwrap().is_empty());
        assert!(prepared.store.previews(&run_id).unwrap().is_empty());
        for forbidden in [
            "workflow-cells",
            "workflow-outputs",
            "workflow-runs",
            "workflow-operations",
            "workflow-attempts",
            "workflow-commits",
            "workflow-artifacts",
        ] {
            assert!(
                !store_root.join(forbidden).exists(),
                "ungranted Allow created workflow state at {forbidden}"
            );
        }
    }

    #[test]
    fn undelivered_begin_interrupts_pending_approval_without_probe_or_execution() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = dunce::canonicalize(temp.path()).unwrap();
        let store_root = workspace.join("science-store");
        std::fs::create_dir(&store_root).unwrap();
        let store = xai_grok_science::ScienceStore::new_confined(&store_root, &workspace).unwrap();
        let operation_id = "operation-undelivered-workflow-begin";
        let project_id =
            xai_grok_science::project::ProjectId("project-undelivered-workflow-begin".into());
        let mut context = xai_grok_science::RunContext {
            run_id: xai_grok_science::RunId::new(
                xai_grok_science::workflow::run_id_for_operation(operation_id).unwrap(),
            ),
            project_id: xai_grok_science::ProjectId::new(project_id.0.clone()),
            session_id: "session-undelivered-workflow-begin".into(),
            owner_id: "owner-undelivered-workflow-begin".into(),
            workspace_root: workspace.clone(),
            provider: "offline-deterministic".into(),
            approval_policy: "production-session-permission".into(),
            tool_profile: "science-workflow-execute-v1".into(),
            artifact_root: store_root.join("runs"),
            environment: std::collections::BTreeMap::new(),
        };
        let binding = crate::session::commands::ScienceWorkflowBinding {
            execution: xai_grok_science::workflow::WorkflowExecutionRequest {
                operation_id: operation_id.into(),
                session_id: context.session_id.clone(),
                owner_id: context.owner_id.clone(),
                spec: xai_grok_science::workflow::WorkflowSpec {
                    workflow_id: "workflow-undelivered-begin".into(),
                    project_id,
                    name: "undelivered Begin".into(),
                    steps: Vec::new(),
                    parameters: std::collections::BTreeMap::new(),
                    permissions: Vec::new(),
                    resources: xai_grok_science::workflow::ResourceLimits {
                        max_concurrent_steps: 1,
                        max_total_duration_secs: 30,
                        max_memory_mb: 128,
                        max_disk_mb: 1,
                    },
                    schema_version:
                        xai_grok_science::workflow::WorkflowSpec::CURRENT_SCHEMA_VERSION,
                },
            },
            executor_root: store_root.clone(),
            kernel_id: "kernel-must-not-probe".into(),
            kernel_kind: xai_grok_science::workflow::KernelKind::Python,
            interpreter_path: workspace.join("interpreter-must-not-be-opened"),
            probe_timeout: std::time::Duration::from_secs(1),
            allow_kernel_steps: true,
        };
        bind_test_workflow_admission(&mut context, &binding);
        let ticket = begin_workflow_execution_run(&store, context.clone(), &binding).unwrap();
        let io = xai_grok_science::workflow::WorkflowIoCapability::open_existing_confined(
            &store_root,
            &workspace,
        )
        .unwrap();

        let terminal = finish_unexecuted_workflow_authority(
            &store,
            &ticket,
            &binding,
            &io,
            &context,
            xai_grok_science::ApprovalDecision::Interrupted,
            "workflow Begin response receiver closed before delivery".into(),
        )
        .unwrap();

        assert_eq!(terminal.state, xai_grok_science::RunState::Interrupted);
        assert_eq!(
            terminal.terminal_reason.as_deref(),
            Some("workflow Begin response receiver closed before delivery")
        );
        require_exact_workflow_events(
            &store,
            &ticket,
            &[
                workflow_begin_expected(&context, &binding).unwrap(),
                workflow_terminal_expected(
                    &ticket,
                    "approval.interrupted",
                    "workflow Begin response receiver closed before delivery",
                ),
            ],
            "undelivered Begin",
        )
        .unwrap();
        let approvals = store.approvals(&ticket.run_id).unwrap();
        assert_eq!(approvals.len(), 1);
        assert_eq!(
            approvals[0].decision,
            xai_grok_science::ApprovalDecision::Interrupted
        );
        assert!(approvals[0].decided_at.is_some());
        assert!(store.artifacts(&ticket.run_id).unwrap().is_empty());
        assert!(store.evidence(&ticket.run_id).unwrap().is_empty());
        assert!(store.provenance(&ticket.run_id).unwrap().is_empty());
        assert!(store.previews(&ticket.run_id).unwrap().is_empty());
        for forbidden in [
            "workflow-runs",
            "workflow-operations",
            "workflow-attempts",
            "workflow-commits",
            "workflow-artifacts",
        ] {
            assert!(
                !store_root.join(forbidden).exists(),
                "undelivered Begin created execution state at {forbidden}"
            );
        }
        assert!(
            !binding.interpreter_path.exists(),
            "test interpreter unexpectedly exists, so a probe could be hidden"
        );
    }

    #[test]
    fn terminal_replay_rejects_missing_duplicate_nonfinal_or_tampered_finish_event() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = dunce::canonicalize(temp.path()).unwrap();
        let store_root = workspace.join("science-store");
        std::fs::create_dir(&store_root).unwrap();
        let store = xai_grok_science::ScienceStore::new_confined(&store_root, &workspace).unwrap();
        let operation_id = "operation-finish-event-tamper";
        let project_id = xai_grok_science::project::ProjectId("project-finish-event".into());
        let mut context = xai_grok_science::RunContext {
            run_id: xai_grok_science::RunId::new(
                xai_grok_science::workflow::run_id_for_operation(operation_id).unwrap(),
            ),
            project_id: xai_grok_science::ProjectId::new(project_id.0.clone()),
            session_id: "session-finish-event".into(),
            owner_id: "owner-finish-event".into(),
            workspace_root: workspace.clone(),
            provider: "offline-deterministic".into(),
            approval_policy: "production-session-permission".into(),
            tool_profile: "science-workflow-execute-v1".into(),
            artifact_root: store_root.join("runs"),
            environment: std::collections::BTreeMap::new(),
        };
        let binding = crate::session::commands::ScienceWorkflowBinding {
            execution: xai_grok_science::workflow::WorkflowExecutionRequest {
                operation_id: operation_id.into(),
                session_id: context.session_id.clone(),
                owner_id: context.owner_id.clone(),
                spec: xai_grok_science::workflow::WorkflowSpec {
                    workflow_id: "workflow-finish-event".into(),
                    project_id,
                    name: "finish event".into(),
                    steps: Vec::new(),
                    parameters: std::collections::BTreeMap::new(),
                    permissions: Vec::new(),
                    resources: xai_grok_science::workflow::ResourceLimits {
                        max_concurrent_steps: 1,
                        max_total_duration_secs: 30,
                        max_memory_mb: 128,
                        max_disk_mb: 1,
                    },
                    schema_version:
                        xai_grok_science::workflow::WorkflowSpec::CURRENT_SCHEMA_VERSION,
                },
            },
            executor_root: store_root.clone(),
            kernel_id: "kernel-not-needed".into(),
            kernel_kind: xai_grok_science::workflow::KernelKind::Python,
            interpreter_path: workspace.join("interpreter-not-needed"),
            probe_timeout: std::time::Duration::from_secs(1),
            allow_kernel_steps: false,
        };
        bind_test_workflow_admission(&mut context, &binding);
        let ticket = begin_workflow_execution_run(&store, context.clone(), &binding).unwrap();
        xai_grok_science::csv::mark_allowed(&store, &ticket).unwrap();
        let io = xai_grok_science::workflow::WorkflowIoCapability::open_existing_confined(
            &store_root,
            &workspace,
        )
        .unwrap();
        let executor = xai_grok_science::workflow::WorkflowExecutor::from_io(
            &store_root,
            &io,
            workflow_compute_environment(&binding, None),
        )
        .with_policy(workflow_execution_policy(&binding));
        let report = executor.execute(&binding.execution).unwrap();
        assert_eq!(
            report.run.state,
            xai_grok_science::workflow::WorkflowState::Succeeded
        );
        let fresh_payload = workflow_finished_event_payload(&ticket, &report, 0).unwrap();
        let mut replay_response = report.clone();
        replay_response.replayed = true;
        replay_response.recovered = true;
        assert_eq!(
            workflow_finished_event_payload(&ticket, &replay_response, 0).unwrap(),
            fresh_payload,
            "response-only delivery status changed the durable finish projection"
        );
        assert!(
            fresh_payload.get("replayed").is_none(),
            "response-only replay status leaked into the durable finish event"
        );
        store
            .append_recoverable_commit_event(
                &ticket.run_id,
                "SessionActor",
                "workflow.execution.finished",
                fresh_payload,
            )
            .unwrap();
        store.transition_succeeded_verified(&ticket.run_id).unwrap();
        validate_workflow_finished_event(&store, &ticket, &binding, &context, &report, 0).unwrap();

        let events_path = store_root
            .join("runs")
            .join(&ticket.run_id.0)
            .join("events.json");
        let original_bytes = std::fs::read(&events_path).unwrap();
        let original: Vec<serde_json::Value> = serde_json::from_slice(&original_bytes).unwrap();
        let finish_index = original
            .iter()
            .position(|event| event["kind"] == "workflow.execution.finished")
            .unwrap();
        for tamper in [
            "missing",
            "duplicate",
            "non-final",
            "actor",
            "report-sha256",
            "unknown-prefix",
            "duplicate-run-created",
            "duplicate-allowed",
        ] {
            let mut changed = original.clone();
            match tamper {
                "missing" => {
                    changed.remove(finish_index);
                }
                "duplicate" => {
                    let mut duplicate = changed[finish_index].clone();
                    duplicate["seq"] =
                        serde_json::json!(changed.last().unwrap()["seq"].as_u64().unwrap() + 1);
                    changed.push(duplicate);
                }
                "non-final" => {
                    let mut trailing = changed[finish_index].clone();
                    trailing["seq"] =
                        serde_json::json!(changed.last().unwrap()["seq"].as_u64().unwrap() + 1);
                    trailing["kind"] = serde_json::json!("attacker.after.workflow.finish");
                    trailing["actor"] = serde_json::json!("attacker");
                    trailing["payload"] = serde_json::json!({});
                    changed.push(trailing);
                }
                "actor" => changed[finish_index]["actor"] = serde_json::json!("attacker"),
                "report-sha256" => {
                    changed[finish_index]["payload"]["workflow_report_sha256"] =
                        serde_json::json!("0".repeat(64));
                }
                "unknown-prefix" => {
                    let mut unknown = changed[0].clone();
                    unknown["actor"] = serde_json::json!("attacker");
                    unknown["kind"] = serde_json::json!("attacker.before.workflow.finish");
                    unknown["payload"] = serde_json::json!({});
                    changed.insert(1, unknown);
                    for (index, event) in changed.iter_mut().enumerate() {
                        event["seq"] = serde_json::json!(index + 1);
                    }
                }
                "duplicate-run-created" => {
                    changed.insert(1, changed[0].clone());
                    for (index, event) in changed.iter_mut().enumerate() {
                        event["seq"] = serde_json::json!(index + 1);
                    }
                }
                "duplicate-allowed" => {
                    changed.insert(2, changed[1].clone());
                    for (index, event) in changed.iter_mut().enumerate() {
                        event["seq"] = serde_json::json!(index + 1);
                    }
                }
                _ => unreachable!(),
            }
            std::fs::write(&events_path, serde_json::to_vec_pretty(&changed).unwrap()).unwrap();
            assert!(
                validate_workflow_finished_event(&store, &ticket, &binding, &context, &report, 0,)
                    .is_err(),
                "accepted {tamper} workflow finish-event tamper"
            );
        }
        std::fs::write(&events_path, original_bytes).unwrap();
        validate_workflow_finished_event(&store, &ticket, &binding, &context, &report, 0).unwrap();
    }

    #[test]
    fn recoverable_finish_event_failure_rolls_back_before_failed_terminal() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = dunce::canonicalize(temp.path()).unwrap();
        let store_root = workspace.join("science-store");
        std::fs::create_dir(&store_root).unwrap();
        let store = xai_grok_science::ScienceStore::new_confined(&store_root, &workspace).unwrap();
        let ticket = xai_grok_science::csv::ScienceRunTicket {
            project_id: xai_grok_science::ProjectId::new("workflow-event-project"),
            run_id: xai_grok_science::RunId::new("workflow-event-run"),
            owner_id: "workflow-event-owner".into(),
            call_id: xai_grok_science::CallId::new("science_workflow_execute"),
        };
        store
            .create_run(xai_grok_science::RunContext {
                run_id: ticket.run_id.clone(),
                project_id: ticket.project_id.clone(),
                session_id: "workflow-event-session".into(),
                owner_id: ticket.owner_id.clone(),
                workspace_root: workspace,
                provider: "offline-deterministic".into(),
                approval_policy: "production-session-permission".into(),
                tool_profile: "science-workflow-execute-v1".into(),
                artifact_root: store_root.join("runs"),
                environment: std::collections::BTreeMap::new(),
            })
            .unwrap();
        store
            .request_approval(xai_grok_science::Approval {
                project_id: ticket.project_id.clone(),
                run_id: ticket.run_id.clone(),
                call_id: ticket.call_id.clone(),
                owner_id: ticket.owner_id.clone(),
                decision: xai_grok_science::ApprovalDecision::Pending,
                decided_at: None,
            })
            .unwrap();
        store
            .transition(
                &ticket.run_id,
                xai_grok_science::RunState::AwaitingApproval,
                None,
            )
            .unwrap();
        xai_grok_science::csv::mark_allowed(&store, &ticket).unwrap();

        let relative = std::path::Path::new("workflow/compute/result.json");
        let artifact = store
            .put_artifact(
                &ticket.project_id,
                &ticket.run_id,
                &ticket.owner_id,
                ticket.call_id.clone(),
                relative,
                b"{\"ok\":true}",
                "application/json",
                "workflow output",
            )
            .unwrap();
        store
            .add_evidence(xai_grok_science::Evidence {
                run_id: ticket.run_id.clone(),
                claim: "event failure output".into(),
                source: "lumen-workflow-cas:test".into(),
                artifact_sha256: Some(artifact.sha256.clone()),
                verified_at: chrono::Utc::now(),
            })
            .unwrap();
        store
            .add_provenance(xai_grok_science::Provenance {
                run_id: ticket.run_id.clone(),
                source_uri: "lumen-workflow-cas://test".into(),
                source_commit: Some("test-commit".into()),
                source_path: Some(relative.display().to_string()),
                license: "test".into(),
                retrieved_at: chrono::Utc::now(),
                input_sha256: artifact.sha256,
                tool: "SessionActor/science_workflow_execute-v1".into(),
                environment: std::collections::BTreeMap::new(),
            })
            .unwrap();

        std::fs::write(
            store_root
                .join("runs")
                .join(&ticket.run_id.0)
                .join("events.json"),
            b"{",
        )
        .unwrap();
        assert!(
            store
                .append_recoverable_commit_event(
                    &ticket.run_id,
                    "SessionActor",
                    "workflow.execution.finished",
                    serde_json::json!({}),
                )
                .is_err()
        );
        assert_eq!(
            store.load_run(&ticket.run_id).unwrap().state,
            xai_grok_science::RunState::Running
        );

        let returned = fail_workflow_authority_run(
            &store,
            &ticket,
            vec![relative.to_path_buf()],
            xai_grok_science::ScienceError::Invalid("injected event failure".into()),
        );
        assert!(returned.to_string().contains("injected event failure"));
        assert_eq!(
            store.load_run(&ticket.run_id).unwrap().state,
            xai_grok_science::RunState::Failed
        );
        assert!(store.artifacts(&ticket.run_id).unwrap().is_empty());
        assert!(store.evidence(&ticket.run_id).unwrap().is_empty());
        assert!(store.provenance(&ticket.run_id).unwrap().is_empty());
        assert!(store.previews(&ticket.run_id).unwrap().is_empty());
        assert!(
            !store_root
                .join("runs")
                .join(&ticket.run_id.0)
                .join("artifacts")
                .join(relative)
                .exists()
        );
    }
}

#[cfg(test)]
mod workflow_authority_recovery_tests {
    use super::*;

    struct Fixture {
        _temp: tempfile::TempDir,
        store: xai_grok_science::ScienceStore,
        context: xai_grok_science::RunContext,
        binding: crate::session::commands::ScienceWorkflowBinding,
        io: xai_grok_science::workflow::WorkflowIoCapability,
        executor: xai_grok_science::workflow::WorkflowExecutor,
    }

    impl Fixture {
        fn new(operation_id: &str) -> Self {
            let temp = tempfile::tempdir().unwrap();
            let workspace = dunce::canonicalize(temp.path()).unwrap();
            let store_root = workspace.join("science-store");
            std::fs::create_dir(&store_root).unwrap();
            let store =
                xai_grok_science::ScienceStore::new_confined(&store_root, &workspace).unwrap();
            let interpreter_path = dunce::canonicalize("/bin/sh").unwrap();
            let project_id = xai_grok_science::ProjectId::new(format!("project-{operation_id}"));
            let mut context = xai_grok_science::RunContext {
                run_id: xai_grok_science::RunId::new(
                    xai_grok_science::workflow::run_id_for_operation(operation_id).unwrap(),
                ),
                project_id: project_id.clone(),
                session_id: format!("session-{operation_id}"),
                owner_id: format!("owner-{operation_id}"),
                workspace_root: workspace.clone(),
                provider: "offline-deterministic".into(),
                approval_policy: "production-session-permission".into(),
                tool_profile: "science-workflow-execute-v1".into(),
                artifact_root: store_root.join("runs"),
                environment: std::collections::BTreeMap::new(),
            };
            let binding = crate::session::commands::ScienceWorkflowBinding {
                execution: xai_grok_science::workflow::WorkflowExecutionRequest {
                    operation_id: operation_id.into(),
                    session_id: context.session_id.clone(),
                    owner_id: context.owner_id.clone(),
                    spec: xai_grok_science::workflow::WorkflowSpec {
                        workflow_id: format!("workflow-{operation_id}"),
                        project_id: xai_grok_science::project::ProjectId(project_id.0.clone()),
                        name: "workflow recovery fixture".into(),
                        steps: Vec::new(),
                        parameters: std::collections::BTreeMap::new(),
                        permissions: Vec::new(),
                        resources: xai_grok_science::workflow::ResourceLimits {
                            max_concurrent_steps: 1,
                            max_total_duration_secs: 30,
                            max_memory_mb: 128,
                            max_disk_mb: 1,
                        },
                        schema_version:
                            xai_grok_science::workflow::WorkflowSpec::CURRENT_SCHEMA_VERSION,
                    },
                },
                executor_root: store_root.clone(),
                kernel_id: "recovery-test-kernel".into(),
                kernel_kind: xai_grok_science::workflow::KernelKind::Python,
                interpreter_path,
                probe_timeout: std::time::Duration::from_secs(1),
                allow_kernel_steps: false,
            };
            let executable =
                xai_grok_science::workflow::PinnedExecutable::pin(&binding.interpreter_path)
                    .unwrap();
            let target = workflow_permission_target(&binding, &executable);
            context.environment.insert(
                WORKFLOW_OPERATION_ID_ENV.into(),
                binding.execution.operation_id.clone(),
            );
            let admission = workflow_admission_sha256(
                &context,
                &binding,
                &executable,
                "revision-recovery-fixture",
                &target,
            )
            .unwrap();
            context
                .environment
                .insert(WORKFLOW_ADMISSION_SHA256_ENV.into(), admission);
            let io = xai_grok_science::workflow::WorkflowIoCapability::open_existing_confined(
                &store_root,
                &workspace,
            )
            .unwrap();
            let executor = xai_grok_science::workflow::WorkflowExecutor::from_io(
                &store_root,
                &io,
                workflow_compute_environment(&binding, Some(executable.sha256())),
            )
            .with_policy(workflow_execution_policy(&binding));
            Self {
                _temp: temp,
                store,
                context,
                binding,
                io,
                executor,
            }
        }

        fn prepare(&self) -> xai_grok_science::Result<WorkflowAuthorityPreparation> {
            prepare_or_recover_workflow_authority(
                &self.store,
                &self.context,
                &self.binding,
                &self.io,
                &self.executor,
            )
        }
    }

    #[test]
    fn created_without_approval_is_repaired_and_pending_retry_is_reused() {
        let fixture = Fixture::new("workflow-recover-pending");
        fixture.store.create_run(fixture.context.clone()).unwrap();

        assert!(matches!(
            fixture.prepare().unwrap(),
            WorkflowAuthorityPreparation::AwaitPermission(_)
        ));
        assert!(matches!(
            fixture.prepare().unwrap(),
            WorkflowAuthorityPreparation::AwaitPermission(_)
        ));
        assert_eq!(
            fixture
                .store
                .load_run(&fixture.context.run_id)
                .unwrap()
                .state,
            xai_grok_science::RunState::AwaitingApproval
        );
        let approvals = fixture.store.approvals(&fixture.context.run_id).unwrap();
        assert_eq!(approvals.len(), 1);
        assert_eq!(
            approvals[0].decision,
            xai_grok_science::ApprovalDecision::Pending
        );
        assert_eq!(
            workflow_events(&fixture.store, &fixture.context.run_id)
                .unwrap()
                .iter()
                .filter(|event| event.kind == "run.created")
                .count(),
            1
        );
        assert!(
            fixture
                .executor
                .lookup_operation(&fixture.binding.execution.operation_id)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn running_allow_without_ledger_returns_internal_recovery_without_prompt() {
        let fixture = Fixture::new("workflow-recover-running-allow");
        let ticket = match fixture.prepare().unwrap() {
            WorkflowAuthorityPreparation::AwaitPermission(ticket) => ticket,
            _ => panic!("fresh workflow did not await permission"),
        };
        xai_grok_science::csv::mark_allowed(&fixture.store, &ticket).unwrap();

        assert!(matches!(
            fixture.prepare().unwrap(),
            WorkflowAuthorityPreparation::ResumeAllowed(ref recovered)
                if recovered.run_id == ticket.run_id
        ));
        assert!(
            fixture
                .executor
                .lookup_operation(&fixture.binding.execution.operation_id)
                .unwrap()
                .is_none(),
            "internal recovery created a workflow ledger before actor Finish"
        );
        assert_eq!(
            workflow_events(&fixture.store, &ticket.run_id)
                .unwrap()
                .iter()
                .filter(|event| event.kind == "approval.allowed")
                .count(),
            1
        );
        require_exact_workflow_events(
            &fixture.store,
            &ticket,
            &[
                workflow_begin_expected(&fixture.context, &fixture.binding).unwrap(),
                workflow_allowed_expected(&ticket),
            ],
            "Running+Allow test",
        )
        .unwrap();
    }

    #[test]
    fn running_allow_recovery_rejects_unknown_event_prefix() {
        let fixture = Fixture::new("workflow-recover-running-dirty-prefix");
        let ticket = match fixture.prepare().unwrap() {
            WorkflowAuthorityPreparation::AwaitPermission(ticket) => ticket,
            _ => panic!("fresh workflow did not await permission"),
        };
        xai_grok_science::csv::mark_allowed(&fixture.store, &ticket).unwrap();
        fixture
            .store
            .append_event(
                &ticket.run_id,
                "attacker",
                "attacker.inserted",
                serde_json::json!({}),
            )
            .unwrap();

        let error = fixture
            .prepare()
            .err()
            .expect("Running+Allow recovery accepted an unknown event");
        assert!(
            error.to_string().contains("requires exactly"),
            "unexpected rejection: {error}"
        );
    }

    #[test]
    fn running_allow_resume_rechecks_ledger_absence_before_execution() {
        let fixture = Fixture::new("workflow-recover-ledger-before-resume");
        let ticket = match fixture.prepare().unwrap() {
            WorkflowAuthorityPreparation::AwaitPermission(ticket) => ticket,
            _ => panic!("fresh workflow did not await permission"),
        };
        xai_grok_science::csv::mark_allowed(&fixture.store, &ticket).unwrap();
        assert!(matches!(
            fixture.prepare().unwrap(),
            WorkflowAuthorityPreparation::ResumeAllowed(_)
        ));

        fixture
            .executor
            .execute(&fixture.binding.execution)
            .unwrap();
        let error = validate_running_allowed_workflow_authority(
            &fixture.store,
            &ticket,
            &fixture.binding,
            &fixture.io,
            &fixture.context,
            &fixture.executor,
        )
        .expect_err("resume accepted a workflow ledger created before execution");
        assert!(
            error.to_string().contains("ledger already exists"),
            "unexpected rejection: {error}"
        );
    }

    #[test]
    fn running_with_durable_finish_event_returns_replay_for_finalization() {
        let fixture = Fixture::new("workflow-recover-after-finish-event");
        let ticket = match fixture.prepare().unwrap() {
            WorkflowAuthorityPreparation::AwaitPermission(ticket) => ticket,
            _ => panic!("fresh workflow did not await permission"),
        };
        xai_grok_science::csv::mark_allowed(&fixture.store, &ticket).unwrap();
        let report = fixture
            .executor
            .execute(&fixture.binding.execution)
            .unwrap();
        fixture
            .store
            .append_recoverable_commit_event(
                &ticket.run_id,
                "SessionActor",
                "workflow.execution.finished",
                workflow_finished_event_payload(&ticket, &report, 0).unwrap(),
            )
            .unwrap();

        let replay = match fixture.prepare().unwrap() {
            WorkflowAuthorityPreparation::Replay {
                ticket: recovered,
                report,
            } if recovered.run_id == ticket.run_id => report,
            _ => panic!("durable finish crash window did not return replay recovery"),
        };
        assert_eq!(
            fixture.store.load_run(&ticket.run_id).unwrap().state,
            xai_grok_science::RunState::Running,
            "prepare recovery performed the Finish transition instead of returning a replay"
        );
        validate_workflow_finished_event(
            &fixture.store,
            &ticket,
            &fixture.binding,
            &fixture.context,
            &replay,
            0,
        )
        .unwrap();
    }

    #[test]
    fn recovery_rejects_context_or_admission_mismatch() {
        let fixture = Fixture::new("workflow-recover-context-mismatch");
        assert!(matches!(
            fixture.prepare().unwrap(),
            WorkflowAuthorityPreparation::AwaitPermission(_)
        ));
        let mut changed = fixture.context.clone();
        changed
            .environment
            .insert(WORKFLOW_ADMISSION_SHA256_ENV.into(), "0".repeat(64));
        let error = prepare_or_recover_workflow_authority(
            &fixture.store,
            &changed,
            &fixture.binding,
            &fixture.io,
            &fixture.executor,
        )
        .err()
        .expect("mismatched admission was accepted");
        assert!(error.to_string().contains("context or admission"));
    }

    #[test]
    fn succeeded_authority_without_workflow_ledger_is_corruption() {
        let fixture = Fixture::new("workflow-recover-succeeded-no-ledger");
        let ticket = match fixture.prepare().unwrap() {
            WorkflowAuthorityPreparation::AwaitPermission(ticket) => ticket,
            _ => panic!("fresh workflow did not await permission"),
        };
        xai_grok_science::csv::mark_allowed(&fixture.store, &ticket).unwrap();
        fixture
            .store
            .transition_succeeded_verified(&ticket.run_id)
            .unwrap();

        let error = fixture
            .prepare()
            .err()
            .expect("Succeeded authority without ledger was accepted");
        assert!(error.to_string().contains("missing its workflow ledger"));
    }

    #[test]
    fn workflow_ledger_without_session_authority_is_rejected() {
        let fixture = Fixture::new("workflow-recover-ledger-no-authority");
        let report = fixture
            .executor
            .execute(&fixture.binding.execution)
            .unwrap();
        assert_eq!(
            report.run.state,
            xai_grok_science::workflow::WorkflowState::Succeeded
        );

        let error = fixture
            .prepare()
            .err()
            .expect("workflow ledger without SessionActor authority was accepted");
        assert!(
            error
                .to_string()
                .contains("without its SessionActor authority")
        );
    }
}

/// What the permission prompt names. A workflow step spawns an interpreter, so
/// the prompt says which interpreter and which workflow — not merely "a
/// workflow ran".
fn workflow_permission_target(
    binding: &crate::session::commands::ScienceWorkflowBinding,
    executable: &xai_grok_science::workflow::PinnedExecutable,
) -> String {
    format!(
        "execute workflow '{}' ({} step(s)) on {} [sha256:{}; {}]",
        binding.execution.spec.workflow_id,
        binding.execution.spec.steps.len(),
        binding.interpreter_path.display(),
        &executable.sha256()[..12],
        executable.backend(),
    )
}

/// The compute environment recorded against a workflow run.
///
/// HONESTY NOTE: `lumen_binary_hash` is a version label, NOT a digest of this
/// executable. Hashing a multi-hundred-megabyte binary on every execution is
/// not something to do silently, and a field that says `version:` cannot be
/// mistaken for one that says `sha256:`. Reproduction across builds therefore
/// rests on the version string, and that limit is stated rather than papered
/// over with a plausible-looking hash.
fn workflow_compute_environment(
    binding: &crate::session::commands::ScienceWorkflowBinding,
    executable_sha256: Option<&str>,
) -> xai_grok_science::workflow::ComputeEnvironment {
    xai_grok_science::workflow::ComputeEnvironment {
        environment_id: format!("session-actor:{}", binding.kernel_id),
        os: std::env::consts::OS.to_string(),
        architecture: std::env::consts::ARCH.to_string(),
        lumen_binary_hash: format!("version:{}", xai_grok_version::VERSION),
        rust_lock_hash: None,
        python_hash: (binding.kernel_kind == xai_grok_science::workflow::KernelKind::Python)
            .then(|| executable_sha256.map(|hash| format!("sha256:{hash}")))
            .flatten(),
        r_hash: (binding.kernel_kind == xai_grok_science::workflow::KernelKind::R)
            .then(|| executable_sha256.map(|hash| format!("sha256:{hash}")))
            .flatten(),
        julia_hash: (binding.kernel_kind == xai_grok_science::workflow::KernelKind::Julia)
            .then(|| executable_sha256.map(|hash| format!("sha256:{hash}")))
            .flatten(),
        dependency_lock_hash: format!("version:{}", xai_grok_version::VERSION),
        locale: "C".into(),
        timezone: "UTC".into(),
        environment_allowlist: vec![
            format!("interpreter={}", binding.interpreter_path.display()),
            format!("kernel_id={}", binding.kernel_id),
            format!("kernel_kind={:?}", binding.kernel_kind),
            format!("probe_timeout_ms={}", binding.probe_timeout.as_millis()),
        ],
        cpu_identity: None,
        gpu_identity: None,
        deterministic_flags: vec![
            "PYTHONHASHSEED=0".into(),
            format!("allow_kernel_steps={}", binding.allow_kernel_steps),
        ],
        network_policy: xai_grok_science::workflow::NetworkPolicy::None,
        container_digest: None,
    }
}

fn workflow_execution_policy(
    binding: &crate::session::commands::ScienceWorkflowBinding,
) -> xai_grok_science::workflow::ExecutionPolicy {
    if binding.allow_kernel_steps {
        xai_grok_science::workflow::ExecutionPolicy::default().allowing_kernel_steps()
    } else {
        xai_grok_science::workflow::ExecutionPolicy::default()
    }
}

/// Open the durable run + pending approval for a workflow execution. Mirrors
/// `begin_project_mutation_run`, with a call id that names this product path.
fn begin_workflow_execution_run(
    store: &xai_grok_science::ScienceStore,
    context: xai_grok_science::RunContext,
    binding: &crate::session::commands::ScienceWorkflowBinding,
) -> xai_grok_science::Result<xai_grok_science::csv::ScienceRunTicket> {
    let ticket = workflow_ticket(&context)?;
    let begin = workflow_begin_expected(&context, binding)?;
    store.create_run(context)?;
    ensure_exact_next_workflow_event(store, &ticket, &[], begin, "fresh Begin")?;
    store.request_approval(xai_grok_science::Approval {
        project_id: ticket.project_id.clone(),
        run_id: ticket.run_id.clone(),
        call_id: ticket.call_id.clone(),
        owner_id: ticket.owner_id.clone(),
        decision: xai_grok_science::ApprovalDecision::Pending,
        decided_at: None,
    })?;
    store.transition(
        &ticket.run_id,
        xai_grok_science::RunState::AwaitingApproval,
        None,
    )?;
    Ok(ticket)
}

/// Open the durable run + pending approval for a project mutation. Mirrors
/// `csv::begin_fixture`, with a call id that names this product path.
fn project_mutation_begin_event_payload(
    context: &xai_grok_science::RunContext,
    kind: &str,
    operation_id: &str,
) -> serde_json::Value {
    serde_json::json!({
        "mutation": kind,
        "operation_id": operation_id,
        "migration_admission_sha256": context
            .environment
            .get("project_migration_admission_sha256"),
        "review_admission_sha256": context
            .environment
            .get(xai_grok_science::project::ReviewAdmission::ENV_ADMISSION_SHA256),
        "review_request_sha256": context
            .environment
            .get(xai_grok_science::project::ReviewAdmission::ENV_REQUEST_SHA256),
        "review_source_authority_sha256": context
            .environment
            .get(xai_grok_science::project::ReviewAdmission::ENV_SOURCE_AUTHORITY_SHA256),
        "review_project_revision": context
            .environment
            .get(xai_grok_science::project::ReviewAdmission::ENV_PROJECT_REVISION),
    })
}

fn begin_project_mutation_run(
    store: &xai_grok_science::ScienceStore,
    context: xai_grok_science::RunContext,
    kind: &str,
    operation_id: &str,
) -> xai_grok_science::Result<xai_grok_science::csv::ScienceRunTicket> {
    let ticket = xai_grok_science::csv::ScienceRunTicket {
        project_id: context.project_id.clone(),
        run_id: context.run_id.clone(),
        owner_id: context.owner_id.clone(),
        call_id: xai_grok_science::CallId::new("science_project_mutation"),
    };
    let event_payload = project_mutation_begin_event_payload(&context, kind, operation_id);
    store.create_run(context)?;
    store.append_recoverable_commit_event(
        &ticket.run_id,
        "SessionActor",
        "run.created",
        event_payload,
    )?;
    store.request_approval(xai_grok_science::Approval {
        project_id: ticket.project_id.clone(),
        run_id: ticket.run_id.clone(),
        call_id: ticket.call_id.clone(),
        owner_id: ticket.owner_id.clone(),
        decision: xai_grok_science::ApprovalDecision::Pending,
        decided_at: None,
    })?;
    store.transition(
        &ticket.run_id,
        xai_grok_science::RunState::AwaitingApproval,
        None,
    )?;
    Ok(ticket)
}

enum ReviewAuthorityPreparation {
    AwaitPermission(xai_grok_science::csv::ScienceRunTicket),
    ResumeAllowed(xai_grok_science::csv::ScienceRunTicket),
}

fn project_mutation_ticket(
    context: &xai_grok_science::RunContext,
) -> xai_grok_science::csv::ScienceRunTicket {
    xai_grok_science::csv::ScienceRunTicket {
        project_id: context.project_id.clone(),
        run_id: context.run_id.clone(),
        owner_id: context.owner_id.clone(),
        call_id: xai_grok_science::CallId::new("science_project_mutation"),
    }
}

fn project_mutation_events(
    store: &xai_grok_science::ScienceStore,
    run_id: &xai_grok_science::RunId,
) -> xai_grok_science::Result<Vec<xai_grok_science::Event>> {
    let events = store.events_after(run_id, 0, 1_000)?;
    if events.len() == 1_000 {
        return Err(xai_grok_science::ScienceError::Invalid(
            "project mutation authority event log is too large to verify".into(),
        ));
    }
    Ok(events)
}

fn validate_review_authority_event_prefix(
    store: &xai_grok_science::ScienceStore,
    ticket: &xai_grok_science::csv::ScienceRunTicket,
    context: &xai_grok_science::RunContext,
    request: &xai_grok_science::project::MutationRequest,
    allowed: bool,
) -> xai_grok_science::Result<Vec<xai_grok_science::Event>> {
    let events = project_mutation_events(store, &ticket.run_id)?;
    let expected_len = if allowed { 2 } else { 1 };
    if events.len() != expected_len {
        return Err(xai_grok_science::ScienceError::Invalid(
            "review authority contains an unknown, missing, or duplicate pre-finish event".into(),
        ));
    }
    let begin = &events[0];
    let begin_payload =
        project_mutation_begin_event_payload(context, "review_record", &request.operation_id);
    if begin.run_id != ticket.run_id
        || begin.actor != "SessionActor"
        || begin.kind != "run.created"
        || begin.payload != begin_payload
    {
        return Err(xai_grok_science::ScienceError::Invalid(
            "review authority run.created event differs from its admission".into(),
        ));
    }
    if allowed {
        let event = &events[1];
        if event.run_id != ticket.run_id
            || event.actor != "LumenApproval"
            || event.kind != "approval.allowed"
            || event.payload != serde_json::json!({"call_id": ticket.call_id.0})
        {
            return Err(xai_grok_science::ScienceError::Invalid(
                "review authority approval.allowed event differs from its call".into(),
            ));
        }
    }
    Ok(events)
}

fn validate_migration_authority_event_prefix(
    store: &xai_grok_science::ScienceStore,
    ticket: &xai_grok_science::csv::ScienceRunTicket,
    context: &xai_grok_science::RunContext,
    request: &xai_grok_science::project::MutationRequest,
    allowed: bool,
) -> xai_grok_science::Result<Vec<xai_grok_science::Event>> {
    if !matches!(
        request.mutation,
        xai_grok_science::project::ProjectMutation::ProjectMigrate { .. }
    ) {
        return Err(xai_grok_science::ScienceError::Invalid(
            "migration event validation received another mutation kind".into(),
        ));
    }
    let events = project_mutation_events(store, &ticket.run_id)?;
    let expected_len = if allowed { 2 } else { 1 };
    if events.len() != expected_len {
        return Err(xai_grok_science::ScienceError::Invalid(
            "migration authority contains an unknown, missing, or duplicate pre-finish event"
                .into(),
        ));
    }
    let begin = &events[0];
    if begin.run_id != ticket.run_id
        || begin.actor != "SessionActor"
        || begin.kind != "run.created"
        || begin.payload
            != project_mutation_begin_event_payload(
                context,
                "project_migrate",
                &request.operation_id,
            )
    {
        return Err(xai_grok_science::ScienceError::Invalid(
            "migration authority run.created event differs from its admission".into(),
        ));
    }
    if allowed {
        let event = &events[1];
        if event.run_id != ticket.run_id
            || event.actor != "LumenApproval"
            || event.kind != "approval.allowed"
            || event.payload != serde_json::json!({"call_id": ticket.call_id.0})
        {
            return Err(xai_grok_science::ScienceError::Invalid(
                "migration authority approval.allowed event differs from its call".into(),
            ));
        }
    }
    Ok(events)
}

fn ensure_created_migration_begin_event(
    store: &xai_grok_science::ScienceStore,
    project_store: &xai_grok_science::project::ProjectStore,
    ticket: &xai_grok_science::csv::ScienceRunTicket,
    context: &xai_grok_science::RunContext,
    request: &xai_grok_science::project::MutationRequest,
) -> xai_grok_science::Result<()> {
    ensure_empty_project_mutation_outputs(store, &ticket.run_id)?;
    if project_store
        .lookup_operation(&request.operation_id)?
        .is_some()
        || project_store
            .lookup_migration_commit(&request.operation_id)?
            .is_some()
    {
        return Err(xai_grok_science::ScienceError::Invalid(
            "Created migration authority already has a project or commit ledger".into(),
        ));
    }
    if project_mutation_events(store, &ticket.run_id)?.is_empty() {
        store.append_recoverable_commit_event(
            &ticket.run_id,
            "SessionActor",
            "run.created",
            project_mutation_begin_event_payload(context, "project_migrate", &request.operation_id),
        )?;
    }
    validate_migration_authority_event_prefix(store, ticket, context, request, false)?;
    Ok(())
}

fn ensure_migration_allowed_event(
    store: &xai_grok_science::ScienceStore,
    ticket: &xai_grok_science::csv::ScienceRunTicket,
    context: &xai_grok_science::RunContext,
    request: &xai_grok_science::project::MutationRequest,
) -> xai_grok_science::Result<()> {
    match validate_migration_authority_event_prefix(store, ticket, context, request, false) {
        Ok(_) => {
            store.append_recoverable_commit_event(
                &ticket.run_id,
                "LumenApproval",
                "approval.allowed",
                serde_json::json!({"call_id": ticket.call_id.0}),
            )?;
        }
        Err(_) => {
            validate_migration_authority_event_prefix(store, ticket, context, request, true)?;
            return Ok(());
        }
    }
    validate_migration_authority_event_prefix(store, ticket, context, request, true)?;
    Ok(())
}

fn recover_migration_terminal_decision(
    store: &xai_grok_science::ScienceStore,
    project_store: &xai_grok_science::project::ProjectStore,
    ticket: &xai_grok_science::csv::ScienceRunTicket,
    context: &xai_grok_science::RunContext,
    request: &xai_grok_science::project::MutationRequest,
    decision: xai_grok_science::ApprovalDecision,
) -> xai_grok_science::Result<xai_grok_science::RunState> {
    use xai_grok_science::{ApprovalDecision, RunState, ScienceError};

    ensure_empty_project_mutation_outputs(store, &ticket.run_id)?;
    if project_store
        .lookup_operation(&request.operation_id)?
        .is_some()
        || project_store
            .lookup_migration_commit(&request.operation_id)?
            .is_some()
    {
        return Err(ScienceError::Invalid(
            "terminal migration recovery found a post-Allow ledger".into(),
        ));
    }
    let (state, kind) = match decision {
        ApprovalDecision::Deny => (RunState::Denied, "approval.denied"),
        ApprovalDecision::Timeout => (RunState::TimedOut, "approval.timed_out"),
        ApprovalDecision::Cancel => (RunState::Cancelled, "approval.cancelled"),
        ApprovalDecision::Interrupted => (RunState::Interrupted, "approval.interrupted"),
        _ => {
            return Err(ScienceError::Invalid(
                "migration terminal recovery requires a non-Allow decision".into(),
            ));
        }
    };
    let mut reason = "recovered durable migration permission decision".to_string();
    let expected_terminal = serde_json::json!({"call_id": ticket.call_id.0, "reason": reason});
    let events = project_mutation_events(store, &ticket.run_id)?;
    match events.as_slice() {
        [_] => {
            validate_migration_authority_event_prefix(store, ticket, context, request, false)?;
            store.append_recoverable_commit_event(
                &ticket.run_id,
                "LumenApproval",
                kind,
                expected_terminal.clone(),
            )?;
        }
        [begin, terminal]
            if begin.run_id == ticket.run_id
                && begin.actor == "SessionActor"
                && begin.kind == "run.created"
                && begin.payload
                    == project_mutation_begin_event_payload(
                        context,
                        "project_migrate",
                        &request.operation_id,
                    )
                && terminal.run_id == ticket.run_id
                && terminal.actor == "LumenApproval"
                && terminal.kind == kind
                && terminal.payload.get("call_id")
                    == Some(&serde_json::json!(ticket.call_id.0))
                && terminal
                    .payload
                    .get("reason")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|value| !value.is_empty() && value.len() <= 64 * 1024) =>
        {
            reason = terminal.payload["reason"]
                .as_str()
                .expect("terminal reason checked above")
                .to_string();
        }
        _ => {
            return Err(ScienceError::Invalid(
                "migration terminal recovery found an unknown, missing, or duplicate event".into(),
            ));
        }
    }
    Ok(store.transition(&ticket.run_id, state, Some(reason))?.state)
}

fn exact_project_mutation_approval(
    store: &xai_grok_science::ScienceStore,
    ticket: &xai_grok_science::csv::ScienceRunTicket,
) -> xai_grok_science::Result<Option<xai_grok_science::Approval>> {
    let approvals = store.approvals(&ticket.run_id)?;
    let approval = match approvals.as_slice() {
        [] => return Ok(None),
        [approval] => approval,
        _ => {
            return Err(xai_grok_science::ScienceError::Invalid(
                "project mutation authority requires at most one approval".into(),
            ));
        }
    };
    if approval.project_id != ticket.project_id
        || approval.run_id != ticket.run_id
        || approval.call_id != ticket.call_id
        || approval.owner_id != ticket.owner_id
    {
        return Err(xai_grok_science::ScienceError::Ownership);
    }
    if (approval.decision == xai_grok_science::ApprovalDecision::Pending)
        != approval.decided_at.is_none()
    {
        return Err(xai_grok_science::ScienceError::Invalid(
            "project mutation approval decision and timestamp disagree".into(),
        ));
    }
    Ok(Some(approval.clone()))
}

fn ensure_empty_project_mutation_outputs(
    store: &xai_grok_science::ScienceStore,
    run_id: &xai_grok_science::RunId,
) -> xai_grok_science::Result<()> {
    if !store.artifacts(run_id)?.is_empty()
        || !store.evidence(run_id)?.is_empty()
        || !store.provenance(run_id)?.is_empty()
        || !store.previews(run_id)?.is_empty()
    {
        return Err(xai_grok_science::ScienceError::Invalid(
            "project mutation authority has outputs before its admitted commit".into(),
        ));
    }
    Ok(())
}

fn ensure_review_ledgers_absent(
    project_store: &xai_grok_science::project::ProjectStore,
    request: &xai_grok_science::project::MutationRequest,
) -> xai_grok_science::Result<()> {
    let xai_grok_science::project::ProjectMutation::ReviewRecord { project_id, .. } =
        &request.mutation
    else {
        return Err(xai_grok_science::ScienceError::Invalid(
            "review recovery received another mutation kind".into(),
        ));
    };
    if project_store
        .lookup_operation(&request.operation_id)?
        .is_some()
        || project_store
            .lookup_review_record(project_id, &request.operation_id)?
            .is_some()
    {
        return Err(xai_grok_science::ScienceError::Invalid(
            "review pre-ledger recovery found a partial project commit".into(),
        ));
    }
    Ok(())
}

fn ensure_review_allowed_event(
    store: &xai_grok_science::ScienceStore,
    ticket: &xai_grok_science::csv::ScienceRunTicket,
    context: &xai_grok_science::RunContext,
    request: &xai_grok_science::project::MutationRequest,
) -> xai_grok_science::Result<()> {
    match validate_review_authority_event_prefix(store, ticket, context, request, false) {
        Ok(_) => {
            store.append_recoverable_commit_event(
                &ticket.run_id,
                "LumenApproval",
                "approval.allowed",
                serde_json::json!({"call_id": ticket.call_id.0}),
            )?;
        }
        Err(_) => {
            validate_review_authority_event_prefix(store, ticket, context, request, true)?;
            return Ok(());
        }
    }
    validate_review_authority_event_prefix(store, ticket, context, request, true)?;
    Ok(())
}

fn recover_review_terminal_decision(
    store: &xai_grok_science::ScienceStore,
    ticket: &xai_grok_science::csv::ScienceRunTicket,
    context: &xai_grok_science::RunContext,
    request: &xai_grok_science::project::MutationRequest,
    decision: xai_grok_science::ApprovalDecision,
) -> xai_grok_science::Result<xai_grok_science::RunState> {
    use xai_grok_science::{ApprovalDecision, RunState, ScienceError};

    ensure_empty_project_mutation_outputs(store, &ticket.run_id)?;
    let (state, kind) = match decision {
        ApprovalDecision::Deny => (RunState::Denied, "approval.denied"),
        ApprovalDecision::Timeout => (RunState::TimedOut, "approval.timed_out"),
        ApprovalDecision::Cancel => (RunState::Cancelled, "approval.cancelled"),
        ApprovalDecision::Interrupted => (RunState::Interrupted, "approval.interrupted"),
        _ => {
            return Err(ScienceError::Invalid(
                "review terminal recovery requires a non-Allow decision".into(),
            ));
        }
    };
    let reason = "recovered durable review permission decision";
    let expected_terminal = serde_json::json!({"call_id": ticket.call_id.0, "reason": reason});
    let events = project_mutation_events(store, &ticket.run_id)?;
    match events.as_slice() {
        [_] => {
            validate_review_authority_event_prefix(store, ticket, context, request, false)?;
            store.append_recoverable_commit_event(
                &ticket.run_id,
                "LumenApproval",
                kind,
                expected_terminal.clone(),
            )?;
        }
        [begin, terminal]
            if begin.actor == "SessionActor"
                && begin.kind == "run.created"
                && begin.payload
                    == project_mutation_begin_event_payload(
                        context,
                        "review_record",
                        &request.operation_id,
                    )
                && terminal.actor == "LumenApproval"
                && terminal.kind == kind
                && terminal.payload == expected_terminal => {}
        _ => {
            return Err(ScienceError::Invalid(
                "review terminal recovery found an unknown, missing, or duplicate event".into(),
            ));
        }
    }
    Ok(store
        .transition(&ticket.run_id, state, Some(reason.into()))?
        .state)
}

fn prepare_or_recover_review_authority(
    store: &xai_grok_science::ScienceStore,
    project_store: &xai_grok_science::project::ProjectStore,
    context: &xai_grok_science::RunContext,
    request: &xai_grok_science::project::MutationRequest,
    admission: &xai_grok_science::project::ReviewAdmission,
) -> xai_grok_science::Result<ReviewAuthorityPreparation> {
    use xai_grok_science::{Approval, ApprovalDecision, RunState, ScienceError};

    if admission.request_sha256() != request.replay_fingerprint()?
        || admission.authority_run_id() != context.run_id.0
        || admission.owner_id() != context.owner_id
        || admission.session_id() != context.session_id
        || admission.project_id().0 != context.project_id.0
    {
        return Err(ScienceError::Ownership);
    }
    for (key, expected) in admission.authority_environment() {
        if context.environment.get(&key) != Some(&expected) {
            return Err(ScienceError::Invalid(format!(
                "review authority context is missing exact {key}"
            )));
        }
    }

    let ticket = project_mutation_ticket(context);
    let Some(run) = store.load_run_optional(&ticket.run_id)? else {
        let ticket = begin_project_mutation_run(
            store,
            context.clone(),
            "review_record",
            &request.operation_id,
        )?;
        return Ok(ReviewAuthorityPreparation::AwaitPermission(ticket));
    };
    if run.context != *context {
        return Err(ScienceError::Ownership);
    }

    if run.state == RunState::Succeeded {
        return Err(ScienceError::Invalid(
            "succeeded review authority is missing its project ledgers".into(),
        ));
    }
    if run.state.terminal() {
        ensure_review_ledgers_absent(project_store, request)?;
        ensure_empty_project_mutation_outputs(store, &ticket.run_id)?;
        return Err(ScienceError::Invalid(format!(
            "review authority {} is already terminal {:?}",
            ticket.run_id.0, run.state
        )));
    }

    match run.state {
        RunState::Created => {
            let events = project_mutation_events(store, &ticket.run_id)?;
            if events.is_empty() {
                store.append_recoverable_commit_event(
                    &ticket.run_id,
                    "SessionActor",
                    "run.created",
                    project_mutation_begin_event_payload(
                        context,
                        "review_record",
                        &request.operation_id,
                    ),
                )?;
            }
            validate_review_authority_event_prefix(store, &ticket, context, request, false)?;
            ensure_review_ledgers_absent(project_store, request)?;
            ensure_empty_project_mutation_outputs(store, &ticket.run_id)?;
            match exact_project_mutation_approval(store, &ticket)? {
                None => store.request_approval(Approval {
                    project_id: ticket.project_id.clone(),
                    run_id: ticket.run_id.clone(),
                    call_id: ticket.call_id.clone(),
                    owner_id: ticket.owner_id.clone(),
                    decision: ApprovalDecision::Pending,
                    decided_at: None,
                })?,
                Some(approval)
                    if approval.decision == ApprovalDecision::Pending
                        && approval.decided_at.is_none() => {}
                Some(_) => {
                    return Err(ScienceError::Invalid(
                        "Created review authority has a terminal approval".into(),
                    ));
                }
            }
            store.transition(&ticket.run_id, RunState::AwaitingApproval, None)?;
            Ok(ReviewAuthorityPreparation::AwaitPermission(ticket))
        }
        RunState::AwaitingApproval => {
            ensure_review_ledgers_absent(project_store, request)?;
            ensure_empty_project_mutation_outputs(store, &ticket.run_id)?;
            let approval = exact_project_mutation_approval(store, &ticket)?.ok_or_else(|| {
                ScienceError::Invalid(
                    "AwaitingApproval review authority is missing its approval".into(),
                )
            })?;
            match approval.decision {
                ApprovalDecision::Pending if approval.decided_at.is_none() => {
                    validate_review_authority_event_prefix(
                        store, &ticket, context, request, false,
                    )?;
                    Ok(ReviewAuthorityPreparation::AwaitPermission(ticket))
                }
                ApprovalDecision::Allow if approval.decided_at.is_some() => {
                    ensure_review_allowed_event(store, &ticket, context, request)?;
                    store.transition(&ticket.run_id, RunState::Running, None)?;
                    Ok(ReviewAuthorityPreparation::ResumeAllowed(ticket))
                }
                ApprovalDecision::Deny
                | ApprovalDecision::Timeout
                | ApprovalDecision::Cancel
                | ApprovalDecision::Interrupted
                    if approval.decided_at.is_some() =>
                {
                    let state = recover_review_terminal_decision(
                        store,
                        &ticket,
                        context,
                        request,
                        approval.decision,
                    )?;
                    Err(ScienceError::Invalid(format!(
                        "review authority {} recovered terminal {state:?}",
                        ticket.run_id.0
                    )))
                }
                _ => Err(ScienceError::Invalid(
                    "review approval state cannot be recovered".into(),
                )),
            }
        }
        RunState::Running => {
            ensure_review_ledgers_absent(project_store, request)?;
            ensure_empty_project_mutation_outputs(store, &ticket.run_id)?;
            let approval = exact_project_mutation_approval(store, &ticket)?.ok_or_else(|| {
                ScienceError::Invalid("Running review authority is missing Allow".into())
            })?;
            if approval.decision != ApprovalDecision::Allow || approval.decided_at.is_none() {
                return Err(ScienceError::Invalid(
                    "Running review authority is not bound to durable Allow".into(),
                ));
            }
            validate_review_authority_event_prefix(store, &ticket, context, request, true)?;
            Ok(ReviewAuthorityPreparation::ResumeAllowed(ticket))
        }
        _ => unreachable!("terminal review states returned above"),
    }
}

fn validate_project_mutation_retained_authority(
    prepared: &PreparedScienceProjectMutation,
) -> xai_grok_science::Result<xai_grok_science::RunRecord> {
    use xai_grok_science::ScienceError;

    if prepared.store.root() != prepared.project_root
        || !prepared
            .store
            .shares_root_capability_with(&prepared.project_store)?
        || prepared.expected_context.artifact_root != prepared.store.root().join("runs")
        || prepared.ticket.run_id != prepared.expected_context.run_id
        || prepared.ticket.project_id != prepared.expected_context.project_id
        || prepared.ticket.owner_id != prepared.expected_context.owner_id
        || prepared.ticket.call_id != xai_grok_science::CallId::new("science_project_mutation")
        || prepared.request.owner_id != prepared.expected_context.owner_id
        || prepared.request.session_id != prepared.expected_context.session_id
    {
        return Err(ScienceError::Ownership);
    }
    if let Some(project_id) = prepared.request.mutation.target_project()
        && project_id.0 != prepared.expected_context.project_id.0
    {
        return Err(ScienceError::Ownership);
    }
    let run = prepared.store.load_run(&prepared.ticket.run_id)?;
    if run.context != prepared.expected_context {
        return Err(ScienceError::Ownership);
    }
    Ok(run)
}

fn validate_pending_project_mutation_authority(
    prepared: &PreparedScienceProjectMutation,
) -> xai_grok_science::Result<()> {
    use xai_grok_science::{ApprovalDecision, RunState, ScienceError};

    let run = validate_project_mutation_retained_authority(prepared)?;
    if run.state != RunState::AwaitingApproval {
        return Err(ScienceError::Invalid(
            "fresh project mutation Allow requires AwaitingApproval".into(),
        ));
    }
    let approval = exact_project_mutation_approval(&prepared.store, &prepared.ticket)?
        .ok_or_else(|| ScienceError::Invalid("project mutation approval is missing".into()))?;
    if approval.decision != ApprovalDecision::Pending || approval.decided_at.is_some() {
        return Err(ScienceError::Invalid(
            "fresh project mutation Allow requires one durable Pending approval".into(),
        ));
    }
    ensure_empty_project_mutation_outputs(&prepared.store, &prepared.ticket.run_id)?;
    if prepared
        .project_store
        .lookup_operation(&prepared.request.operation_id)?
        .is_some()
    {
        return Err(ScienceError::Invalid(
            "fresh project mutation already has an operation ledger".into(),
        ));
    }
    if matches!(
        prepared.request.mutation,
        xai_grok_science::project::ProjectMutation::ProjectMigrate { .. }
    ) && prepared
        .project_store
        .lookup_migration_commit(&prepared.request.operation_id)?
        .is_some()
    {
        return Err(ScienceError::Invalid(
            "fresh migration already has a post-Allow commit journal".into(),
        ));
    }
    if matches!(
        prepared.request.mutation,
        xai_grok_science::project::ProjectMutation::ReviewRecord { .. }
    ) {
        ensure_review_ledgers_absent(&prepared.project_store, &prepared.request)?;
        validate_review_authority_event_prefix(
            &prepared.store,
            &prepared.ticket,
            &prepared.expected_context,
            &prepared.request,
            false,
        )?;
    } else {
        let events = project_mutation_events(&prepared.store, &prepared.ticket.run_id)?;
        let [begin] = events.as_slice() else {
            return Err(ScienceError::Invalid(
                "fresh project mutation requires exactly one run.created event".into(),
            ));
        };
        if begin.actor != "SessionActor"
            || begin.kind != "run.created"
            || begin.payload
                != project_mutation_begin_event_payload(
                    &prepared.expected_context,
                    prepared.request.mutation.kind(),
                    &prepared.request.operation_id,
                )
        {
            return Err(ScienceError::Invalid(
                "project mutation run.created event differs from its admitted request".into(),
            ));
        }
    }
    Ok(())
}

fn require_exact_project_mutation_allow_grant(
    prepared: &PreparedScienceProjectMutation,
) -> xai_grok_science::Result<()> {
    use xai_grok_science::{ApprovalDecision, ScienceError};

    let authority = validate_pending_project_mutation_authority(prepared);
    let grant_matches = prepared
        .permission_grant
        .as_ref()
        .map(|grant| grant.authorizes(prepared))
        .transpose();
    if authority.is_ok() && matches!(grant_matches, Ok(Some(true))) {
        return Ok(());
    }

    let reason = match (authority, grant_matches) {
        (Err(error), _) => format!("project mutation authority validation failed: {error}"),
        (_, Err(error)) => format!("project mutation Allow grant validation failed: {error}"),
        _ => "project mutation Allow is missing its exact private permission grant".into(),
    };
    let terminalization = if let Ok(run) = prepared.store.load_run(&prepared.ticket.run_id)
        && run.state == xai_grok_science::RunState::AwaitingApproval
        && exact_project_mutation_approval(&prepared.store, &prepared.ticket)
            .ok()
            .flatten()
            .is_some_and(|approval| approval.decision == ApprovalDecision::Pending)
    {
        Some(xai_grok_science::csv::finish_without_execution(
            &prepared.store,
            &prepared.ticket,
            ApprovalDecision::Deny,
            reason.clone(),
        ))
    } else {
        None
    };
    match terminalization {
        Some(Err(error)) => Err(ScienceError::Invalid(format!(
            "{reason}; durable Deny terminalization failed and recovery is required: {error}"
        ))),
        Some(Ok(_)) | None => Err(ScienceError::Invalid(reason)),
    }
}

fn validate_running_project_mutation_authority(
    prepared: &PreparedScienceProjectMutation,
) -> xai_grok_science::Result<()> {
    use xai_grok_science::{ApprovalDecision, RunState, ScienceError};

    let run = validate_project_mutation_retained_authority(prepared)?;
    if run.state != RunState::Running {
        return Err(ScienceError::Invalid(
            "resumed project mutation requires its original Running authority".into(),
        ));
    }
    let approval = exact_project_mutation_approval(&prepared.store, &prepared.ticket)?
        .ok_or_else(|| ScienceError::Invalid("Running project mutation is missing Allow".into()))?;
    if approval.decision != ApprovalDecision::Allow || approval.decided_at.is_none() {
        return Err(ScienceError::Invalid(
            "Running project mutation is not bound to one durable Allow".into(),
        ));
    }

    if matches!(
        prepared.request.mutation,
        xai_grok_science::project::ProjectMutation::ReviewRecord { .. }
    ) {
        ensure_review_ledgers_absent(&prepared.project_store, &prepared.request)?;
        ensure_empty_project_mutation_outputs(&prepared.store, &prepared.ticket.run_id)?;
        validate_review_authority_event_prefix(
            &prepared.store,
            &prepared.ticket,
            &prepared.expected_context,
            &prepared.request,
            true,
        )?;
        let admission = prepared.review_admission.as_ref().ok_or_else(|| {
            ScienceError::Invalid("resumed review authority lost its immutable admission".into())
        })?;
        if prepared.project_revision.as_deref() != Some(admission.project_revision()) {
            return Err(ScienceError::Invalid(
                "resumed review project revision differs from admission".into(),
            ));
        }
        for (key, expected) in admission.authority_environment() {
            if prepared.expected_context.environment.get(&key) != Some(&expected) {
                return Err(ScienceError::Invalid(format!(
                    "resumed review authority is missing exact {key}"
                )));
            }
        }
    } else if matches!(
        prepared.request.mutation,
        xai_grok_science::project::ProjectMutation::ProjectMigrate { .. }
    ) {
        ensure_empty_project_mutation_outputs(&prepared.store, &prepared.ticket.run_id)?;
        if prepared
            .project_store
            .lookup_operation(&prepared.request.operation_id)?
            .is_some()
            || prepared
                .project_store
                .lookup_migration_commit(&prepared.request.operation_id)?
                .is_some()
        {
            return Err(ScienceError::Invalid(
                "resumed pre-commit migration already has a project or commit ledger".into(),
            ));
        }
        let admission = prepared.migration_admission.as_ref().ok_or_else(|| {
            ScienceError::Invalid("resumed migration authority lost its immutable admission".into())
        })?;
        let admission_sha256 = admission.sha256()?;
        if prepared
            .expected_context
            .environment
            .get("project_migration_admission_sha256")
            != Some(&admission_sha256)
        {
            return Err(ScienceError::Invalid(
                "resumed migration authority differs from its admitted source digest".into(),
            ));
        }
        validate_migration_authority_event_prefix(
            &prepared.store,
            &prepared.ticket,
            &prepared.expected_context,
            &prepared.request,
            true,
        )?;
    }
    Ok(())
}

fn interrupt_pending_project_mutation_authority(
    prepared: &PreparedScienceProjectMutation,
    reason: &str,
) -> xai_grok_science::Result<()> {
    validate_pending_project_mutation_authority(prepared)?;
    prepared.store.decide_approval(
        &prepared.ticket.project_id,
        &prepared.ticket.run_id,
        &prepared.ticket.owner_id,
        &prepared.ticket.call_id,
        xai_grok_science::ApprovalDecision::Interrupted,
    )?;
    prepared.store.append_recoverable_commit_event(
        &prepared.ticket.run_id,
        "LumenApproval",
        "approval.interrupted",
        serde_json::json!({
            "call_id": prepared.ticket.call_id.0,
            "reason": reason,
        }),
    )?;
    prepared.store.transition(
        &prepared.ticket.run_id,
        xai_grok_science::RunState::Interrupted,
        Some(reason.into()),
    )?;
    Ok(())
}

fn terminalize_project_mutation_failure(
    store: &xai_grok_science::ScienceStore,
    ticket: &xai_grok_science::csv::ScienceRunTicket,
    reason: String,
    original: xai_grok_science::ScienceError,
) -> xai_grok_science::ScienceError {
    match xai_grok_science::csv::fail_running(store, ticket, reason) {
        Ok(_) => original,
        Err(terminalization) => xai_grok_science::ScienceError::Invalid(format!(
            "{original}; durable Failed terminalization failed and recovery is required: {terminalization}"
        )),
    }
}

fn rollback_migration_outputs(
    store: &xai_grok_science::ScienceStore,
    ticket: &xai_grok_science::csv::ScienceRunTicket,
    paths: &[std::path::PathBuf],
) {
    let path_refs = paths
        .iter()
        .map(std::path::PathBuf::as_path)
        .collect::<Vec<_>>();
    if let Err(error) = store.discard_running_outputs(
        &ticket.project_id,
        &ticket.run_id,
        &ticket.owner_id,
        &ticket.call_id,
        &path_refs,
    ) {
        tracing::warn!(
            run_id = %ticket.run_id.0,
            "failed to roll back migration outputs: {error}"
        );
    }
}

/// Idempotently materialize the journaled source bundle into the original
/// Running+Allow authority run.
///
/// Existing paths are reopened through `ScienceStore` and byte-verified;
/// missing paths are copied. This is used both by the first Finish and by a
/// retry after a stop during target copying.
fn ensure_migration_target_artifacts(
    store: &xai_grok_science::ScienceStore,
    ticket: &xai_grok_science::csv::ScienceRunTicket,
    bundle: &xai_grok_science::project::VerifiedMigrationBundle,
) -> xai_grok_science::Result<Vec<std::path::PathBuf>> {
    use sha2::Digest as _;
    use xai_grok_science::ScienceError;

    let existing = store.artifacts(&ticket.run_id)?;
    for registered in &existing {
        if registered.relative_path == std::path::Path::new("migration.json") {
            continue;
        }
        let Some((expected, _)) = bundle
            .artifacts()
            .find(|(artifact, _)| artifact.target_relative_path == registered.relative_path)
        else {
            return Err(ScienceError::Invalid(
                "migration authority contains an unexpected target artifact".into(),
            ));
        };
        if registered.call_id != ticket.call_id
            || registered.sha256 != expected.sha256
            || registered.bytes != expected.bytes
            || registered.mime != expected.mime
            || registered.preview != expected.preview
        {
            return Err(ScienceError::Invalid(
                "migration authority artifact registry differs from its journal".into(),
            ));
        }
    }

    let mut paths = Vec::with_capacity(bundle.artifact_records().len());
    for (artifact, bytes) in bundle.artifacts() {
        paths.push(artifact.target_relative_path.clone());
        let copied = store.put_artifact(
            &ticket.project_id,
            &ticket.run_id,
            &ticket.owner_id,
            ticket.call_id.clone(),
            &artifact.target_relative_path,
            bytes,
            artifact.mime.clone(),
            artifact.preview.clone(),
        )?;
        if copied.run_id != ticket.run_id
            || copied.call_id != ticket.call_id
            || copied.relative_path != artifact.target_relative_path
            || copied.sha256 != artifact.sha256
            || copied.bytes != artifact.bytes
            || copied.mime != artifact.mime
            || copied.preview != artifact.preview
        {
            return Err(ScienceError::Invalid(
                "migration copy changed artifact registry metadata".into(),
            ));
        }
        let reopened = store.allowed_running_artifact_bytes(
            &ticket.project_id,
            &ticket.run_id,
            &ticket.owner_id,
            &ticket.call_id,
            &artifact.target_relative_path,
        )?;
        if reopened.as_slice() != bytes
            || format!("{:x}", sha2::Sha256::digest(&reopened)) != artifact.sha256
        {
            return Err(ScienceError::Invalid(
                "migration authority target bytes differ from their journal".into(),
            ));
        }
    }
    Ok(paths)
}

fn migration_apply_error_may_have_committed(
    project_store: &xai_grok_science::project::ProjectStore,
    request: &xai_grok_science::project::MutationRequest,
) -> bool {
    if !matches!(
        request.mutation,
        xai_grok_science::project::ProjectMutation::ProjectMigrate { .. }
    ) {
        return false;
    }
    // Once the journal exists the target may be partially or fully published.
    // Failing the authority run would make the only safe recovery capability
    // terminal, so preserve Running and let an exact retry resume it.
    match project_store.lookup_migration_commit(&request.operation_id) {
        Ok(Some(commit)) => {
            commit.verify().is_ok()
                && commit.request_sha256 == request.replay_fingerprint().unwrap_or_default()
        }
        Ok(None) => false,
        Err(_) => true,
    }
}

fn verify_migration_replay(
    store: &xai_grok_science::ScienceStore,
    project_store: &xai_grok_science::project::ProjectStore,
    request: &xai_grok_science::project::MutationRequest,
    outcome: &xai_grok_science::project::MutationOutcome,
) -> xai_grok_science::Result<()> {
    use xai_grok_science::{ApprovalDecision, RunId, RunState, ScienceError};

    let result: xai_grok_science::project::MigrationResult =
        serde_json::from_value(outcome.result.clone())?;
    let commit = project_store
        .lookup_migration_commit(&request.operation_id)?
        .ok_or_else(|| {
            xai_grok_science::ScienceError::Invalid(format!(
                "migration commit {} is missing",
                request.operation_id
            ))
        })?;
    commit.verify()?;
    if commit.request_sha256 != request.replay_fingerprint()?
        || xai_grok_science::project::MigrationResult::from_commit(&commit)? != result
    {
        return Err(xai_grok_science::ScienceError::Invalid(
            "migration recovery outcome differs from its durable commit".into(),
        ));
    }
    if outcome.project_id != result.target_project_id
        || outcome.project_id != commit.manifest.target_project_id
    {
        return Err(ScienceError::Ownership);
    }
    let authority_run_id = RunId::new(&result.authority_run_id);
    let authority = store.recover_interrupted(&authority_run_id)?;
    if authority.state != RunState::Succeeded
        || authority.context.run_id != authority_run_id
        || authority.context.project_id.0 != result.target_project_id.0
        || authority.context.owner_id != request.owner_id
        || authority.context.session_id != request.session_id
        || authority.context.workspace_root.as_path() != commit.admission.workspace_root()
        || authority.context.artifact_root.as_path() != commit.admission.artifact_root()
        || authority
            .context
            .environment
            .get("project_migration_admission_sha256")
            != Some(&result.admission_sha256)
    {
        return Err(ScienceError::Ownership);
    }
    let approvals = store.approvals(&authority_run_id)?;
    let [approval] = approvals.as_slice() else {
        return Err(ScienceError::Invalid(
            "migration replay requires exactly one authority approval".into(),
        ));
    };
    if approval.project_id.0 != result.target_project_id.0
        || approval.run_id != authority_run_id
        || approval.owner_id != request.owner_id
        || approval.call_id.0 != "science_project_mutation"
        || approval.decision != ApprovalDecision::Allow
        || approval.decided_at.is_none()
    {
        return Err(ScienceError::Invalid(
            "migration replay is not backed by the original durable Allow".into(),
        ));
    }
    let ticket = xai_grok_science::csv::ScienceRunTicket {
        project_id: authority.context.project_id.clone(),
        run_id: authority_run_id.clone(),
        owner_id: authority.context.owner_id.clone(),
        call_id: xai_grok_science::CallId::new("science_project_mutation"),
    };
    validate_migration_success_events(store, &ticket, &authority.context, request, outcome)?;
    if !store.previews(&authority_run_id)?.is_empty() {
        return Err(ScienceError::Invalid(
            "migration replay authority contains unexpected previews".into(),
        ));
    }

    let artifacts = store.artifacts(&authority_run_id)?;
    if artifacts.len() != commit.manifest.artifacts.len() + 1 {
        return Err(ScienceError::Invalid(
            "migration replay artifact registry has unexpected entries".into(),
        ));
    }
    for artifact in &commit.manifest.artifacts {
        let registered = artifacts
            .iter()
            .find(|registered| registered.relative_path == artifact.target_relative_path)
            .ok_or_else(|| {
                ScienceError::Invalid(
                    "migration replay target artifact is missing from its registry".into(),
                )
            })?;
        if registered.run_id != authority_run_id
            || registered.call_id.0 != "science_project_mutation"
            || registered.sha256 != artifact.sha256
            || registered.bytes != artifact.bytes
            || registered.mime != artifact.mime
            || registered.preview != artifact.preview
        {
            return Err(ScienceError::Invalid(
                "migration replay target artifact metadata differs from its manifest".into(),
            ));
        }
        let bytes = store.artifact_bytes(
            &authority.context.project_id,
            &authority_run_id,
            &request.owner_id,
            &artifact.target_relative_path,
        )?;
        if bytes.len() as u64 != artifact.bytes
            || format!("{:x}", sha2::Sha256::digest(&bytes)) != artifact.sha256
        {
            return Err(ScienceError::Invalid(
                "migration replay target artifact failed byte verification".into(),
            ));
        }
    }
    let manifest_bytes = store.artifact_bytes(
        &authority.context.project_id,
        &authority_run_id,
        &request.owner_id,
        std::path::Path::new("migration.json"),
    )?;
    if manifest_bytes != serde_json::to_vec(&commit.manifest)?
        || format!("{:x}", sha2::Sha256::digest(&manifest_bytes)) != result.manifest_sha256
    {
        return Err(ScienceError::Invalid(
            "migration replay manifest artifact failed byte verification".into(),
        ));
    }
    let manifest_record = artifacts
        .iter()
        .find(|artifact| artifact.relative_path == std::path::Path::new("migration.json"))
        .ok_or_else(|| {
            ScienceError::Invalid(
                "migration replay manifest is missing from its artifact registry".into(),
            )
        })?;
    if manifest_record.run_id != authority_run_id
        || manifest_record.call_id.0 != "science_project_mutation"
        || manifest_record.sha256 != result.manifest_sha256
        || manifest_record.bytes != manifest_bytes.len() as u64
        || manifest_record.mime != "application/json"
        || manifest_record.preview
            != format!(
                "Verified migration {} → {}",
                result.source_run_id, result.target_project_id.0
            )
    {
        return Err(ScienceError::Invalid(
            "migration replay manifest registry metadata differs from its commit".into(),
        ));
    }
    let evidence = store.evidence(&authority_run_id)?;
    let provenance = store.provenance(&authority_run_id)?;
    let expected_evidence =
        expected_migration_evidence(&authority_run_id, outcome, &result, &commit.manifest);
    let expected_provenance = expected_migration_provenance(
        &authority_run_id,
        outcome,
        &result,
        &commit.admission,
        &commit.manifest,
    )?;
    if evidence != expected_evidence || provenance != expected_provenance {
        return Err(ScienceError::Invalid(
            "migration replay evidence or provenance differs from its durable manifest".into(),
        ));
    }
    Ok(())
}

fn recover_migration_authority_if_needed(
    store: &xai_grok_science::ScienceStore,
    project_store: &xai_grok_science::project::ProjectStore,
    request: &xai_grok_science::project::MutationRequest,
    outcome: &xai_grok_science::project::MutationOutcome,
) -> xai_grok_science::Result<xai_grok_science::project::MutationOutcome> {
    use xai_grok_science::RunState;

    let result: xai_grok_science::project::MigrationResult =
        serde_json::from_value(outcome.result.clone())?;
    // Recovery starts from the journal, not from the published project.
    // `verify_migration_result` requires the project commit marker, graph,
    // registry and manifest to be complete, which is exactly what can be
    // missing after a stop between journal admission and project publication.
    // Verify the immutable journal/request/result tuple first; the actor-owned
    // recovery path republishes missing project records, then the full replay
    // verifier below proves the completed state before returning.
    let commit = project_store
        .lookup_migration_commit(&request.operation_id)?
        .ok_or_else(|| {
            xai_grok_science::ScienceError::Invalid(format!(
                "migration commit {} is missing",
                request.operation_id
            ))
        })?;
    commit.verify()?;
    if commit.request_sha256 != request.replay_fingerprint()?
        || xai_grok_science::project::MigrationResult::from_commit(&commit)? != result
    {
        return Err(xai_grok_science::ScienceError::Invalid(
            "migration recovery outcome differs from its durable journal".into(),
        ));
    }
    let grant = xai_grok_science::project::MigrationRecoveryGrant::verify(store, &commit)?;
    let ticket = xai_grok_science::csv::ScienceRunTicket {
        project_id: xai_grok_science::ProjectId::new(result.target_project_id.0.clone()),
        run_id: grant.authority_run_id().clone(),
        owner_id: request.owner_id.clone(),
        call_id: xai_grok_science::CallId::new("science_project_mutation"),
    };
    let recovered = project_store.recover_actor_migration_operation(request, &grant)?;
    if recovered.project_id != outcome.project_id || recovered.result != outcome.result {
        return Err(xai_grok_science::ScienceError::Invalid(
            "recovered migration operation differs from its durable commit".into(),
        ));
    }
    if grant.authority_state() == RunState::Running {
        persist_migration_mutation_evidence(store, project_store, &ticket, request, &recovered)?;
        append_project_mutation_applied_once(store, &ticket.run_id, &recovered)?;
        let authority = store.load_run(&ticket.run_id)?;
        commit_migration_authority_success(
            store,
            project_store,
            &ticket,
            &authority.context,
            request,
            &recovered,
        )?;
    }
    verify_migration_replay(store, project_store, request, &recovered)?;
    Ok(recovered)
}

fn persist_migration_mutation_evidence(
    store: &xai_grok_science::ScienceStore,
    project_store: &xai_grok_science::project::ProjectStore,
    ticket: &xai_grok_science::csv::ScienceRunTicket,
    request: &xai_grok_science::project::MutationRequest,
    outcome: &xai_grok_science::project::MutationOutcome,
) -> xai_grok_science::Result<()> {
    use xai_grok_science::ScienceError;

    if outcome.kind != "project_migrate" {
        return Err(ScienceError::Invalid(
            "migration evidence received another mutation kind".into(),
        ));
    }
    let result: xai_grok_science::project::MigrationResult =
        serde_json::from_value(outcome.result.clone())?;
    if result.target_project_id != outcome.project_id
        || result.target_project_id.0 != ticket.project_id.0
        || result.authority_run_id != ticket.run_id.0
        || result.source_run_id.is_empty()
    {
        return Err(ScienceError::Ownership);
    }
    let commit = project_store.verify_migration_result(request, &result)?;
    let admission = commit.admission;
    let manifest = commit.manifest;
    manifest.verify_against_admission(&admission)?;
    if manifest.sha256()? != result.manifest_sha256 {
        return Err(ScienceError::Invalid(
            "migration result is not bound to the project manifest".into(),
        ));
    }

    // Reopen every copied target-owned payload while the actor run is still
    // Running+Allow. This proves the project registry will not point back to
    // inaccessible source-project bytes.
    for artifact in &manifest.artifacts {
        let copied = store.allowed_running_artifact_bytes(
            &ticket.project_id,
            &ticket.run_id,
            &ticket.owner_id,
            &ticket.call_id,
            &artifact.target_relative_path,
        )?;
        if copied.len() as u64 != artifact.bytes
            || format!("{:x}", sha2::Sha256::digest(&copied)) != artifact.sha256
        {
            return Err(ScienceError::Invalid(
                "target migration artifact differs from its verified source".into(),
            ));
        }
    }

    let manifest_path = std::path::Path::new("migration.json");
    let manifest_bytes = serde_json::to_vec(&manifest)?;
    let manifest_preview = format!(
        "Verified migration {} → {}",
        result.source_run_id, result.target_project_id.0
    );
    // Always enter the store-owned idempotent write protocol. Besides the
    // ordinary first write, this reconciles both crash-visible states:
    // registry-visible/payload-missing and payload-visible/registry-missing.
    // Pre-reading the registry and then opening the payload would strand the
    // first state instead of repairing it.
    let manifest_artifact = store.put_artifact(
        &ticket.project_id,
        &ticket.run_id,
        &ticket.owner_id,
        ticket.call_id.clone(),
        manifest_path,
        &manifest_bytes,
        "application/json",
        manifest_preview.clone(),
    )?;
    if manifest_artifact.run_id != ticket.run_id
        || manifest_artifact.call_id != ticket.call_id
        || manifest_artifact.relative_path != manifest_path
        || manifest_artifact.sha256 != result.manifest_sha256
        || manifest_artifact.bytes != manifest_bytes.len() as u64
        || manifest_artifact.mime != "application/json"
        || manifest_artifact.preview != manifest_preview
    {
        return Err(ScienceError::Invalid(
            "stored migration manifest registry metadata does not match its commit".into(),
        ));
    }
    let reopened_manifest = store.allowed_running_artifact_bytes(
        &ticket.project_id,
        &ticket.run_id,
        &ticket.owner_id,
        &ticket.call_id,
        manifest_path,
    )?;
    if reopened_manifest != manifest_bytes
        || format!("{:x}", sha2::Sha256::digest(&reopened_manifest)) != result.manifest_sha256
    {
        return Err(ScienceError::Invalid(
            "stored migration manifest bytes do not match its commit".into(),
        ));
    }

    let expected_evidence =
        expected_migration_evidence(&ticket.run_id, outcome, &result, &manifest);
    let expected_provenance =
        expected_migration_provenance(&ticket.run_id, outcome, &result, &admission, &manifest)?;

    append_exact_registry_suffix(
        store.evidence(&ticket.run_id)?,
        &expected_evidence,
        |item| store.add_evidence(item),
        "migration evidence",
    )?;
    append_exact_registry_suffix(
        store.provenance(&ticket.run_id)?,
        &expected_provenance,
        |item| store.add_provenance(item),
        "migration provenance",
    )?;
    Ok(())
}

fn expected_migration_evidence(
    authority_run_id: &xai_grok_science::RunId,
    outcome: &xai_grok_science::project::MutationOutcome,
    result: &xai_grok_science::project::MigrationResult,
    manifest: &xai_grok_science::project::MigrationManifest,
) -> Vec<xai_grok_science::Evidence> {
    let mut expected = manifest
        .evidence
        .iter()
        .map(|item| xai_grok_science::Evidence {
            run_id: authority_run_id.clone(),
            claim: item.claim.clone(),
            source: format!(
                "migrated:{}:{}",
                manifest.source_run.context.run_id.0, item.source
            ),
            artifact_sha256: item.artifact_sha256.clone(),
            verified_at: item.verified_at,
        })
        .collect::<Vec<_>>();
    expected.push(xai_grok_science::Evidence {
        run_id: authority_run_id.clone(),
        claim: format!(
            "Migration {} preserved {} byte-verified artifact(s), {} evidence record(s), and {} provenance record(s).",
            outcome.operation_id,
            result.artifacts_migrated,
            result.evidence_items_migrated,
            result.provenance_items_migrated,
        ),
        source: format!(
            "lumen-science://project/{}/migration.json",
            outcome.project_id.0
        ),
        artifact_sha256: Some(result.manifest_sha256.clone()),
        verified_at: manifest.generated_at,
    });
    expected
}

fn expected_migration_provenance(
    authority_run_id: &xai_grok_science::RunId,
    outcome: &xai_grok_science::project::MutationOutcome,
    result: &xai_grok_science::project::MigrationResult,
    admission: &xai_grok_science::project::MigrationAdmission,
    manifest: &xai_grok_science::project::MigrationManifest,
) -> xai_grok_science::Result<Vec<xai_grok_science::Provenance>> {
    let admission_sha256 = admission.sha256()?;
    let mut expected = manifest
        .provenance
        .iter()
        .map(|item| {
            let mut environment = item.environment.clone();
            environment.insert(
                "migration_source_run_id".into(),
                manifest.source_run.context.run_id.0.clone(),
            );
            environment.insert(
                "migration_admission_sha256".into(),
                admission_sha256.clone(),
            );
            xai_grok_science::Provenance {
                run_id: authority_run_id.clone(),
                source_uri: item.source_uri.clone(),
                source_commit: item.source_commit.clone(),
                source_path: item.source_path.clone(),
                license: item.license.clone(),
                retrieved_at: item.retrieved_at,
                input_sha256: item.input_sha256.clone(),
                tool: "lumen-science/project-migrate".into(),
                environment,
            }
        })
        .collect::<Vec<_>>();
    expected.push(xai_grok_science::Provenance {
        run_id: authority_run_id.clone(),
        source_uri: format!(
            "lumen-science://project/{}/migration.json",
            outcome.project_id.0
        ),
        source_commit: Some(result.manifest_sha256.clone()),
        source_path: Some("migration.json".into()),
        license: "Lumen-Science-Derived-Evidence".into(),
        retrieved_at: manifest.generated_at,
        input_sha256: result.manifest_sha256.clone(),
        tool: "lumen-science/project-migrate".into(),
        environment: std::collections::BTreeMap::from([
            ("operation_id".into(), outcome.operation_id.clone()),
            ("admission_sha256".into(), admission_sha256),
            ("network".into(), "disabled".into()),
        ]),
    });
    Ok(expected)
}

fn append_exact_registry_suffix<T: Clone + PartialEq>(
    existing: Vec<T>,
    expected: &[T],
    mut append: impl FnMut(T) -> xai_grok_science::Result<()>,
    kind: &str,
) -> xai_grok_science::Result<()> {
    if existing.len() > expected.len() || existing != expected[..existing.len()] {
        return Err(xai_grok_science::ScienceError::Invalid(format!(
            "{kind} registry conflicts with its recovery plan"
        )));
    }
    for item in &expected[existing.len()..] {
        append(item.clone())?;
    }
    Ok(())
}

/// A review is itself evidence. Alongside the project review ledger, keep an
/// immutable manifest artifact plus Evidence/Provenance rows in the actor's
/// approval run. Host verification can therefore re-open the exact record
/// bytes and correlate them to the operation id and source-artifact
/// fingerprint. Other project mutations remain record-only.
fn persist_review_mutation_evidence(
    store: &xai_grok_science::ScienceStore,
    ticket: &xai_grok_science::csv::ScienceRunTicket,
    outcome: &xai_grok_science::project::MutationOutcome,
) -> xai_grok_science::Result<Option<xai_grok_science::project::ReviewRecord>> {
    if outcome.kind != "review_record" {
        return Ok(None);
    }
    let review: xai_grok_science::project::ReviewRecord =
        serde_json::from_value(outcome.result.clone())?;
    if review.operation_id != outcome.operation_id
        || review.project_id != outcome.project_id
        || review.project_id.0 != ticket.project_id.0
        || review.owner_id != ticket.owner_id
        || review.authority_run_id != ticket.run_id.0
    {
        return Err(xai_grok_science::ScienceError::Ownership);
    }
    if review.evidence_fingerprint.len() != 64
        || !review
            .evidence_fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(xai_grok_science::ScienceError::Invalid(
            "durable review record has a malformed evidence fingerprint".into(),
        ));
    }
    let manifest = serde_json::to_vec_pretty(&review)?;
    let existing_artifacts = store.artifacts(&ticket.run_id)?;
    let artifact = match existing_artifacts.as_slice() {
        [] => store.put_artifact(
            &ticket.project_id,
            &ticket.run_id,
            &ticket.owner_id,
            ticket.call_id.clone(),
            std::path::Path::new("review_record.json"),
            &manifest,
            "application/json",
            "actor-owned durable review record",
        )?,
        [artifact]
            if artifact.call_id == ticket.call_id
                && artifact.relative_path == std::path::Path::new("review_record.json")
                && artifact.sha256 == format!("{:x}", sha2::Sha256::digest(&manifest))
                && artifact.bytes == manifest.len() as u64
                && artifact.mime == "application/json"
                && artifact.preview == "actor-owned durable review record" =>
        {
            // The crate-owned preterminal verifier reopens and rehashes these
            // bytes after the exact evidence/provenance suffix is complete.
            // Do not expose a generic Running-artifact read API to the shell.
            artifact.clone()
        }
        _ => {
            return Err(xai_grok_science::ScienceError::Invalid(
                "review authority run contains a conflicting manifest set".into(),
            ));
        }
    };

    let expected_evidence = review.expected_evidence(artifact.sha256.clone());
    match store.evidence(&ticket.run_id)?.as_slice() {
        [] => store.add_evidence(expected_evidence)?,
        [existing] if existing == &expected_evidence => {}
        _ => {
            return Err(xai_grok_science::ScienceError::Invalid(
                "review authority run contains conflicting evidence".into(),
            ));
        }
    }
    let expected_provenance = review.expected_provenance();
    match store.provenance(&ticket.run_id)?.as_slice() {
        [] => store.add_provenance(expected_provenance)?,
        [existing] if existing == &expected_provenance => {}
        _ => {
            return Err(xai_grok_science::ScienceError::Invalid(
                "review authority run contains conflicting provenance".into(),
            ));
        }
    }
    Ok(Some(review))
}

fn validate_review_replay_request(
    request: &xai_grok_science::project::MutationRequest,
    record: &xai_grok_science::project::ReviewRecord,
) -> xai_grok_science::Result<()> {
    let xai_grok_science::project::ProjectMutation::ReviewRecord {
        project_id,
        reviewer_id,
        verdict,
        summary,
        claim_id,
        source_run_id,
        authority_run_id,
        artifact_sha256s,
    } = &request.mutation
    else {
        return Err(xai_grok_science::ScienceError::Invalid(
            "review operation replayed with a non-review mutation".into(),
        ));
    };
    let mut requested = artifact_sha256s.clone();
    requested.sort();
    let mut recorded: Vec<_> = record
        .artifacts
        .iter()
        .map(|artifact| artifact.sha256.clone())
        .collect();
    recorded.sort();
    let expected_authority = format!("review-authority-{}", request.replay_fingerprint()?);
    if request.replay_fingerprint()? != record.review_request_sha256
        || request.operation_id != record.operation_id
        || request.session_id != record.session_id
        || request.owner_id != record.owner_id
        || project_id != &record.project_id
        || reviewer_id != &record.reviewer_id
        || verdict != &record.verdict
        || summary != &record.summary
        || claim_id != &record.claim_id
        || source_run_id != &record.source_run_id
        || authority_run_id != &expected_authority
        || record.authority_run_id != expected_authority
        || requested != recorded
    {
        return Err(xai_grok_science::ScienceError::Invalid(
            "review operation replay does not match its original request".into(),
        ));
    }
    Ok(())
}

fn project_mutation_applied_payload(
    outcome: &xai_grok_science::project::MutationOutcome,
) -> serde_json::Value {
    serde_json::json!({
        "operation_id": outcome.operation_id,
        "kind": outcome.kind,
        "project_id": outcome.project_id.0,
        "revision": outcome.revision,
    })
}

fn append_project_mutation_applied_once(
    store: &xai_grok_science::ScienceStore,
    run_id: &xai_grok_science::RunId,
    outcome: &xai_grok_science::project::MutationOutcome,
) -> xai_grok_science::Result<()> {
    let payload = project_mutation_applied_payload(outcome);
    let events = store.events_after(run_id, 0, 1_000)?;
    if events
        .iter()
        .any(|event| event.kind == "project.mutation.applied" && event.payload == payload)
    {
        return Ok(());
    }
    if events.iter().any(|event| {
        event.kind == "project.mutation.applied"
            && event.payload["operation_id"] == outcome.operation_id
    }) {
        return Err(xai_grok_science::ScienceError::Invalid(
            "project mutation applied event conflicts with its operation record".into(),
        ));
    }
    store.append_recoverable_commit_event(
        run_id,
        "SessionActor",
        "project.mutation.applied",
        payload,
    )?;
    Ok(())
}

fn validate_review_success_events(
    store: &xai_grok_science::ScienceStore,
    ticket: &xai_grok_science::csv::ScienceRunTicket,
    context: &xai_grok_science::RunContext,
    operation_id: &str,
    outcome: &xai_grok_science::project::MutationOutcome,
) -> xai_grok_science::Result<Vec<xai_grok_science::Event>> {
    let events = project_mutation_events(store, &ticket.run_id)?;
    let [begin, allowed, applied] = events.as_slice() else {
        return Err(xai_grok_science::ScienceError::Invalid(
            "successful review requires exactly run.created, approval.allowed, and project.mutation.applied"
                .into(),
        ));
    };
    if begin.actor != "SessionActor"
        || begin.kind != "run.created"
        || begin.payload
            != project_mutation_begin_event_payload(context, "review_record", operation_id)
        || allowed.actor != "LumenApproval"
        || allowed.kind != "approval.allowed"
        || allowed.payload != serde_json::json!({"call_id": ticket.call_id.0})
        || applied.actor != "SessionActor"
        || applied.kind != "project.mutation.applied"
        || applied.payload != project_mutation_applied_payload(outcome)
    {
        return Err(xai_grok_science::ScienceError::Invalid(
            "successful review authority event protocol is not canonical".into(),
        ));
    }
    Ok(events)
}

fn validate_migration_success_events(
    store: &xai_grok_science::ScienceStore,
    ticket: &xai_grok_science::csv::ScienceRunTicket,
    context: &xai_grok_science::RunContext,
    request: &xai_grok_science::project::MutationRequest,
    outcome: &xai_grok_science::project::MutationOutcome,
) -> xai_grok_science::Result<Vec<xai_grok_science::Event>> {
    if !matches!(
        request.mutation,
        xai_grok_science::project::ProjectMutation::ProjectMigrate { .. }
    ) || outcome.kind != "project_migrate"
        || outcome.operation_id != request.operation_id
    {
        return Err(xai_grok_science::ScienceError::Invalid(
            "migration success validation received another mutation kind".into(),
        ));
    }
    let events = project_mutation_events(store, &ticket.run_id)?;
    let [begin, allowed, applied] = events.as_slice() else {
        return Err(xai_grok_science::ScienceError::Invalid(
            "successful migration requires exactly run.created, approval.allowed, and project.mutation.applied"
                .into(),
        ));
    };
    if begin.run_id != ticket.run_id
        || begin.actor != "SessionActor"
        || begin.kind != "run.created"
        || begin.payload
            != project_mutation_begin_event_payload(
                context,
                "project_migrate",
                &request.operation_id,
            )
        || allowed.run_id != ticket.run_id
        || allowed.actor != "LumenApproval"
        || allowed.kind != "approval.allowed"
        || allowed.payload != serde_json::json!({"call_id": ticket.call_id.0})
        || applied.run_id != ticket.run_id
        || applied.actor != "SessionActor"
        || applied.kind != "project.mutation.applied"
        || applied.payload != project_mutation_applied_payload(outcome)
    {
        return Err(xai_grok_science::ScienceError::Invalid(
            "successful migration authority event protocol is not canonical".into(),
        ));
    }
    Ok(events)
}

fn commit_migration_authority_success(
    store: &xai_grok_science::ScienceStore,
    project_store: &xai_grok_science::project::ProjectStore,
    ticket: &xai_grok_science::csv::ScienceRunTicket,
    expected_context: &xai_grok_science::RunContext,
    request: &xai_grok_science::project::MutationRequest,
    outcome: &xai_grok_science::project::MutationOutcome,
) -> xai_grok_science::Result<()> {
    use xai_grok_science::{RunState, ScienceError};

    let result: xai_grok_science::project::MigrationResult =
        serde_json::from_value(outcome.result.clone())?;
    if outcome.kind != "project_migrate"
        || outcome.operation_id != request.operation_id
        || outcome.project_id != result.target_project_id
        || result.authority_run_id != ticket.run_id.0
        || ticket.project_id.0 != result.target_project_id.0
        || ticket.owner_id != request.owner_id
        || expected_context.run_id != ticket.run_id
        || expected_context.project_id != ticket.project_id
        || expected_context.owner_id != ticket.owner_id
        || expected_context.session_id != request.session_id
        || expected_context.artifact_root != store.root().join("runs")
        || !store.shares_root_capability_with(project_store)?
    {
        return Err(ScienceError::Ownership);
    }
    if expected_context
        .environment
        .get("project_migration_admission_sha256")
        != Some(&result.admission_sha256)
    {
        return Err(ScienceError::Invalid(
            "migration authority context differs from its durable admission".into(),
        ));
    }

    let run = store.load_run(&ticket.run_id)?;
    if run.context != *expected_context || run.state != RunState::Running {
        return Err(ScienceError::Invalid(
            "migration completion requires its exact Running authority".into(),
        ));
    }
    let events =
        validate_migration_success_events(store, ticket, expected_context, request, outcome)?;
    let artifacts = store.artifacts(&ticket.run_id)?;
    let evidence = store.evidence(&ticket.run_id)?;
    let provenance = store.provenance(&ticket.run_id)?;
    let previews = store.previews(&ticket.run_id)?;
    if previews.len() > 0 {
        return Err(ScienceError::Invalid(
            "migration authority contains unexpected previews".into(),
        ));
    }
    let final_event = events.last().cloned().ok_or_else(|| {
        ScienceError::Invalid("migration final event disappeared before commit".into())
    })?;
    store.transition_succeeded_with_manifest(&xai_grok_science::SuccessfulCompletionManifest {
        context: expected_context.clone(),
        artifacts,
        evidence,
        provenance,
        previews,
        events,
        final_event,
    })?;
    Ok(())
}

fn commit_review_authority_success_inner(
    store: &xai_grok_science::ScienceStore,
    project_store: &xai_grok_science::project::ProjectStore,
    ticket: &xai_grok_science::csv::ScienceRunTicket,
    expected_context: &xai_grok_science::RunContext,
    outcome: &xai_grok_science::project::MutationOutcome,
) -> xai_grok_science::Result<()> {
    use xai_grok_science::{RunState, ScienceError};

    let review: xai_grok_science::project::ReviewRecord =
        serde_json::from_value(outcome.result.clone())?;
    if outcome.kind != "review_record"
        || outcome.operation_id != review.operation_id
        || outcome.project_id != review.project_id
        || ticket.run_id.0 != review.authority_run_id
        || ticket.project_id.0 != review.project_id.0
        || ticket.owner_id != review.owner_id
        || expected_context.run_id != ticket.run_id
        || expected_context.project_id != ticket.project_id
        || expected_context.owner_id != ticket.owner_id
        || expected_context.session_id != review.session_id
        || expected_context.artifact_root != store.root().join("runs")
        || !store.shares_root_capability_with(project_store)?
    {
        return Err(ScienceError::Ownership);
    }
    if expected_context
        .environment
        .get(xai_grok_science::project::ReviewAdmission::ENV_ADMISSION_SHA256)
        != Some(&review.review_admission_sha256)
        || expected_context
            .environment
            .get(xai_grok_science::project::ReviewAdmission::ENV_REQUEST_SHA256)
            != Some(&review.review_request_sha256)
        || expected_context
            .environment
            .get(xai_grok_science::project::ReviewAdmission::ENV_SOURCE_AUTHORITY_SHA256)
            != Some(&review.source_authority_sha256)
        || expected_context
            .environment
            .get(xai_grok_science::project::ReviewAdmission::ENV_PROJECT_REVISION)
            != Some(&review.review_project_revision)
    {
        return Err(ScienceError::Invalid(
            "review authority context differs from its durable admission".into(),
        ));
    }

    let run = store.load_run(&ticket.run_id)?;
    let run = if run.state == RunState::Succeeded {
        store.recover_interrupted(&ticket.run_id)?
    } else {
        run
    };
    if run.context != *expected_context {
        return Err(ScienceError::Ownership);
    }
    match run.state {
        RunState::Running => {
            persist_review_mutation_evidence(store, ticket, outcome)?;
            project_store.verify_pending_review_commit_with_store(store, &review)?;
            append_project_mutation_applied_once(store, &ticket.run_id, outcome)?;
            let events = validate_review_success_events(
                store,
                ticket,
                expected_context,
                &outcome.operation_id,
                outcome,
            )?;
            let artifacts = store.artifacts(&ticket.run_id)?;
            let evidence = store.evidence(&ticket.run_id)?;
            let provenance = store.provenance(&ticket.run_id)?;
            let previews = store.previews(&ticket.run_id)?;
            if !previews.is_empty() {
                return Err(ScienceError::Invalid(
                    "review authority contains unexpected previews".into(),
                ));
            }
            let final_event = events.last().cloned().ok_or_else(|| {
                ScienceError::Invalid("review final event disappeared before commit".into())
            })?;
            store.transition_succeeded_with_manifest(
                &xai_grok_science::SuccessfulCompletionManifest {
                    context: expected_context.clone(),
                    artifacts,
                    evidence,
                    provenance,
                    previews,
                    events,
                    final_event,
                },
            )?;
        }
        RunState::Succeeded => {
            let events = validate_review_success_events(
                store,
                ticket,
                expected_context,
                &outcome.operation_id,
                outcome,
            )?;
            if !store.previews(&ticket.run_id)?.is_empty()
                || events.last().is_none_or(|event| {
                    event.actor != "SessionActor" || event.kind != "project.mutation.applied"
                })
            {
                return Err(ScienceError::Invalid(
                    "terminal review authority differs from its exact completion manifest".into(),
                ));
            }
        }
        state => {
            return Err(ScienceError::Invalid(format!(
                "review commit requires Running or Succeeded authority, found {state:?}"
            )));
        }
    }
    project_store.verify_review_record_with_store(store, &review)?;
    Ok(())
}

fn commit_review_authority_success(
    store: &xai_grok_science::ScienceStore,
    project_store: &xai_grok_science::project::ProjectStore,
    ticket: &xai_grok_science::csv::ScienceRunTicket,
    expected_context: &xai_grok_science::RunContext,
    request: &xai_grok_science::project::MutationRequest,
    outcome: &xai_grok_science::project::MutationOutcome,
) -> xai_grok_science::Result<()> {
    let review: xai_grok_science::project::ReviewRecord =
        serde_json::from_value(outcome.result.clone())?;
    validate_review_replay_request(request, &review)?;
    if request.replay_fingerprint()? != review.review_request_sha256 {
        return Err(xai_grok_science::ScienceError::Invalid(
            "review result does not bind the admitted normalized request".into(),
        ));
    }
    commit_review_authority_success_inner(store, project_store, ticket, expected_context, outcome)
}

fn recover_interrupted_review_commit(
    store: &xai_grok_science::ScienceStore,
    project_store: &xai_grok_science::project::ProjectStore,
    operation: &xai_grok_science::project::OperationRecord,
    review: &xai_grok_science::project::ReviewRecord,
) -> xai_grok_science::Result<()> {
    let ticket = xai_grok_science::csv::ScienceRunTicket {
        project_id: xai_grok_science::ProjectId::new(review.project_id.0.clone()),
        run_id: xai_grok_science::RunId::new(&review.authority_run_id),
        owner_id: review.owner_id.clone(),
        call_id: xai_grok_science::CallId::new("science_project_mutation"),
    };
    let outcome = xai_grok_science::project::MutationOutcome {
        operation_id: operation.operation_id.clone(),
        kind: operation.kind.clone(),
        project_id: operation.project_id.clone(),
        revision: operation.revision.clone(),
        result: operation.result.clone(),
        replayed: false,
    };
    if operation.kind != "review_record"
        || operation.operation_id != review.operation_id
        || operation.project_id != review.project_id
        || operation.result != serde_json::to_value(review)?
        || operation.request_sha256 != review.review_request_sha256
    {
        return Err(xai_grok_science::ScienceError::Invalid(
            "review recovery operation differs from its immutable review ledger".into(),
        ));
    }
    let authority = store.load_run(&ticket.run_id)?;
    commit_review_authority_success_inner(
        store,
        project_store,
        &ticket,
        &authority.context,
        &outcome,
    )
}

fn recover_orphan_review_ledger(
    store: &xai_grok_science::ScienceStore,
    project_store: &xai_grok_science::project::ProjectStore,
    request: &xai_grok_science::project::MutationRequest,
    review: &xai_grok_science::project::ReviewRecord,
) -> xai_grok_science::Result<xai_grok_science::project::MutationOutcome> {
    validate_review_replay_request(request, review)?;
    project_store.verify_pending_review_record_with_store(store, review)?;
    let grant =
        xai_grok_science::project::ReviewRecoveryGrant::verify(project_store, store, request)?;
    let outcome = project_store.recover_actor_review_operation(request, &grant)?;
    let operation = project_store
        .lookup_operation(&request.operation_id)?
        .ok_or_else(|| {
            xai_grok_science::ScienceError::Invalid(
                "review orphan recovery did not create its operation record".into(),
            )
        })?;
    recover_interrupted_review_commit(store, project_store, &operation, review)?;
    Ok(outcome)
}

fn review_apply_error_may_have_committed(
    store: &xai_grok_science::ScienceStore,
    project_store: &xai_grok_science::project::ProjectStore,
    request: &xai_grok_science::project::MutationRequest,
) -> bool {
    let xai_grok_science::project::ProjectMutation::ReviewRecord { project_id, .. } =
        &request.mutation
    else {
        return false;
    };
    match project_store.lookup_review_record(project_id, &request.operation_id) {
        Ok(Some(review)) => {
            validate_review_replay_request(request, &review).is_ok()
                && project_store
                    .verify_pending_review_record_with_store(store, &review)
                    .is_ok()
        }
        Ok(None) => false,
        // An unreadable ledger path means the actor cannot prove that the
        // earlier atomic write did not happen. Preserve Running so a retry can
        // diagnose/recover instead of irreversibly poisoning the operation.
        Err(_) => true,
    }
}

#[cfg(test)]
mod actor_root_tests {
    use super::*;
    use std::collections::BTreeMap;

    fn context(
        workspace: &std::path::Path,
        project_root: &std::path::Path,
    ) -> xai_grok_science::RunContext {
        xai_grok_science::RunContext {
            run_id: xai_grok_science::RunId::new_v7(),
            project_id: xai_grok_science::ProjectId::new("pending-op-root-test"),
            session_id: "session-root-test".into(),
            owner_id: "owner-root-test".into(),
            workspace_root: workspace.to_path_buf(),
            provider: "offline-deterministic".into(),
            approval_policy: "production-session-permission".into(),
            tool_profile: "science-project-mutation-v1".into(),
            artifact_root: project_root.join("runs"),
            environment: BTreeMap::from([("network".into(), "disabled".into())]),
        }
    }

    #[test]
    fn project_mutation_roots_are_rechecked_at_the_actor_boundary() {
        let workspace = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let workspace = dunce::canonicalize(workspace.path()).unwrap();
        let outside = dunce::canonicalize(outside.path()).unwrap();
        let project_root = workspace.join("science-store");
        std::fs::create_dir_all(project_root.join("runs")).unwrap();
        let store = xai_grok_science::ScienceStore::new(&project_root);
        let good = context(&workspace, &project_root);
        validate_project_mutation_actor_roots(&workspace, &store, &project_root, &good).unwrap();

        let outside_root = outside.join("science-store");
        std::fs::create_dir_all(outside_root.join("runs")).unwrap();
        let outside_store = xai_grok_science::ScienceStore::new(&outside_root);
        assert!(
            validate_project_mutation_actor_roots(
                &workspace,
                &outside_store,
                &outside_root,
                &context(&workspace, &outside_root),
            )
            .is_err(),
            "actor accepted a project store outside its workspace"
        );

        let other_root = workspace.join("other-store");
        std::fs::create_dir_all(other_root.join("runs")).unwrap();
        let other_store = xai_grok_science::ScienceStore::new(&other_root);
        assert!(
            validate_project_mutation_actor_roots(&workspace, &other_store, &project_root, &good,)
                .is_err(),
            "actor accepted mismatched ScienceStore and ProjectStore roots"
        );

        let mut forged_workspace = good.clone();
        forged_workspace.workspace_root = outside.clone();
        assert!(
            validate_project_mutation_actor_roots(
                &workspace,
                &store,
                &project_root,
                &forged_workspace,
            )
            .is_err(),
            "actor accepted a forged RunContext workspace"
        );

        let mut forged_artifact_root = good;
        forged_artifact_root.artifact_root = other_root.join("runs");
        assert!(
            validate_project_mutation_actor_roots(
                &workspace,
                &store,
                &project_root,
                &forged_artifact_root,
            )
            .is_err(),
            "actor accepted an artifact root unrelated to the project store"
        );
        assert!(
            std::fs::read_dir(project_root.join("runs"))
                .unwrap()
                .next()
                .is_none(),
            "root validation wrote inside the durable run root"
        );
    }

    fn migration_protocol_fixture(
        label: &str,
    ) -> (
        tempfile::TempDir,
        xai_grok_science::ScienceStore,
        xai_grok_science::project::ProjectStore,
        xai_grok_science::RunContext,
        xai_grok_science::project::MutationRequest,
        xai_grok_science::csv::ScienceRunTicket,
    ) {
        let root = tempfile::tempdir().unwrap();
        let workspace = dunce::canonicalize(root.path()).unwrap();
        let store_root = workspace.join(format!("science-store-{label}"));
        std::fs::create_dir_all(&store_root).unwrap();
        let store = xai_grok_science::ScienceStore::new_confined(&store_root, &workspace).unwrap();
        let project_store =
            xai_grok_science::project::ProjectStore::new_confined(&store_root, &workspace).unwrap();
        let mut context = context(&workspace, &store_root);
        context.run_id = xai_grok_science::RunId::new(format!("migration-authority-{label}"));
        context.project_id = xai_grok_science::ProjectId::new(format!("migration-target-{label}"));
        context
            .environment
            .insert("project_migration_admission_sha256".into(), "a".repeat(64));
        let request = xai_grok_science::project::MutationRequest {
            operation_id: format!("op-migration-{label}"),
            session_id: context.session_id.clone(),
            owner_id: context.owner_id.clone(),
            expected_revision: None,
            mutation: xai_grok_science::project::ProjectMutation::ProjectMigrate {
                source_run_id: format!("source-{label}"),
                title: "Migration protocol fixture".into(),
                research_question: "Does recovery preserve the exact event protocol?".into(),
                authority_run_id: context.run_id.0.clone(),
            },
        };
        let ticket = project_mutation_ticket(&context);
        store.create_run(context.clone()).unwrap();
        (root, store, project_store, context, request, ticket)
    }

    #[test]
    fn migration_created_and_allowed_crash_windows_recover_exact_events_once() {
        let (_root, store, project_store, context, request, ticket) =
            migration_protocol_fixture("allow-recovery");
        ensure_created_migration_begin_event(&store, &project_store, &ticket, &context, &request)
            .unwrap();
        ensure_created_migration_begin_event(&store, &project_store, &ticket, &context, &request)
            .unwrap();
        assert_eq!(
            project_mutation_events(&store, &ticket.run_id)
                .unwrap()
                .len(),
            1
        );
        store
            .request_approval(xai_grok_science::Approval {
                project_id: ticket.project_id.clone(),
                run_id: ticket.run_id.clone(),
                call_id: ticket.call_id.clone(),
                owner_id: ticket.owner_id.clone(),
                decision: xai_grok_science::ApprovalDecision::Pending,
                decided_at: None,
            })
            .unwrap();
        store
            .transition(
                &ticket.run_id,
                xai_grok_science::RunState::AwaitingApproval,
                None,
            )
            .unwrap();
        store
            .decide_approval(
                &ticket.project_id,
                &ticket.run_id,
                &ticket.owner_id,
                &ticket.call_id,
                xai_grok_science::ApprovalDecision::Allow,
            )
            .unwrap();
        ensure_migration_allowed_event(&store, &ticket, &context, &request).unwrap();
        ensure_migration_allowed_event(&store, &ticket, &context, &request).unwrap();
        validate_migration_authority_event_prefix(&store, &ticket, &context, &request, true)
            .unwrap();
        store
            .transition(&ticket.run_id, xai_grok_science::RunState::Running, None)
            .unwrap();
        assert_eq!(
            project_mutation_events(&store, &ticket.run_id)
                .unwrap()
                .len(),
            2
        );

        store
            .append_recoverable_commit_event(
                &ticket.run_id,
                "Intruder",
                "migration.unknown",
                serde_json::json!({}),
            )
            .unwrap();
        assert!(
            validate_migration_authority_event_prefix(&store, &ticket, &context, &request, true,)
                .is_err(),
            "inserted migration event was accepted"
        );
    }

    #[test]
    fn migration_terminal_event_before_state_recovers_original_reason() {
        let (_root, store, project_store, context, request, ticket) =
            migration_protocol_fixture("terminal-recovery");
        ensure_created_migration_begin_event(&store, &project_store, &ticket, &context, &request)
            .unwrap();
        store
            .request_approval(xai_grok_science::Approval {
                project_id: ticket.project_id.clone(),
                run_id: ticket.run_id.clone(),
                call_id: ticket.call_id.clone(),
                owner_id: ticket.owner_id.clone(),
                decision: xai_grok_science::ApprovalDecision::Pending,
                decided_at: None,
            })
            .unwrap();
        store
            .transition(
                &ticket.run_id,
                xai_grok_science::RunState::AwaitingApproval,
                None,
            )
            .unwrap();
        store
            .decide_approval(
                &ticket.project_id,
                &ticket.run_id,
                &ticket.owner_id,
                &ticket.call_id,
                xai_grok_science::ApprovalDecision::Deny,
            )
            .unwrap();
        let original_reason = "operator denied the exact migration";
        store
            .append_recoverable_commit_event(
                &ticket.run_id,
                "LumenApproval",
                "approval.denied",
                serde_json::json!({
                    "call_id": ticket.call_id.0,
                    "reason": original_reason,
                }),
            )
            .unwrap();
        let state = recover_migration_terminal_decision(
            &store,
            &project_store,
            &ticket,
            &context,
            &request,
            xai_grok_science::ApprovalDecision::Deny,
        )
        .unwrap();
        assert_eq!(state, xai_grok_science::RunState::Denied);
        let run = store.load_run(&ticket.run_id).unwrap();
        assert_eq!(run.terminal_reason.as_deref(), Some(original_reason));
        assert!(store.artifacts(&ticket.run_id).unwrap().is_empty());
        assert!(store.evidence(&ticket.run_id).unwrap().is_empty());
        assert!(store.provenance(&ticket.run_id).unwrap().is_empty());
        assert!(
            project_store
                .lookup_operation(&request.operation_id)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn migration_replay_begin_finish_window_rejects_late_event() {
        let (_root, store, project_store, context, request, ticket) =
            migration_protocol_fixture("replay-finish-revalidation");
        ensure_created_migration_begin_event(&store, &project_store, &ticket, &context, &request)
            .unwrap();
        store
            .request_approval(xai_grok_science::Approval {
                project_id: ticket.project_id.clone(),
                run_id: ticket.run_id.clone(),
                call_id: ticket.call_id.clone(),
                owner_id: ticket.owner_id.clone(),
                decision: xai_grok_science::ApprovalDecision::Pending,
                decided_at: None,
            })
            .unwrap();
        store
            .transition(
                &ticket.run_id,
                xai_grok_science::RunState::AwaitingApproval,
                None,
            )
            .unwrap();
        store
            .decide_approval(
                &ticket.project_id,
                &ticket.run_id,
                &ticket.owner_id,
                &ticket.call_id,
                xai_grok_science::ApprovalDecision::Allow,
            )
            .unwrap();
        ensure_migration_allowed_event(&store, &ticket, &context, &request).unwrap();
        store
            .transition(&ticket.run_id, xai_grok_science::RunState::Running, None)
            .unwrap();
        let artifact = store
            .put_artifact(
                &ticket.project_id,
                &ticket.run_id,
                &ticket.owner_id,
                ticket.call_id.clone(),
                std::path::Path::new("migration.json"),
                br#"{"migration":"replay-finish-revalidation"}"#,
                "application/json",
                "migration",
            )
            .unwrap();
        store
            .add_evidence(xai_grok_science::Evidence {
                run_id: ticket.run_id.clone(),
                claim: "The migration replay fixture is store-owned.".into(),
                source: "fixture://migration-replay-finish".into(),
                artifact_sha256: Some(artifact.sha256.clone()),
                verified_at: chrono::Utc::now(),
            })
            .unwrap();
        store
            .add_provenance(xai_grok_science::Provenance {
                run_id: ticket.run_id.clone(),
                source_uri: "fixture://migration-replay-finish".into(),
                source_commit: None,
                source_path: Some("migration.json".into()),
                license: "test-only".into(),
                retrieved_at: chrono::Utc::now(),
                input_sha256: artifact.sha256,
                tool: "migration-replay-finish-fixture".into(),
                environment: std::collections::BTreeMap::new(),
            })
            .unwrap();

        let outcome = xai_grok_science::project::MutationOutcome {
            operation_id: request.operation_id.clone(),
            kind: "project_migrate".into(),
            project_id: xai_grok_science::project::ProjectId(ticket.project_id.0.clone()),
            revision: "migration-replay-finish-revision".into(),
            result: serde_json::json!({}),
            replayed: false,
        };
        store
            .append_recoverable_commit_event(
                &ticket.run_id,
                "SessionActor",
                "project.mutation.applied",
                project_mutation_applied_payload(&outcome),
            )
            .unwrap();
        let events =
            validate_migration_success_events(&store, &ticket, &context, &request, &outcome)
                .unwrap();
        let final_event = events.last().cloned().unwrap();
        store
            .transition_succeeded_with_manifest(&xai_grok_science::SuccessfulCompletionManifest {
                context: context.clone(),
                artifacts: store.artifacts(&ticket.run_id).unwrap(),
                evidence: store.evidence(&ticket.run_id).unwrap(),
                provenance: store.provenance(&ticket.run_id).unwrap(),
                previews: Vec::new(),
                events: events.clone(),
                final_event,
            })
            .unwrap();

        assert!(
            store
                .append_event(
                    &ticket.run_id,
                    "Intruder",
                    "migration.late-after-replay-begin",
                    serde_json::json!({}),
                )
                .is_err(),
            "Succeeded migration accepted a late event in the replay Begin/Finish window"
        );
        assert_eq!(
            validate_migration_success_events(&store, &ticket, &context, &request, &outcome)
                .unwrap(),
            events,
            "rejected late event changed the exact migration success protocol"
        );
    }

    #[test]
    fn raw_project_mutation_allow_without_private_grant_durably_denies_and_writes_nothing() {
        let root = tempfile::tempdir().unwrap();
        let workspace = dunce::canonicalize(root.path()).unwrap();
        let store_root = workspace.join("science-store");
        std::fs::create_dir_all(&store_root).unwrap();
        let store = xai_grok_science::ScienceStore::new_confined(&store_root, &workspace).unwrap();
        let project_store =
            xai_grok_science::project::ProjectStore::new_confined(&store_root, &workspace).unwrap();
        let expected_context = context(&workspace, &store_root);
        let request = xai_grok_science::project::MutationRequest {
            operation_id: "op-raw-allow-without-private-grant".into(),
            session_id: expected_context.session_id.clone(),
            owner_id: expected_context.owner_id.clone(),
            expected_revision: None,
            mutation: xai_grok_science::project::ProjectMutation::ProjectCreate {
                title: "Must not be created".into(),
                research_question: "Can raw Allow bypass SessionHandle?".into(),
            },
        };
        let ticket = begin_project_mutation_run(
            &store,
            expected_context.clone(),
            request.mutation.kind(),
            &request.operation_id,
        )
        .unwrap();
        let prepared = PreparedScienceProjectMutation {
            store: store.clone(),
            project_store: project_store.clone(),
            ticket: ticket.clone(),
            expected_context,
            request: request.clone(),
            project_revision: None,
            project_root: store_root,
            review_admission: None,
            migration_admission: None,
            target: "project create denied without private grant".into(),
            replayed: None,
            resume_allowed: false,
            permission_grant: None,
        };

        assert!(require_exact_project_mutation_allow_grant(&prepared).is_err());
        assert_eq!(
            store.load_run(&ticket.run_id).unwrap().state,
            xai_grok_science::RunState::Denied
        );
        assert_eq!(
            store.approvals(&ticket.run_id).unwrap()[0].decision,
            xai_grok_science::ApprovalDecision::Deny
        );
        assert!(store.artifacts(&ticket.run_id).unwrap().is_empty());
        assert!(store.evidence(&ticket.run_id).unwrap().is_empty());
        assert!(store.provenance(&ticket.run_id).unwrap().is_empty());
        assert!(store.previews(&ticket.run_id).unwrap().is_empty());
        assert!(
            project_store
                .lookup_operation(&request.operation_id)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn interrupted_review_commit_recovers_same_run_without_duplicate_evidence() {
        let root = tempfile::tempdir().unwrap();
        let workspace = dunce::canonicalize(root.path()).unwrap();
        let store_root = workspace.join("science-store");
        let project_store = xai_grok_science::project::ProjectStore::new(&store_root);
        let project = project_store
            .create_project("owner-1", "Recover review", "Can this commit resume?")
            .unwrap();
        let store = xai_grok_science::ScienceStore::new(&store_root);
        let source_run = xai_grok_science::RunId::new("source-run-recover");
        store
            .create_run(xai_grok_science::RunContext {
                run_id: source_run.clone(),
                project_id: xai_grok_science::ProjectId::new(project.project_id.0.clone()),
                session_id: "session-1".into(),
                owner_id: "owner-1".into(),
                workspace_root: workspace.clone(),
                provider: "offline-test".into(),
                approval_policy: "test".into(),
                tool_profile: "review-source".into(),
                artifact_root: store_root.join("runs"),
                environment: BTreeMap::new(),
            })
            .unwrap();
        let source_call = xai_grok_science::CallId::new("source-call");
        store
            .request_approval(xai_grok_science::Approval {
                project_id: xai_grok_science::ProjectId::new(project.project_id.0.clone()),
                run_id: source_run.clone(),
                call_id: source_call.clone(),
                owner_id: "owner-1".into(),
                decision: xai_grok_science::ApprovalDecision::Pending,
                decided_at: None,
            })
            .unwrap();
        store
            .transition(
                &source_run,
                xai_grok_science::RunState::AwaitingApproval,
                None,
            )
            .unwrap();
        store
            .decide_approval(
                &xai_grok_science::ProjectId::new(project.project_id.0.clone()),
                &source_run,
                "owner-1",
                &source_call,
                xai_grok_science::ApprovalDecision::Allow,
            )
            .unwrap();
        store
            .transition(&source_run, xai_grok_science::RunState::Running, None)
            .unwrap();
        let source_artifact = store
            .put_artifact(
                &xai_grok_science::ProjectId::new(project.project_id.0.clone()),
                &source_run,
                "owner-1",
                source_call,
                std::path::Path::new("result.json"),
                br#"{"result":"recoverable"}"#,
                "application/json",
                "source",
            )
            .unwrap();
        store
            .add_evidence(xai_grok_science::Evidence {
                run_id: source_run.clone(),
                claim: "Recovery source bytes were verified.".into(),
                source: "fixture://review-recovery".into(),
                artifact_sha256: Some(source_artifact.sha256.clone()),
                verified_at: chrono::Utc::now(),
            })
            .unwrap();
        store
            .add_provenance(xai_grok_science::Provenance {
                run_id: source_run.clone(),
                source_uri: "fixture://review-recovery".into(),
                source_commit: None,
                source_path: Some("result.json".into()),
                license: "test-only".into(),
                retrieved_at: chrono::Utc::now(),
                input_sha256: source_artifact.sha256.clone(),
                tool: "review-recovery-fixture".into(),
                environment: BTreeMap::new(),
            })
            .unwrap();
        store.transition_succeeded_verified(&source_run).unwrap();

        let mut request = xai_grok_science::project::MutationRequest {
            operation_id: "op-review-recover".into(),
            session_id: "session-1".into(),
            owner_id: "owner-1".into(),
            expected_revision: Some(project_store.project_revision(&project.project_id).unwrap()),
            mutation: xai_grok_science::project::ProjectMutation::ReviewRecord {
                project_id: project.project_id.clone(),
                reviewer_id: "owner-1".into(),
                verdict: xai_grok_science::project::ReviewVerdict::Pass,
                summary: "The exact source bytes support this recovery fixture.".into(),
                claim_id: None,
                source_run_id: source_run.0,
                authority_run_id: String::new(),
                artifact_sha256s: vec![source_artifact.sha256],
            },
        };
        let authority_run = xai_grok_science::RunId::new(format!(
            "review-authority-{}",
            request.replay_fingerprint().unwrap()
        ));
        let xai_grok_science::project::ProjectMutation::ReviewRecord {
            authority_run_id, ..
        } = &mut request.mutation
        else {
            unreachable!("review fixture must retain review mutation");
        };
        *authority_run_id = authority_run.0.clone();
        let admission =
            xai_grok_science::project::ReviewAdmission::capture(&project_store, &store, &request)
                .unwrap();
        let authority_context = xai_grok_science::RunContext {
            run_id: authority_run.clone(),
            project_id: xai_grok_science::ProjectId::new(project.project_id.0.clone()),
            session_id: "session-1".into(),
            owner_id: "owner-1".into(),
            workspace_root: workspace,
            provider: "offline-test".into(),
            approval_policy: "test".into(),
            tool_profile: "science-project-mutation-v1".into(),
            artifact_root: store_root.join("runs"),
            environment: admission.authority_environment(),
        };
        store.create_run(authority_context.clone()).unwrap();
        store
            .append_recoverable_commit_event(
                &authority_run,
                "SessionActor",
                "run.created",
                project_mutation_begin_event_payload(
                    &authority_context,
                    "review_record",
                    &request.operation_id,
                ),
            )
            .unwrap();
        let call_id = xai_grok_science::CallId::new("science_project_mutation");
        store
            .request_approval(xai_grok_science::Approval {
                project_id: xai_grok_science::ProjectId::new(project.project_id.0.clone()),
                run_id: authority_run.clone(),
                call_id: call_id.clone(),
                owner_id: "owner-1".into(),
                decision: xai_grok_science::ApprovalDecision::Pending,
                decided_at: None,
            })
            .unwrap();
        store
            .transition(
                &authority_run,
                xai_grok_science::RunState::AwaitingApproval,
                None,
            )
            .unwrap();
        store
            .decide_approval(
                &xai_grok_science::ProjectId::new(project.project_id.0.clone()),
                &authority_run,
                "owner-1",
                &call_id,
                xai_grok_science::ApprovalDecision::Allow,
            )
            .unwrap();
        store
            .append_recoverable_commit_event(
                &authority_run,
                "LumenApproval",
                "approval.allowed",
                serde_json::json!({"call_id": call_id.0}),
            )
            .unwrap();
        store
            .transition(&authority_run, xai_grok_science::RunState::Running, None)
            .unwrap();
        // Simulate the crash window after both atomic writes by removing only
        // the generic operation record. The immutable review ledger and its
        // original Running+Allow authority must be enough to recover without
        // minting a second run or permission.
        let operation_path = store_root
            .join("operations")
            .join(format!("{}.json", request.operation_id));
        let outcome = project_store
            .apply_actor_review(&request, &store, &admission)
            .unwrap();
        let review: xai_grok_science::project::ReviewRecord =
            serde_json::from_value(outcome.result).unwrap();
        std::fs::remove_file(&operation_path).unwrap();
        assert!(
            review_apply_error_may_have_committed(&store, &project_store, &request),
            "orphan review ledger was treated as a pre-commit rejection"
        );
        assert!(
            project_store
                .verify_review_record_with_store(&store, &review)
                .is_err()
        );
        assert_eq!(
            store.load_run(&authority_run).unwrap().state,
            xai_grok_science::RunState::Running
        );

        // Simulate the first crash window: review ledger exists, generic
        // operation ledger does not.
        assert!(
            project_store
                .lookup_operation(&request.operation_id)
                .unwrap()
                .is_none()
        );

        // Simulate the second window at the same time: the applied-event file
        // is temporarily unwritable. Recovery may persist the operation and
        // evidence, but must leave the authority run Running, never Failed.
        let events_path = store_root
            .join("runs")
            .join(&authority_run.0)
            .join("events.json");
        let events_before = std::fs::read(&events_path).unwrap();
        std::fs::remove_file(&events_path).unwrap();
        std::fs::create_dir(&events_path).unwrap();
        let retry = request.clone();
        assert!(recover_orphan_review_ledger(&store, &project_store, &retry, &review).is_err());
        assert_eq!(
            store.load_run(&authority_run).unwrap().state,
            xai_grok_science::RunState::Running,
            "recoverable applied-event failure terminalized the review authority"
        );
        assert!(
            project_store
                .lookup_operation(&request.operation_id)
                .unwrap()
                .is_some(),
            "orphan review recovery did not restore its operation ledger"
        );
        std::fs::remove_dir(&events_path).unwrap();
        std::fs::write(&events_path, events_before).unwrap();

        let operation = project_store
            .lookup_operation(&request.operation_id)
            .unwrap()
            .unwrap();
        recover_interrupted_review_commit(&store, &project_store, &operation, &review).unwrap();
        assert_eq!(
            store.load_run(&authority_run).unwrap().state,
            xai_grok_science::RunState::Succeeded
        );
        project_store
            .verify_review_record_with_store(&store, &review)
            .unwrap();
        assert_eq!(store.artifacts(&authority_run).unwrap().len(), 1);
        assert_eq!(store.evidence(&authority_run).unwrap().len(), 1);
        assert_eq!(store.provenance(&authority_run).unwrap().len(), 1);
        let events = store.events_after(&authority_run, 0, 1_000).unwrap();
        assert_eq!(
            events
                .iter()
                .map(|event| (event.actor.as_str(), event.kind.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("SessionActor", "run.created"),
                ("LumenApproval", "approval.allowed"),
                ("SessionActor", "project.mutation.applied"),
            ]
        );
        assert!(events[2].payload.get("replayed").is_none());

        let replay_outcome = xai_grok_science::project::MutationOutcome {
            operation_id: operation.operation_id.clone(),
            kind: operation.kind.clone(),
            project_id: operation.project_id.clone(),
            revision: operation.revision.clone(),
            result: operation.result.clone(),
            replayed: true,
        };
        let prepared_replay = PreparedScienceProjectMutation {
            store: store.clone(),
            project_store: project_store.clone(),
            ticket: xai_grok_science::csv::ScienceRunTicket {
                project_id: xai_grok_science::ProjectId::new(project.project_id.0.clone()),
                run_id: authority_run.clone(),
                owner_id: "owner-1".into(),
                call_id: call_id.clone(),
            },
            expected_context: authority_context,
            request: request.clone(),
            project_revision: Some(review.review_project_revision.clone()),
            project_root: store_root.clone(),
            review_admission: None,
            migration_admission: None,
            target: "verified review replay".into(),
            replayed: Some(replay_outcome.clone()),
            resume_allowed: false,
            permission_grant: None,
        };
        assert_eq!(
            finish_replayed_project_mutation(
                &prepared_replay,
                &replay_outcome,
                xai_grok_science::ApprovalDecision::Allow,
            )
            .unwrap(),
            replay_outcome
        );

        let seal_path = store_root
            .join("runs")
            .join(&authority_run.0)
            .join("successful-completion-seal.json");
        let seal_bytes = std::fs::read(&seal_path).unwrap();
        std::fs::write(&seal_path, b"{}").unwrap();
        assert!(
            finish_replayed_project_mutation(
                &prepared_replay,
                &replay_outcome,
                xai_grok_science::ApprovalDecision::Allow,
            )
            .is_err(),
            "Finish replay accepted a corrupt exact-completion seal"
        );
        assert!(
            store.load_run(&authority_run).is_err(),
            "ordinary reads served a run with a corrupt exact-completion seal"
        );
        std::fs::write(seal_path, seal_bytes).unwrap();
        assert_eq!(
            store.load_run(&authority_run).unwrap().state,
            xai_grok_science::RunState::Succeeded,
            "restoring the seal did not preserve the terminal authority state"
        );
        assert_eq!(
            store.events_after(&authority_run, 0, 1_000).unwrap(),
            events,
            "rejected replay Finish changed the durable event protocol"
        );
    }

    #[test]
    fn kernel_project_and_owner_binding_fails_closed() {
        let project = xai_grok_science::project::ResearchProject::new(
            xai_grok_science::project::ProjectId("project-a".into()),
            xai_grok_science::project::OwnerId("alice".into()),
            "Project A".into(),
            "Question".into(),
        );
        let workspace = tempfile::tempdir().unwrap();
        let project_root = workspace.path().join("science-store");
        let mut bound = context(workspace.path(), &project_root);
        bound.project_id = xai_grok_science::ProjectId::new("project-a");
        bound.owner_id = "alice".into();
        validate_kernel_project_binding(&project, &bound).unwrap();

        let mut wrong_project = bound.clone();
        wrong_project.project_id = xai_grok_science::ProjectId::new("project-b");
        assert!(validate_kernel_project_binding(&project, &wrong_project).is_err());

        let mut wrong_owner = bound;
        wrong_owner.owner_id = "mallory".into();
        assert!(matches!(
            validate_kernel_project_binding(&project, &wrong_owner),
            Err(xai_grok_science::ScienceError::Ownership)
        ));
    }

    #[test]
    fn kernel_session_binding_fails_closed() {
        let workspace = tempfile::tempdir().unwrap();
        let project_root = workspace.path().join("science-store");
        let mut bound = context(workspace.path(), &project_root);
        validate_kernel_session_binding("session-root-test", &bound).unwrap();

        bound.session_id = "foreign-session".into();
        assert!(validate_kernel_session_binding("session-root-test", &bound).is_err());
    }
}
