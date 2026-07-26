//! Lumen Science product dispatch. Seam contract: S2 and S4.

use super::*;
use crate::session::commands::{
    PreparedScienceCsv, PreparedScienceFetch, PreparedScienceImport,
    PreparedScienceProjectMutation, PreparedScienceSshScpAdmission,
    PreparedScienceWorkflowExecution,
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
        response
            .await
            .map_err(|_| {
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
        request: xai_grok_science::project::MutationRequest,
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
        let project_store = xai_grok_science::project::ProjectStore::new(&project_root);

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

        let target = match request.mutation.target_project() {
            Some(project_id) => format!("{}/projects/{}", project_root.display(), project_id.0),
            None => format!("{}/projects", project_root.display()),
        };

        // Idempotent replay: already applied, so no run and no second prompt.
        if let Some(record) = project_store.lookup_operation(&request.operation_id)? {
            if record.session_id != request.session_id || record.owner_id != request.owner_id {
                return Err(xai_grok_science::ScienceError::Ownership);
            }
            return Ok(PreparedScienceProjectMutation {
                store,
                project_root,
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

        let ticket = begin_project_mutation_run(&store, context, request.mutation.kind())?;
        Ok(PreparedScienceProjectMutation {
            store,
            project_root,
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
        let project_store = xai_grok_science::project::ProjectStore::new(&prepared.project_root);
        let outcome = match project_store.apply_mutation(&prepared.request) {
            Ok(outcome) => outcome,
            Err(error) => {
                let _ = xai_grok_science::csv::fail_running(
                    &prepared.store,
                    &prepared.ticket,
                    format!("project mutation rejected: {error}"),
                );
                return Err(error);
            }
        };
        prepared.store.append_event(
            &prepared.ticket.run_id,
            "SessionActor",
            "project.mutation.applied",
            serde_json::json!({
                "operation_id": outcome.operation_id,
                "kind": outcome.kind,
                "project_id": outcome.project_id.0,
                "revision": outcome.revision,
                "replayed": outcome.replayed,
            }),
        )?;
        prepared.store.transition(
            &prepared.ticket.run_id,
            xai_grok_science::RunState::Succeeded,
            None,
        )?;
        Ok(outcome)
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
        binding: crate::session::commands::ScienceWorkflowBinding,
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
        if !binding.interpreter_path.is_absolute() {
            return Err(ScienceError::Invalid(
                "interpreter path must be absolute; a kernel is never resolved from PATH".into(),
            ));
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
        let ledger = xai_grok_science::workflow::WorkflowExecutor::new(
            &binding.executor_root,
            workflow_compute_environment(&binding),
        );
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
            AdmissionStatus, DirCellSourceStore, ExecutionPolicy, KernelAdmissionRequest,
            KernelManifest, PythonLoopRunner, StepKind, WorkflowExecutor, WorkflowState,
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
        let policy = if binding.allow_kernel_steps {
            ExecutionPolicy::default().allowing_kernel_steps()
        } else {
            ExecutionPolicy::default()
        };

        let runner = PythonLoopRunner::new(
            loop_script,
            std::sync::Arc::new(DirCellSourceStore::new(&binding.cell_source_root)),
            &binding.output_root,
        );
        let executor = WorkflowExecutor::new(
            &binding.executor_root,
            workflow_compute_environment(binding),
        )
        .with_policy(policy)
        .with_runner(std::sync::Arc::new(runner))
        .with_kernels(KernelManifest {
            kernels: vec![admission],
            default_python: None,
            default_r: None,
            default_julia: None,
        })
        .map_err(&failed)?;

        let report = executor
            .execute(&binding.execution)
            .map_err(&failed)?;

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
        environment_allowlist: Vec::new(),
        cpu_identity: None,
        gpu_identity: None,
        deterministic_flags: vec!["PYTHONHASHSEED=0".into()],
        network_policy: xai_grok_science::workflow::NetworkPolicy::None,
        container_digest: None,
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
        serde_json::json!({"mutation": kind}),
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
