//! Lumen Science product dispatch. Seam contract: S2 and S4.

use super::*;
use crate::session::commands::{
    PreparedScienceCsv, PreparedScienceFetch, PreparedScienceImport,
    PreparedScienceKernelAdmission, PreparedScienceProjectMutation, PreparedScienceSeqAnalyze,
    PreparedScienceSshScpAdmission, PreparedScienceWorkflowExecution,
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
        context: xai_grok_science::RunContext,
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
            || !context.artifact_root.starts_with(&actor_workspace)
            || dunce::canonicalize(store.root())? != context.artifact_root
        {
            return Err(xai_grok_science::ScienceError::Invalid(
                "sequence analysis store or paths do not belong to this SessionActor workspace"
                    .into(),
            ));
        }
        let ticket = xai_grok_science::seqbench::begin_analysis_with_options(
            &store,
            context.clone(),
            &options,
        )?;
        let target = context
            .artifact_root
            .join("runs")
            .join(&ticket.run_id.0)
            .join("artifacts")
            .display()
            .to_string();
        Ok(PreparedScienceSeqAnalyze {
            store,
            ticket,
            options,
            source_path,
            source_bytes,
            target,
        })
    }

    pub(super) fn finish_science_seq_analyze(
        &self,
        prepared: PreparedScienceSeqAnalyze,
        decision: xai_grok_science::ApprovalDecision,
        reason: String,
    ) -> xai_grok_science::Result<xai_grok_science::seqbench::SeqAnalyzeResult> {
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
        xai_grok_science::seqbench::finish_analysis_with_options(
            &prepared.store,
            prepared.ticket,
            &prepared.source_path,
            &prepared.source_bytes,
            &prepared.options,
        )
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
    ) -> xai_grok_science::Result<PreparedScienceFetch> {
        if requests.len() != fixture_bytes.len() || requests.is_empty() {
            return Err(xai_grok_science::ScienceError::Invalid(
                "fetch requires one staged response per request".into(),
            ));
        }
        let ticket = xai_grok_science::connectors::fetch::begin_fetch(&store, context.clone())?;
        let staging = context
            .artifact_root
            .join(&ticket.run_id.0)
            .join("tool-staging");
        std::fs::create_dir_all(&staging)?;
        let mut command = format!("python3 -c {}", quote(FETCH_TOOL_SCRIPT)?);
        let mut output_paths = Vec::with_capacity(fixture_bytes.len());
        for (index, bytes) in fixture_bytes.iter().enumerate() {
            let input_path = staging.join(format!("input_{index}.bin"));
            let output_path = staging.join(format!("output_{index}.bin"));
            std::fs::write(&input_path, bytes)?;
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
        context: xai_grok_science::RunContext,
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

        // Project binding: the run context must name the project actually
        // being mutated, so the durable record cannot point at another one.
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
        }
        if let xai_grok_science::project::ProjectMutation::ReviewRecord {
            source_run_id,
            reviewer_id,
            ..
        } = &request.mutation
        {
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
        }

        let target = match request.mutation.target_project() {
            Some(project_id) => format!("{}/projects/{}", project_root.display(), project_id.0),
            None => format!("{}/projects", project_root.display()),
        };

        // Idempotent replay: already applied, so no run and no second prompt.
        if let Some(record) = project_store.lookup_operation(&request.operation_id)? {
            record.verify_replay(&request)?;
            if record.kind == "review_record" {
                let review: xai_grok_science::project::ReviewRecord =
                    serde_json::from_value(record.result.clone())?;
                validate_review_replay_request(&request, &review)?;
                if project_store.verify_review_record(&review).is_err() {
                    recover_interrupted_review_commit(&store, &project_store, &record, &review)?;
                }
                project_store.verify_review_record(&review)?;
            }
            return Ok(PreparedScienceProjectMutation {
                store,
                project_store,
                ticket: xai_grok_science::csv::ScienceRunTicket {
                    // The run ticket uses the kernel's ProjectId; the record
                    // carries the project-model one.
                    project_id: xai_grok_science::ProjectId::new(record.project_id.0.clone()),
                    run_id: context.run_id.clone(),
                    owner_id: record.owner_id.clone(),
                    call_id: xai_grok_science::CallId::new("science_project_mutation"),
                },
                request,
                target,
                replayed: Some(xai_grok_science::project::MutationOutcome {
                    operation_id: record.operation_id,
                    kind: record.kind,
                    project_id: record.project_id,
                    revision: record.revision,
                    result: record.result,
                    replayed: true,
                }),
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
            let outcome =
                recover_orphan_review_ledger(&store, &project_store, &mut request, &review)?;
            return Ok(PreparedScienceProjectMutation {
                store,
                project_store,
                ticket: xai_grok_science::csv::ScienceRunTicket {
                    project_id: xai_grok_science::ProjectId::new(review.project_id.0.clone()),
                    run_id: xai_grok_science::RunId::new(&review.authority_run_id),
                    owner_id: review.owner_id.clone(),
                    call_id: xai_grok_science::CallId::new("science_project_mutation"),
                },
                request,
                target,
                replayed: Some(xai_grok_science::project::MutationOutcome {
                    replayed: true,
                    ..outcome
                }),
            });
        }

        let ticket = begin_project_mutation_run(
            &store,
            context,
            request.mutation.kind(),
            &request.operation_id,
        )?;
        Ok(PreparedScienceProjectMutation {
            store,
            project_store,
            ticket,
            request,
            target,
            replayed: None,
        })
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
        if let Some(outcome) = prepared.replayed {
            return Ok(outcome);
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
        let outcome = match prepared.project_store.apply_mutation(&prepared.request) {
            Ok(outcome) => outcome,
            Err(error) => {
                if review_apply_error_may_have_committed(&prepared.project_store, &prepared.request)
                {
                    let _ = prepared.store.append_recoverable_commit_event(
                        &prepared.ticket.run_id,
                        "SessionActor",
                        "review.commit.interrupted",
                        serde_json::json!({
                            "operation_id": prepared.request.operation_id,
                            "reason": error.to_string(),
                            "stage": "project-ledger",
                        }),
                    );
                    return Err(error);
                }
                let _ = xai_grok_science::csv::fail_running(
                    &prepared.store,
                    &prepared.ticket,
                    format!("project mutation rejected: {error}"),
                );
                return Err(error);
            }
        };
        let review =
            match persist_review_mutation_evidence(&prepared.store, &prepared.ticket, &outcome) {
                Ok(review) => review,
                Err(error) => {
                    // Once a review + operation ledger exists, the run must stay
                    // Running: a retry can then reopen that exact record and
                    // idempotently finish its manifest/evidence/provenance. Marking
                    // it Failed here would permanently poison the operation id.
                    let _ = prepared.store.append_recoverable_commit_event(
                        &prepared.ticket.run_id,
                        "SessionActor",
                        "review.commit.interrupted",
                        serde_json::json!({
                            "operation_id": outcome.operation_id,
                            "reason": error.to_string(),
                        }),
                    );
                    return Err(error);
                }
            };
        if let Some(review) = review {
            prepared
                .project_store
                .verify_pending_review_commit(&review)?;
        }
        append_project_mutation_applied_once(&prepared.store, &prepared.ticket.run_id, &outcome)?;
        prepared.store.transition(
            &prepared.ticket.run_id,
            xai_grok_science::RunState::Succeeded,
            None,
        )?;
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
        context: xai_grok_science::RunContext,
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
        let project = project_store.load_project(&binding.execution.spec.project_id)?;
        if project.owner_id.0 != binding.execution.owner_id {
            return Err(ScienceError::Ownership);
        }

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
        let ledger = xai_grok_science::workflow::WorkflowExecutor::new_confined(
            &binding.executor_root,
            std::path::Path::new(&self.session_info.cwd),
            workflow_compute_environment(&binding),
        )?
        .with_policy(workflow_execution_policy(&binding));
        if ledger
            .lookup_operation(&binding.execution.operation_id)?
            .is_some()
        {
            let report = ledger.execute(&binding.execution)?;
            return Ok(PreparedScienceWorkflowExecution {
                store,
                ticket: xai_grok_science::csv::ScienceRunTicket {
                    project_id: context.project_id.clone(),
                    run_id: context.run_id.clone(),
                    owner_id: context.owner_id.clone(),
                    call_id: xai_grok_science::CallId::new("science_workflow_execute"),
                },
                target: workflow_permission_target(&binding),
                binding,
                replayed: Some(report),
            });
        }

        let target = workflow_permission_target(&binding);
        let ticket = begin_workflow_execution_run(&store, context, &binding)?;
        Ok(PreparedScienceWorkflowExecution {
            store,
            ticket,
            binding,
            target,
            replayed: None,
        })
    }

    /// Phase two: build the executor and run the workflow, but ONLY on an allow
    /// decision.
    ///
    /// Everything that touches the filesystem or spawns a process lives on this
    /// side of the gate — materialising the exec-loop driver, staging cell
    /// sources, probing the kernel, and the run itself. A denied, cancelled or
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
            AdmissionStatus, DirCellSourceStore, KernelAdmissionRequest, KernelManifest,
            PythonLoopRunner, StepKind, WorkflowExecutor, WorkflowState,
            materialize_python_loop_script, probe_kernel,
        };

        if let Some(report) = prepared.replayed {
            return Ok(report);
        }
        if decision != xai_grok_science::ApprovalDecision::Allow {
            let terminal = xai_grok_science::csv::finish_without_execution(
                &prepared.store,
                &prepared.ticket,
                decision,
                reason,
            )?;
            return Err(ScienceError::Invalid(format!(
                "science run {} finished {:?}",
                prepared.ticket.run_id.0, terminal.state
            )));
        }
        xai_grok_science::csv::mark_allowed(&prepared.store, &prepared.ticket)?;

        let binding = &prepared.binding;
        let failed = |error: ScienceError| -> ScienceError {
            let _ = xai_grok_science::csv::fail_running(
                &prepared.store,
                &prepared.ticket,
                format!("workflow execution rejected: {error}"),
            );
            error
        };

        // The driver script, from the bytes compiled into this binary.
        let loop_script = match materialize_python_loop_script(&binding.runtime_root) {
            Ok(path) => path,
            Err(error) => {
                return Err(failed(ScienceError::Invalid(format!(
                    "cannot materialise the kernel exec-loop: {error}"
                ))));
            }
        };

        // Stage every cell body the spec carries into the content-addressed
        // source store. The runner re-hashes whatever it loads, so this is a
        // delivery step, not a trust step: a source that does not hash to the
        // digest the plan names still fails the step.
        for step in &binding.execution.spec.steps {
            if step.kind != StepKind::NotebookCell {
                continue;
            }
            let Some(source) = step.notebook_cell.as_ref() else {
                continue;
            };
            let digest = format!("{:x}", sha2::Sha256::digest(source.as_bytes()));
            if let Err(error) = std::fs::create_dir_all(&binding.cell_source_root)
                .and_then(|()| std::fs::write(binding.cell_source_root.join(digest), source))
            {
                return Err(failed(ScienceError::Invalid(format!(
                    "cannot stage the source of step '{}': {error}",
                    step.step_id
                ))));
            }
        }

        // Probe the interpreter. This RUNS it, which is why it is here and not
        // in `prepare_*`.
        let admission = probe_kernel(
            &KernelAdmissionRequest::new(
                binding.kernel_id.clone(),
                binding.kernel_kind,
                binding.interpreter_path.clone(),
            )
            .with_admitted_by(format!("session-actor:{}", self.session_info.id.0))
            .with_probe_timeout(binding.probe_timeout),
        )
        .map_err(&failed)?;
        if admission.admission_status != AdmissionStatus::Admitted {
            return Err(failed(ScienceError::Invalid(format!(
                "kernel '{}' was not admitted ({:?}); no step may run on it",
                admission.kernel_id, admission.admission_status
            ))));
        }

        // `ExecutionPolicy::default()` omits NotebookCell so that running
        // arbitrary code is a decision. The decision arrives in the request and
        // is applied here; the default itself is never lowered.
        let policy = workflow_execution_policy(binding);

        let runner = PythonLoopRunner::new(
            loop_script,
            std::sync::Arc::new(DirCellSourceStore::new(&binding.cell_source_root)),
            &binding.output_root,
        );
        let executor = WorkflowExecutor::new_confined(
            &binding.executor_root,
            std::path::Path::new(&self.session_info.cwd),
            workflow_compute_environment(binding),
        )
        .map_err(&failed)?
        .with_policy(policy)
        .with_runner(std::sync::Arc::new(runner))
        .with_kernels(KernelManifest {
            kernels: vec![admission],
            default_python: None,
            default_r: None,
            default_julia: None,
        })
        .map_err(&failed)?;

        let report = executor.execute(&binding.execution).map_err(&failed)?;

        prepared.store.append_event(
            &prepared.ticket.run_id,
            "SessionActor",
            "workflow.execution.finished",
            serde_json::json!({
                "operation_id": report.run.operation_id,
                "workflow_run_id": report.run.run_id,
                "workflow_id": report.run.workflow_id,
                "state": format!("{:?}", report.run.state),
                "artifacts_committed": report.artifacts_committed,
                "steps_reused": report.steps_reused,
                "replayed": report.replayed,
            }),
        )?;
        // The ACP call succeeded either way; the Science run records what the
        // WORKFLOW did, so a failed workflow is not filed as a successful run.
        let terminal = if report.run.state == WorkflowState::Succeeded {
            xai_grok_science::RunState::Succeeded
        } else {
            xai_grok_science::RunState::Failed
        };
        prepared
            .store
            .transition(&prepared.ticket.run_id, terminal, None)?;
        Ok(report)
    }
}

/// What the permission prompt names. A workflow step spawns an interpreter, so
/// the prompt says which interpreter and which workflow — not merely "a
/// workflow ran".
fn workflow_permission_target(
    binding: &crate::session::commands::ScienceWorkflowBinding,
) -> String {
    format!(
        "execute workflow '{}' ({} step(s)) on {}",
        binding.execution.spec.workflow_id,
        binding.execution.spec.steps.len(),
        binding.interpreter_path.display()
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
) -> xai_grok_science::workflow::ComputeEnvironment {
    xai_grok_science::workflow::ComputeEnvironment {
        environment_id: format!("session-actor:{}", binding.kernel_id),
        os: std::env::consts::OS.to_string(),
        architecture: std::env::consts::ARCH.to_string(),
        lumen_binary_hash: format!("version:{}", xai_grok_version::VERSION),
        rust_lock_hash: None,
        python_hash: None,
        r_hash: None,
        julia_hash: None,
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
    let ticket = xai_grok_science::csv::ScienceRunTicket {
        project_id: context.project_id.clone(),
        run_id: context.run_id.clone(),
        owner_id: context.owner_id.clone(),
        call_id: xai_grok_science::CallId::new("science_workflow_execute"),
    };
    store.create_run(context)?;
    store.append_event(
        &ticket.run_id,
        "SessionActor",
        "run.created",
        serde_json::json!({
            "workflow_id": binding.execution.spec.workflow_id,
            "operation_id": binding.execution.operation_id,
            "steps": binding.execution.spec.steps.len(),
            "allow_kernel_steps": binding.allow_kernel_steps,
            "interpreter": binding.interpreter_path.display().to_string(),
        }),
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

/// Open the durable run + pending approval for a project mutation. Mirrors
/// `csv::begin_fixture`, with a call id that names this product path.
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
    store.create_run(context)?;
    store.append_event(
        &ticket.run_id,
        "SessionActor",
        "run.created",
        serde_json::json!({
            "mutation": kind,
            "operation_id": operation_id,
        }),
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
            let reopened = store.artifact_bytes(
                &ticket.project_id,
                &ticket.run_id,
                &ticket.owner_id,
                &artifact.relative_path,
            )?;
            if reopened != manifest {
                return Err(xai_grok_science::ScienceError::Invalid(
                    "existing review manifest bytes do not match the operation record".into(),
                ));
            }
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
        artifact_sha256s,
        ..
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
    if request.operation_id != record.operation_id
        || request.session_id != record.session_id
        || request.owner_id != record.owner_id
        || project_id != &record.project_id
        || reviewer_id != &record.reviewer_id
        || verdict != &record.verdict
        || summary != &record.summary
        || claim_id != &record.claim_id
        || source_run_id != &record.source_run_id
        || requested != recorded
    {
        return Err(xai_grok_science::ScienceError::Invalid(
            "review operation replay does not match its original request".into(),
        ));
    }
    Ok(())
}

fn append_project_mutation_applied_once(
    store: &xai_grok_science::ScienceStore,
    run_id: &xai_grok_science::RunId,
    outcome: &xai_grok_science::project::MutationOutcome,
) -> xai_grok_science::Result<()> {
    let payload = serde_json::json!({
        "operation_id": outcome.operation_id,
        "kind": outcome.kind,
        "project_id": outcome.project_id.0,
        "revision": outcome.revision,
        "replayed": outcome.replayed,
    });
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

fn recover_interrupted_review_commit(
    store: &xai_grok_science::ScienceStore,
    project_store: &xai_grok_science::project::ProjectStore,
    operation: &xai_grok_science::project::OperationRecord,
    review: &xai_grok_science::project::ReviewRecord,
) -> xai_grok_science::Result<()> {
    project_store.verify_pending_review_record(review)?;
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
    persist_review_mutation_evidence(store, &ticket, &outcome)?;
    project_store.verify_pending_review_commit(review)?;
    append_project_mutation_applied_once(store, &ticket.run_id, &outcome)?;
    store.transition(&ticket.run_id, xai_grok_science::RunState::Succeeded, None)?;
    Ok(())
}

fn recover_orphan_review_ledger(
    store: &xai_grok_science::ScienceStore,
    project_store: &xai_grok_science::project::ProjectStore,
    request: &mut xai_grok_science::project::MutationRequest,
    review: &xai_grok_science::project::ReviewRecord,
) -> xai_grok_science::Result<xai_grok_science::project::MutationOutcome> {
    validate_review_replay_request(request, review)?;
    project_store.verify_pending_review_record(review)?;
    // The compare-and-swap guarded the original write. The review ledger now
    // contributes to the project revision, so replaying the pre-write token
    // would necessarily conflict; only the already-validated ledger is being
    // adopted here.
    request.expected_revision = None;
    if let xai_grok_science::project::ProjectMutation::ReviewRecord {
        authority_run_id, ..
    } = &mut request.mutation
    {
        *authority_run_id = review.authority_run_id.clone();
    }
    let outcome = project_store.apply_mutation(request)?;
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
                && project_store.verify_pending_review_record(&review).is_ok()
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
        let source_artifact = store
            .put_artifact(
                &xai_grok_science::ProjectId::new(project.project_id.0.clone()),
                &source_run,
                "owner-1",
                xai_grok_science::CallId::new("source-call"),
                std::path::Path::new("result.json"),
                br#"{"result":"recoverable"}"#,
                "application/json",
                "source",
            )
            .unwrap();
        store
            .transition(&source_run, xai_grok_science::RunState::Succeeded, None)
            .unwrap();

        let authority_run = xai_grok_science::RunId::new("review-run-recover");
        store
            .create_run(xai_grok_science::RunContext {
                run_id: authority_run.clone(),
                project_id: xai_grok_science::ProjectId::new(project.project_id.0.clone()),
                session_id: "session-1".into(),
                owner_id: "owner-1".into(),
                workspace_root: workspace,
                provider: "offline-test".into(),
                approval_policy: "test".into(),
                tool_profile: "science-project-mutation-v1".into(),
                artifact_root: store_root.join("runs"),
                environment: BTreeMap::new(),
            })
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
            .decide_approval(
                &xai_grok_science::ProjectId::new(project.project_id.0.clone()),
                &authority_run,
                "owner-1",
                &call_id,
                xai_grok_science::ApprovalDecision::Allow,
            )
            .unwrap();
        store
            .transition(&authority_run, xai_grok_science::RunState::Running, None)
            .unwrap();

        let request = xai_grok_science::project::MutationRequest {
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
                authority_run_id: authority_run.0.clone(),
                artifact_sha256s: vec![source_artifact.sha256],
            },
        };
        // Force the real synchronous error path: the review ledger write
        // succeeds, then record_operation fails because its target is a
        // directory. The actor must recognize the durable orphan and avoid
        // fail_running.
        let operation_path = store_root
            .join("operations")
            .join(format!("{}.json", request.operation_id));
        std::fs::create_dir_all(&operation_path).unwrap();
        assert!(project_store.apply_mutation(&request).is_err());
        assert!(
            review_apply_error_may_have_committed(&project_store, &request),
            "post-review operation failure was treated as a pre-commit rejection"
        );
        let review = project_store
            .lookup_review_record(&project.project_id, &request.operation_id)
            .unwrap()
            .unwrap();
        assert!(project_store.verify_review_record(&review).is_err());
        assert_eq!(
            store.load_run(&authority_run).unwrap().state,
            xai_grok_science::RunState::Running
        );

        // Simulate the first crash window: review ledger exists, generic
        // operation ledger does not.
        std::fs::remove_dir(&operation_path).unwrap();
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
        let mut retry = request.clone();
        assert!(recover_orphan_review_ledger(&store, &project_store, &mut retry, &review).is_err());
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
        project_store.verify_review_record(&review).unwrap();
        assert_eq!(store.artifacts(&authority_run).unwrap().len(), 1);
        assert_eq!(store.evidence(&authority_run).unwrap().len(), 1);
        assert_eq!(store.provenance(&authority_run).unwrap().len(), 1);
        assert_eq!(
            store
                .events_after(&authority_run, 0, 1_000)
                .unwrap()
                .iter()
                .filter(|event| event.kind == "project.mutation.applied")
                .count(),
            1
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
