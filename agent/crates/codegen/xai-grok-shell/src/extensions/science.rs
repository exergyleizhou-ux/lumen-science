//! Non-test ACP product entry for Lumen Science. Seam contract: S1, S2, S4.

use super::{ExtResult, parse_params, to_raw_response};
use crate::agent::MvpAgent;
use agent_client_protocol as acp;
use serde::Deserialize;
use std::{collections::BTreeMap, path::PathBuf, time::Duration};
use xai_grok_science::{ProjectId, RunContext, RunId, ScienceStore};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RunCsvParams {
    session_id: String,
    project_id: String,
    owner_id: String,
    store_root: PathBuf,
    artifact_root: PathBuf,
    fixture_path: PathBuf,
    #[serde(default = "default_approval_timeout_ms")]
    approval_timeout_ms: u64,
}

fn default_approval_timeout_ms() -> u64 {
    120_000
}

fn internal(error: impl std::fmt::Display) -> acp::Error {
    acp::Error::internal_error().data(error.to_string())
}

fn canonical_dir_within(path: PathBuf, workspace: &std::path::Path) -> Result<PathBuf, acp::Error> {
    std::fs::create_dir_all(&path).map_err(internal)?;
    let canonical = std::fs::canonicalize(path).map_err(internal)?;
    if !canonical.starts_with(workspace) {
        return Err(acp::Error::invalid_params().data("science path must be inside session cwd"));
    }
    Ok(canonical)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ImportPreviewParams {
    session_id: String,
    project_id: String,
    owner_id: String,
    store_root: PathBuf,
    artifact_root: PathBuf,
    source_path: PathBuf,
    #[serde(default = "default_approval_timeout_ms")]
    approval_timeout_ms: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ConnectorFetchParams {
    session_id: String,
    project_id: String,
    owner_id: String,
    store_root: PathBuf,
    artifact_root: PathBuf,
    connector_id: String,
    query: String,
    #[serde(default = "default_max_results")]
    max_results: u32,
    /// Offline mock transport: one local fixture file per protocol exchange,
    /// standing in for the HTTP responses. Live transport is not wired here;
    /// the audited live probe lives in the science crate's ignored tests.
    fixture_paths: Vec<PathBuf>,
    #[serde(default = "default_approval_timeout_ms")]
    approval_timeout_ms: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SshScpFixtureParams {
    session_id: String,
    project_id: String,
    owner_id: String,
    store_root: PathBuf,
    artifact_root: PathBuf,
    port: u16,
    host_key_sha256: String,
    user: String,
    identity_file: PathBuf,
    known_hosts_file: PathBuf,
    ssh_config_file: PathBuf,
    direction: String,
    local_path: PathBuf,
    remote_path: String,
    #[serde(default = "default_approval_timeout_ms")]
    approval_timeout_ms: u64,
    #[serde(default = "default_approval_timeout_ms")]
    transport_timeout_ms: u64,
    cancel_after_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GoalHostVerifyParams {
    session_id: String,
    store_root: PathBuf,
    run_id: String,
}

fn default_max_results() -> u32 {
    5
}

pub async fn handle(agent: &MvpAgent, args: &acp::ExtRequest) -> ExtResult {
    match args.method.as_ref() {
        "x.ai/science/run_csv" => handle_run_csv(agent, args).await,
        "x.ai/science/import_preview" => handle_import_preview(agent, args).await,
        "x.ai/science/connector_fetch" => handle_connector_fetch(agent, args).await,
        "x.ai/science/ssh_scp_fixture" => handle_ssh_scp_fixture(agent, args).await,
        "x.ai/science/goal_host_verify" => handle_goal_host_verify(agent, args).await,
        "x.ai/science/seq_analyze" => handle_seq_analyze(agent, args).await,
        "x.ai/science/project_create" => handle_project_create(agent, args).await,
        "x.ai/science/project_get" => handle_project_get(agent, args).await,
        "x.ai/science/project_list" => handle_project_list(agent, args).await,
        "x.ai/science/project_transition" => handle_project_transition(agent, args).await,
        "x.ai/science/claim_propose" => handle_claim_propose(agent, args).await,
        "x.ai/science/evidence_attach" => handle_evidence_attach(agent, args).await,
        // WP-3 evidence queries
        "x.ai/science/evidence_trace" => handle_evidence_trace(agent, args).await,
        "x.ai/science/evidence_compare" => handle_evidence_compare(agent, args).await,
        "x.ai/science/evidence_consistency" => handle_evidence_consistency(agent, args).await,
        "x.ai/science/evidence_reproduction" => handle_evidence_reproduction(agent, args).await,
        "x.ai/science/project_migrate" => handle_project_migrate(agent, args).await,
        // WP-4/5/6/7/8 preview
        "x.ai/science/workflow_validate" => handle_workflow_validate(agent, args).await,
        "x.ai/science/workflow_dry_run" => handle_workflow_dry_run(agent, args).await,
        "x.ai/science/kernel_admission" => handle_kernel_admission(agent, args).await,
        "x.ai/science/multimodal_index" => handle_multimodal_index(agent, args).await,
        "x.ai/science/review_record" => handle_review_record(agent, args).await,
        "x.ai/science/collaboration_invite" => handle_collaboration_invite(agent, args).await,
        "x.ai/science/remote_compute_plan" => handle_remote_compute_plan(agent, args).await,
        _ => Err(acp::Error::method_not_found()),
    }
}

// ── WP-2 product path: ResearchProject + EvidenceGraph + Claims ──

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectCreateParams {
    session_id: String,
    owner_id: String,
    store_root: PathBuf,
    title: String,
    research_question: String,
}

async fn handle_project_create(agent: &MvpAgent, args: &acp::ExtRequest) -> ExtResult {
    let params: ProjectCreateParams = parse_params(args)?;
    if params.owner_id.is_empty() || params.title.is_empty() {
        return Err(acp::Error::invalid_params().data("ownerId and title are required"));
    }
    let session_id = acp::SessionId::new(params.session_id);
    let handle = agent
        .get_session_handle(&session_id)
        .ok_or_else(|| acp::Error::invalid_params().data("session not found"))?;
    let workspace = std::fs::canonicalize(&handle.info.cwd).map_err(internal)?;
    let store_root = canonical_dir_within(params.store_root, &workspace)?;
    let store = xai_grok_science::project::ProjectStore::new(store_root);
    let project = store
        .create_project(&params.owner_id, params.title, params.research_question)
        .map_err(internal)?;
    to_raw_response(&serde_json::json!({
        "projectId": project.project_id.0,
        "ownerId": project.owner_id.0,
        "title": project.title,
        "status": format!("{:?}", project.status),
        "evidenceGraphId": project.evidence_graph_id,
        "featureGate": "research_project=preview",
        "runtimeAuthority": "SessionActor-gated ACP adapter",
    }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectGetParams {
    session_id: String,
    store_root: PathBuf,
    project_id: String,
}

async fn handle_project_get(agent: &MvpAgent, args: &acp::ExtRequest) -> ExtResult {
    let params: ProjectGetParams = parse_params(args)?;
    let session_id = acp::SessionId::new(params.session_id);
    let handle = agent
        .get_session_handle(&session_id)
        .ok_or_else(|| acp::Error::invalid_params().data("session not found"))?;
    let workspace = std::fs::canonicalize(&handle.info.cwd).map_err(internal)?;
    let store_root = canonical_dir_within(params.store_root, &workspace)?;
    let store = xai_grok_science::project::ProjectStore::new(store_root);
    let pid = xai_grok_science::project::ProjectId(params.project_id);
    let bundle = store.load_bundle(&pid).map_err(internal)?;
    to_raw_response(&bundle)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectListParams {
    session_id: String,
    store_root: PathBuf,
}

async fn handle_project_list(agent: &MvpAgent, args: &acp::ExtRequest) -> ExtResult {
    let params: ProjectListParams = parse_params(args)?;
    let session_id = acp::SessionId::new(params.session_id);
    let handle = agent
        .get_session_handle(&session_id)
        .ok_or_else(|| acp::Error::invalid_params().data("session not found"))?;
    let workspace = std::fs::canonicalize(&handle.info.cwd).map_err(internal)?;
    let store_root = canonical_dir_within(params.store_root, &workspace)?;
    let store = xai_grok_science::project::ProjectStore::new(store_root);
    let projects = store.list_projects().map_err(internal)?;
    to_raw_response(&projects)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectTransitionParams {
    session_id: String,
    store_root: PathBuf,
    project_id: String,
    owner_id: String,
    /// Draft | Planned | Active | ReviewPending | Accepted | Rejected | Inconclusive | Archived
    status: String,
}

async fn handle_project_transition(agent: &MvpAgent, args: &acp::ExtRequest) -> ExtResult {
    let params: ProjectTransitionParams = parse_params(args)?;
    let session_id = acp::SessionId::new(params.session_id);
    let handle = agent
        .get_session_handle(&session_id)
        .ok_or_else(|| acp::Error::invalid_params().data("session not found"))?;
    let workspace = std::fs::canonicalize(&handle.info.cwd).map_err(internal)?;
    let store_root = canonical_dir_within(params.store_root, &workspace)?;
    let store = xai_grok_science::project::ProjectStore::new(store_root);
    let status = parse_project_status(&params.status)
        .ok_or_else(|| acp::Error::invalid_params().data("invalid status"))?;
    let project = store
        .transition_project(
            &xai_grok_science::project::ProjectId(params.project_id),
            &params.owner_id,
            status,
        )
        .map_err(internal)?;
    to_raw_response(&project)
}

fn parse_project_status(s: &str) -> Option<xai_grok_science::project::ProjectStatus> {
    use xai_grok_science::project::ProjectStatus::*;
    match s.to_ascii_lowercase().as_str() {
        "draft" => Some(Draft),
        "planned" => Some(Planned),
        "active" => Some(Active),
        "reviewpending" | "review_pending" => Some(ReviewPending),
        "accepted" => Some(Accepted),
        "rejected" => Some(Rejected),
        "inconclusive" => Some(Inconclusive),
        "archived" => Some(Archived),
        _ => None,
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ClaimProposeParams {
    session_id: String,
    store_root: PathBuf,
    project_id: String,
    owner_id: String,
    statement: String,
    proposed_by: String,
}

async fn handle_claim_propose(agent: &MvpAgent, args: &acp::ExtRequest) -> ExtResult {
    let params: ClaimProposeParams = parse_params(args)?;
    if params.statement.is_empty() {
        return Err(acp::Error::invalid_params().data("statement is required"));
    }
    let session_id = acp::SessionId::new(params.session_id);
    let handle = agent
        .get_session_handle(&session_id)
        .ok_or_else(|| acp::Error::invalid_params().data("session not found"))?;
    let workspace = std::fs::canonicalize(&handle.info.cwd).map_err(internal)?;
    let store_root = canonical_dir_within(params.store_root, &workspace)?;
    let store = xai_grok_science::project::ProjectStore::new(store_root);
    let claim = store
        .propose_claim(
            &xai_grok_science::project::ProjectId(params.project_id),
            &params.owner_id,
            params.statement,
            params.proposed_by,
        )
        .map_err(internal)?;
    to_raw_response(&claim)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EvidenceAttachParams {
    session_id: String,
    store_root: PathBuf,
    project_id: String,
    owner_id: String,
    claim_id: String,
    artifact_sha256: String,
    label: String,
    #[serde(default)]
    run_id: Option<String>,
}

async fn handle_evidence_attach(agent: &MvpAgent, args: &acp::ExtRequest) -> ExtResult {
    let params: EvidenceAttachParams = parse_params(args)?;
    let session_id = acp::SessionId::new(params.session_id);
    let handle = agent
        .get_session_handle(&session_id)
        .ok_or_else(|| acp::Error::invalid_params().data("session not found"))?;
    let workspace = std::fs::canonicalize(&handle.info.cwd).map_err(internal)?;
    let store_root = canonical_dir_within(params.store_root, &workspace)?;
    let store = xai_grok_science::project::ProjectStore::new(store_root);
    let (claim, graph) = store
        .attach_evidence(
            &xai_grok_science::project::ProjectId(params.project_id),
            &params.owner_id,
            &params.claim_id,
            params.artifact_sha256,
            params.label,
            params.run_id,
        )
        .map_err(internal)?;
    to_raw_response(&serde_json::json!({
        "claim": claim,
        "nodeCount": graph.nodes.len(),
        "edgeCount": graph.edges.len(),
    }))
}

/// Offline Motif-class sequence analysis product path.
/// Reads a workspace FASTA, computes deterministic analysis, writes derived
/// artifacts under artifactRoot (analysis.json + report.md) with SHA-256.
/// No network. Session must exist; source must be inside session cwd.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SeqAnalyzeParams {
    session_id: String,
    project_id: String,
    owner_id: String,
    artifact_root: PathBuf,
    source_path: PathBuf,
}

async fn handle_seq_analyze(agent: &MvpAgent, args: &acp::ExtRequest) -> ExtResult {
    let params: SeqAnalyzeParams = parse_params(args)?;
    if params.project_id.is_empty() || params.owner_id.is_empty() {
        return Err(acp::Error::invalid_params().data("projectId and ownerId are required"));
    }
    let session_id = acp::SessionId::new(params.session_id);
    let handle = agent
        .get_session_handle(&session_id)
        .ok_or_else(|| acp::Error::invalid_params().data("session not found"))?;
    let workspace = std::fs::canonicalize(&handle.info.cwd).map_err(internal)?;
    let source_path = std::fs::canonicalize(&params.source_path).map_err(internal)?;
    if !source_path.starts_with(&workspace) || !source_path.is_file() {
        return Err(
            acp::Error::invalid_params().data("sourcePath must be a file inside session cwd")
        );
    }
    let artifact_root = canonical_dir_within(params.artifact_root, &workspace)?;
    let bytes = std::fs::read(&source_path).map_err(internal)?;
    if bytes.len() > 32 * 1024 * 1024 {
        return Err(acp::Error::invalid_params().data("source exceeds 32 MiB cap"));
    }
    let text = String::from_utf8_lossy(&bytes);
    let records = xai_grok_science::seqbench::parse_fasta(&text)
        .map_err(|e| acp::Error::invalid_params().data(e))?;
    let analysis = xai_grok_science::seqbench::analyze(&records, &bytes);
    let report = xai_grok_science::seqbench::markdown_report(
        &analysis,
        source_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("input.fa"),
    );
    let analysis_json = serde_json::to_vec_pretty(&analysis).map_err(internal)?;
    let out_dir = artifact_root
        .join(&params.project_id)
        .join("seqbench");
    std::fs::create_dir_all(&out_dir).map_err(internal)?;
    let analysis_path = out_dir.join("analysis.json");
    let report_path = out_dir.join("report.md");
    std::fs::write(&analysis_path, &analysis_json).map_err(internal)?;
    std::fs::write(&report_path, report.as_bytes()).map_err(internal)?;
    let analysis_sha = xai_grok_science::seqbench::hex_sha256(&analysis_json);
    let report_sha = xai_grok_science::seqbench::hex_sha256(report.as_bytes());
    to_raw_response(&serde_json::json!({
        "projectId": params.project_id,
        "ownerId": params.owner_id,
        "sessionId": session_id.0,
        "sourcePath": source_path,
        "sourceSha256": analysis.source_sha256,
        "recordCount": records.len(),
        "analysisPath": analysis_path,
        "analysisSha256": analysis_sha,
        "reportPath": report_path,
        "reportSha256": report_sha,
        "tool": analysis.tool,
        "toolVersion": analysis.tool_version,
        "network": "disabled",
        "runtimeAuthority": "SessionActor-gated ACP adapter",
    }))
}

/// P5 product completion entry. This endpoint cannot supply a consultant
/// verdict, approval, or verification summary; it only asks the owning actor
/// to derive those facts from its current Goal/Expert state and durable store.
async fn handle_goal_host_verify(agent: &MvpAgent, args: &acp::ExtRequest) -> ExtResult {
    let params: GoalHostVerifyParams = parse_params(args)?;
    if params.session_id.is_empty() || params.run_id.is_empty() {
        return Err(acp::Error::invalid_params().data("sessionId and runId are required"));
    }
    let session_id = acp::SessionId::new(params.session_id);
    let handle = agent
        .get_session_handle(&session_id)
        .ok_or_else(|| acp::Error::invalid_params().data("session not found"))?;
    let workspace = std::fs::canonicalize(&handle.info.cwd).map_err(internal)?;
    let store_root = canonical_dir_within(params.store_root, &workspace)?;
    let result = agent
        .verify_science_goal(
            &session_id,
            ScienceStore::new(store_root),
            RunId::new(params.run_id),
        )
        .await
        .map_err(|error| {
            acp::Error::invalid_params()
                .data(format!("science host verification rejected: {error:?}"))
        })?;
    to_raw_response(&result)
}

/// Debug-only fixture connector. The public S3 policy continues to reject
/// loopback; the temporary ssh config maps this DNS-shaped test target to the
/// isolated local sshd only in debug builds used by product tests.
async fn handle_ssh_scp_fixture(agent: &MvpAgent, args: &acp::ExtRequest) -> ExtResult {
    if !cfg!(debug_assertions) {
        return Err(acp::Error::method_not_found());
    }
    let params: SshScpFixtureParams = parse_params(args)?;
    if params.project_id.is_empty() || params.owner_id.is_empty() || params.port == 0 {
        return Err(acp::Error::invalid_params().data("projectId, ownerId, and port are required"));
    }
    if !(1..=300_000).contains(&params.approval_timeout_ms)
        || !(1..=300_000).contains(&params.transport_timeout_ms)
    {
        return Err(acp::Error::invalid_params().data("timeouts must be in 1..=300000"));
    }
    let session_id = acp::SessionId::new(params.session_id);
    let handle = agent
        .get_session_handle(&session_id)
        .ok_or_else(|| acp::Error::invalid_params().data("session not found"))?;
    let workspace = std::fs::canonicalize(&handle.info.cwd).map_err(internal)?;
    let store_root = canonical_dir_within(params.store_root, &workspace)?;
    let artifact_root = canonical_dir_within(params.artifact_root, &workspace)?;
    let canonical_file = |path: PathBuf, label: &str| -> Result<PathBuf, acp::Error> {
        let path = std::fs::canonicalize(path).map_err(internal)?;
        if !path.starts_with(&workspace) || !path.is_file() {
            return Err(acp::Error::invalid_params()
                .data(format!("{label} must be a file inside session cwd")));
        }
        Ok(path)
    };
    let identity_file = canonical_file(params.identity_file, "identityFile")?;
    let known_hosts_file = canonical_file(params.known_hosts_file, "knownHostsFile")?;
    let ssh_config_file = canonical_file(params.ssh_config_file, "sshConfigFile")?;
    let local_path = match params.direction.as_str() {
        "put" => canonical_file(params.local_path, "localPath")?,
        "get" => {
            let parent = params
                .local_path
                .parent()
                .ok_or_else(|| acp::Error::invalid_params().data("localPath has no parent"))?;
            let parent = std::fs::canonicalize(parent).map_err(internal)?;
            if !parent.starts_with(&workspace) {
                return Err(
                    acp::Error::invalid_params().data("localPath must be inside session cwd")
                );
            }
            params.local_path
        }
        _ => return Err(acp::Error::invalid_params().data("direction must be put or get")),
    };
    let operation = match params.direction.as_str() {
        "put" => xai_grok_science::transport::ScpOperation::Put {
            local_source: local_path,
            remote_path: params.remote_path,
        },
        "get" => xai_grok_science::transport::ScpOperation::Get {
            remote_path: params.remote_path,
            local_destination: local_path,
        },
        _ => unreachable!(),
    };
    let host = "fixture.lumen.test".to_owned();
    let operation_sha256 = xai_grok_science::transport::operation_sha256(&operation);
    let policy = xai_grok_science::connector::ConnectorPolicy {
        project_id: ProjectId::new(params.project_id.clone()),
        owner_id: params.owner_id.clone(),
        targets: vec![xai_grok_science::connector::RemoteTarget {
            host: host.clone(),
            port: params.port,
            host_key_sha256: params.host_key_sha256.clone(),
            max_timeout_ms: params.transport_timeout_ms,
            allow_data_egress: true,
        }],
    };
    let request = xai_grok_science::connector::ConnectorRequest {
        host,
        port: params.port,
        host_key_sha256: params.host_key_sha256,
        timeout_ms: params.transport_timeout_ms,
        data_egress: true,
        operation_sha256: Some(operation_sha256),
    };
    let context = RunContext {
        run_id: RunId::new_v7(),
        project_id: ProjectId::new(params.project_id),
        session_id: session_id.0.to_string(),
        owner_id: params.owner_id,
        workspace_root: workspace,
        provider: "local-sshd-fixture".into(),
        approval_policy: "production-session-permission".into(),
        tool_profile: "science-ssh-scp-v1".into(),
        artifact_root,
        environment: BTreeMap::from([
            ("network".into(), "fixture-loopback-only".into()),
            ("locale".into(), "C".into()),
        ]),
    };
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    if let Some(delay_ms) = params.cancel_after_ms {
        if delay_ms == 0 {
            return Err(acp::Error::invalid_params().data("cancelAfterMs must be positive"));
        }
        let cancel_later = cancel.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(delay_ms));
            cancel_later.store(true, std::sync::atomic::Ordering::SeqCst);
        });
    }
    let config = xai_grok_science::transport::ScpExecutionConfig {
        identity_file,
        known_hosts_file,
        user: params.user,
        cancel,
        fixture_ssh_config: Some(ssh_config_file),
    };
    let result = agent
        .run_science_ssh_scp_transport(
            &session_id,
            ScienceStore::new(store_root),
            context,
            policy,
            request,
            operation,
            config,
            Duration::from_millis(params.approval_timeout_ms),
        )
        .await
        .map_err(internal)?;
    to_raw_response(&result)
}

/// S3 connector fetch entry: validates the connector, builds the protocol's
/// policy-gated request sequence, pairs each request with its offline
/// fixture, then drives the SessionActor begin/permission/finish protocol.
async fn handle_connector_fetch(agent: &MvpAgent, args: &acp::ExtRequest) -> ExtResult {
    let params: ConnectorFetchParams = parse_params(args)?;
    if params.project_id.is_empty() || params.owner_id.is_empty() {
        return Err(acp::Error::invalid_params().data("projectId and ownerId are required"));
    }
    if !(1..=300_000).contains(&params.approval_timeout_ms) {
        return Err(acp::Error::invalid_params().data("approvalTimeoutMs must be in 1..=300000"));
    }
    if params.query.is_empty() || !(1..=50).contains(&params.max_results) {
        return Err(
            acp::Error::invalid_params().data("query required; maxResults must be in 1..=50")
        );
    }
    let descriptor = xai_grok_science::connectors::descriptor(&params.connector_id)
        .ok_or_else(|| acp::Error::invalid_params().data("unknown connectorId"))?;
    let adapter = xai_grok_science::connectors::adapter::REGISTRY
        .get(descriptor.id)
        .ok_or_else(|| acp::Error::invalid_params().data("no protocol adapter for connector"))?;
    let expected = adapter.expected_exchanges();
    if params.fixture_paths.len() != expected {
        return Err(acp::Error::invalid_params().data(format!(
            "connector {} requires exactly {expected} fixture exchange(s)",
            descriptor.id
        )));
    }
    let session_id = acp::SessionId::new(params.session_id);
    let handle = agent
        .get_session_handle(&session_id)
        .ok_or_else(|| acp::Error::invalid_params().data("session not found"))?;
    let workspace = std::fs::canonicalize(&handle.info.cwd).map_err(internal)?;
    let store_root = canonical_dir_within(params.store_root, &workspace)?;
    let artifact_root = canonical_dir_within(params.artifact_root, &workspace)?;
    let mut fixture_bytes = Vec::with_capacity(expected);
    for path in &params.fixture_paths {
        let path = std::fs::canonicalize(path).map_err(internal)?;
        if !path.starts_with(&workspace) || !path.is_file() {
            return Err(
                acp::Error::invalid_params().data("fixturePaths must be files inside session cwd")
            );
        }
        let bytes = std::fs::read(&path).map_err(internal)?;
        if bytes.len() as u64 > xai_grok_science::preview::DEFAULT_MAX_BYTES {
            return Err(acp::Error::invalid_params().data("fixture exceeds the size cap"));
        }
        fixture_bytes.push(bytes);
    }
    // Build the protocol's policy-gated request sequence through the adapter.
    let paths = adapter
        .build_fixture_paths(&params.query, params.max_results, &fixture_bytes)
        .map_err(|error| acp::Error::invalid_params().data(error.to_string()))?;
    let mut requests = Vec::with_capacity(paths.len());
    for path in &paths {
        let req = xai_grok_science::connectors::validate_fixture_request(
            descriptor.id, path, 10_000,
        )
        .map_err(|error| acp::Error::invalid_params().data(error.to_string()))?;
        requests.push(req);
    }
    let context = RunContext {
        run_id: RunId::new_v7(),
        project_id: ProjectId::new(params.project_id),
        session_id: session_id.0.to_string(),
        owner_id: params.owner_id,
        workspace_root: workspace,
        provider: "offline-deterministic".into(),
        approval_policy: "production-session-permission".into(),
        tool_profile: "science-connector-v1".into(),
        artifact_root,
        environment: BTreeMap::from([
            ("network".into(), "disabled".into()),
            ("locale".into(), "C".into()),
        ]),
    };
    let result = agent
        .run_science_fetch(
            &session_id,
            ScienceStore::new(store_root),
            context,
            descriptor.id.to_owned(),
            params.query,
            requests,
            fixture_bytes,
            Duration::from_millis(params.approval_timeout_ms),
        )
        .await
        .map_err(internal)?;
    to_raw_response(&result)
}

/// S2 import entry: validates the source file inside the session workspace,
/// then drives the SessionActor begin/permission/finish protocol so the
/// artifact, structured preview, provenance, and evidence are all durable.
async fn handle_import_preview(agent: &MvpAgent, args: &acp::ExtRequest) -> ExtResult {
    let params: ImportPreviewParams = parse_params(args)?;
    if params.project_id.is_empty() || params.owner_id.is_empty() {
        return Err(acp::Error::invalid_params().data("projectId and ownerId are required"));
    }
    if !(1..=300_000).contains(&params.approval_timeout_ms) {
        return Err(acp::Error::invalid_params().data("approvalTimeoutMs must be in 1..=300000"));
    }
    let session_id = acp::SessionId::new(params.session_id);
    let handle = agent
        .get_session_handle(&session_id)
        .ok_or_else(|| acp::Error::invalid_params().data("session not found"))?;
    let workspace = std::fs::canonicalize(&handle.info.cwd).map_err(internal)?;
    let source_path = std::fs::canonicalize(&params.source_path).map_err(internal)?;
    if !source_path.starts_with(&workspace) || !source_path.is_file() {
        return Err(
            acp::Error::invalid_params().data("sourcePath must be a file inside session cwd")
        );
    }
    let store_root = canonical_dir_within(params.store_root, &workspace)?;
    let artifact_root = canonical_dir_within(params.artifact_root, &workspace)?;
    let bytes = std::fs::read(&source_path).map_err(internal)?;
    if bytes.len() as u64 > xai_grok_science::preview::DEFAULT_MAX_BYTES {
        return Err(acp::Error::invalid_params().data("sourcePath exceeds the preview size cap"));
    }
    let context = RunContext {
        run_id: RunId::new_v7(),
        project_id: ProjectId::new(params.project_id),
        session_id: session_id.0.to_string(),
        owner_id: params.owner_id,
        workspace_root: workspace,
        provider: "offline-deterministic".into(),
        approval_policy: "production-session-permission".into(),
        tool_profile: "science-import-v1".into(),
        artifact_root,
        environment: BTreeMap::from([
            ("network".into(), "disabled".into()),
            ("locale".into(), "C".into()),
        ]),
    };
    let result = agent
        .run_science_import(
            &session_id,
            ScienceStore::new(store_root),
            context,
            source_path,
            bytes,
            Duration::from_millis(params.approval_timeout_ms),
        )
        .await
        .map_err(internal)?;
    to_raw_response(&result)
}

async fn handle_run_csv(agent: &MvpAgent, args: &acp::ExtRequest) -> ExtResult {
    let params: RunCsvParams = parse_params(args)?;
    if params.project_id.is_empty() || params.owner_id.is_empty() {
        return Err(acp::Error::invalid_params().data("projectId and ownerId are required"));
    }
    if !(1..=300_000).contains(&params.approval_timeout_ms) {
        return Err(acp::Error::invalid_params().data("approvalTimeoutMs must be in 1..=300000"));
    }
    let session_id = acp::SessionId::new(params.session_id);
    let handle = agent
        .get_session_handle(&session_id)
        .ok_or_else(|| acp::Error::invalid_params().data("session not found"))?;
    let workspace = std::fs::canonicalize(&handle.info.cwd).map_err(internal)?;
    let fixture_path = std::fs::canonicalize(params.fixture_path).map_err(internal)?;
    if !fixture_path.starts_with(&workspace) || !fixture_path.is_file() {
        return Err(
            acp::Error::invalid_params().data("fixturePath must be a file inside session cwd")
        );
    }
    let store_root = canonical_dir_within(params.store_root, &workspace)?;
    let artifact_root = canonical_dir_within(params.artifact_root, &workspace)?;
    let fixture = std::fs::read(&fixture_path).map_err(internal)?;
    let context = RunContext {
        run_id: RunId::new_v7(),
        project_id: ProjectId::new(params.project_id),
        session_id: session_id.0.to_string(),
        owner_id: params.owner_id,
        workspace_root: workspace,
        provider: "offline-deterministic".into(),
        approval_policy: "production-session-permission".into(),
        tool_profile: "science-csv-v1".into(),
        artifact_root,
        environment: BTreeMap::from([
            ("network".into(), "disabled".into()),
            ("locale".into(), "C".into()),
        ]),
    };
    let result = agent
        .run_science_csv(
            &session_id,
            ScienceStore::new(store_root),
            context,
            fixture_path,
            fixture,
            Duration::from_millis(params.approval_timeout_ms),
        )
        .await
        .map_err(internal)?;
    to_raw_response(&result)
}

// ── WP-3 evidence query handlers ─────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EvidenceTraceParams { session_id: String, store_root: PathBuf, project_id: String, claim_id: String }

async fn handle_evidence_trace(agent: &MvpAgent, args: &acp::ExtRequest) -> ExtResult {
    let params: EvidenceTraceParams = parse_params(args)?;
    let session_id = acp::SessionId::new(params.session_id);
    let handle = agent.get_session_handle(&session_id)
        .ok_or_else(|| acp::Error::invalid_params().data("session not found"))?;
    let workspace = std::fs::canonicalize(&handle.info.cwd).map_err(internal)?;
    let store_root = canonical_dir_within(params.store_root, &workspace)?;
    let store = xai_grok_science::project::ProjectStore::new(store_root);
    let trace = store.trace_evidence(
        &xai_grok_science::project::ProjectId(params.project_id), &params.claim_id,
    ).map_err(internal)?;
    to_raw_response(&trace)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EvidenceCompareParams { session_id: String, store_root: PathBuf, project_id: String, claim_a: String, claim_b: String }

async fn handle_evidence_compare(agent: &MvpAgent, args: &acp::ExtRequest) -> ExtResult {
    let params: EvidenceCompareParams = parse_params(args)?;
    let session_id = acp::SessionId::new(params.session_id);
    let handle = agent.get_session_handle(&session_id)
        .ok_or_else(|| acp::Error::invalid_params().data("session not found"))?;
    let workspace = std::fs::canonicalize(&handle.info.cwd).map_err(internal)?;
    let store_root = canonical_dir_within(params.store_root, &workspace)?;
    let store = xai_grok_science::project::ProjectStore::new(store_root);
    let cmp = store.compare_claims(
        &xai_grok_science::project::ProjectId(params.project_id), &params.claim_a, &params.claim_b,
    ).map_err(internal)?;
    to_raw_response(&cmp)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EvidenceConsistencyParams { session_id: String, store_root: PathBuf, project_id: String }

async fn handle_evidence_consistency(agent: &MvpAgent, args: &acp::ExtRequest) -> ExtResult {
    let params: EvidenceConsistencyParams = parse_params(args)?;
    let session_id = acp::SessionId::new(params.session_id);
    let handle = agent.get_session_handle(&session_id)
        .ok_or_else(|| acp::Error::invalid_params().data("session not found"))?;
    let workspace = std::fs::canonicalize(&handle.info.cwd).map_err(internal)?;
    let store_root = canonical_dir_within(params.store_root, &workspace)?;
    let store = xai_grok_science::project::ProjectStore::new(store_root);
    let report = store.check_consistency(
        &xai_grok_science::project::ProjectId(params.project_id),
    ).map_err(internal)?;
    to_raw_response(&report)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EvidenceReproductionParams { session_id: String, store_root: PathBuf, project_id: String, claim_id: String }

async fn handle_evidence_reproduction(agent: &MvpAgent, args: &acp::ExtRequest) -> ExtResult {
    let params: EvidenceReproductionParams = parse_params(args)?;
    let session_id = acp::SessionId::new(params.session_id);
    let handle = agent.get_session_handle(&session_id)
        .ok_or_else(|| acp::Error::invalid_params().data("session not found"))?;
    let workspace = std::fs::canonicalize(&handle.info.cwd).map_err(internal)?;
    let store_root = canonical_dir_within(params.store_root, &workspace)?;
    let store = xai_grok_science::project::ProjectStore::new(store_root);
    let status = store.reproduction_status(
        &xai_grok_science::project::ProjectId(params.project_id), &params.claim_id,
    ).map_err(internal)?;
    to_raw_response(&status)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectMigrateParams { session_id: String, store_root: PathBuf, run_id: String, owner_id: String, title: String, question: String }

async fn handle_project_migrate(agent: &MvpAgent, args: &acp::ExtRequest) -> ExtResult {
    let params: ProjectMigrateParams = parse_params(args)?;
    let session_id = acp::SessionId::new(params.session_id);
    let handle = agent.get_session_handle(&session_id)
        .ok_or_else(|| acp::Error::invalid_params().data("session not found"))?;
    let workspace = std::fs::canonicalize(&handle.info.cwd).map_err(internal)?;
    let store_root = canonical_dir_within(params.store_root, &workspace)?;
    let store = xai_grok_science::project::ProjectStore::new(store_root);
    let result = store.migrate_v1_to_v2(params.run_id, params.owner_id, params.title, params.question).map_err(internal)?;
    to_raw_response(&result)
}

// ── WP-4/5/6/7/8 preview handlers ────────────────────────────────

async fn store_handler<T: serde::Serialize>(agent: &MvpAgent, session_id: &str, store_root: PathBuf, f: impl FnOnce(&xai_grok_science::project::ProjectStore) -> Result<T, xai_grok_science::ScienceError>) -> ExtResult {
    let sid = acp::SessionId::new(session_id.to_string());
    let handle = agent.get_session_handle(&sid)
        .ok_or_else(|| acp::Error::invalid_params().data("session not found"))?;
    let workspace = std::fs::canonicalize(&handle.info.cwd).map_err(internal)?;
    let sr = canonical_dir_within(store_root, &workspace)?;
    let store = xai_grok_science::project::ProjectStore::new(sr);
    let result = f(&store).map_err(internal)?;
    to_raw_response(&result)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WorkflowGenParams { session_id: String, store_root: PathBuf, #[serde(default)] project_id: String, #[serde(rename = "workflowSpec")] spec: serde_json::Value }

async fn handle_workflow_validate(agent: &MvpAgent, args: &acp::ExtRequest) -> ExtResult {
    let params: WorkflowGenParams = parse_params(args)?;
    let spec: xai_grok_science::workflow::WorkflowSpec = serde_json::from_value(params.spec).map_err(internal)?;
    store_handler(agent, &params.session_id, params.store_root, move |s| s.workflow_validate(&spec)).await
}

async fn handle_workflow_dry_run(agent: &MvpAgent, args: &acp::ExtRequest) -> ExtResult {
    let params: WorkflowGenParams = parse_params(args)?;
    let spec: xai_grok_science::workflow::WorkflowSpec = serde_json::from_value(params.spec).map_err(internal)?;
    store_handler(agent, &params.session_id, params.store_root, move |s| s.workflow_dry_run(&spec)).await
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct KernelAdmParams2 { session_id: String, store_root: PathBuf, kernel_id: String, #[serde(default = "_python_kind")] kind: String, exec_hash: String, lock_hash: String }
fn _python_kind() -> String { "python".into() }

async fn handle_kernel_admission(agent: &MvpAgent, args: &acp::ExtRequest) -> ExtResult {
    let params: KernelAdmParams2 = parse_params(args)?;
    let kind = match params.kind.as_str() { "r" | "R" => xai_grok_science::workflow::KernelKind::R, "julia" => xai_grok_science::workflow::KernelKind::Julia, _ => xai_grok_science::workflow::KernelKind::Python };
    store_handler(agent, &params.session_id, params.store_root, move |s| s.check_kernel_admission(params.kernel_id, kind, params.exec_hash, params.lock_hash)).await
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjRParams { session_id: String, store_root: PathBuf, project_id: String }

async fn handle_multimodal_index(agent: &MvpAgent, args: &acp::ExtRequest) -> ExtResult {
    let params: ProjRParams = parse_params(args)?;
    store_handler(agent, &params.session_id, params.store_root, move |s| s.multimodal_index(&xai_grok_science::project::ProjectId(params.project_id))).await
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReviewRecParams2 { session_id: String, store_root: PathBuf, project_id: String, reviewer_id: String, verdict: String, #[serde(default)] claim_id: Option<String> }

async fn handle_review_record(agent: &MvpAgent, args: &acp::ExtRequest) -> ExtResult {
    let params: ReviewRecParams2 = parse_params(args)?;
    store_handler(agent, &params.session_id, params.store_root, move |s| s.create_review_record(
        &xai_grok_science::project::ProjectId(params.project_id), params.reviewer_id, params.verdict, params.claim_id,
    )).await
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CollabInvParams2 { session_id: String, store_root: PathBuf, project_id: String, owner_id: String, invitee: String }

async fn handle_collaboration_invite(agent: &MvpAgent, args: &acp::ExtRequest) -> ExtResult {
    let params: CollabInvParams2 = parse_params(args)?;
    store_handler(agent, &params.session_id, params.store_root, move |s| s.collaboration_invite(
        &xai_grok_science::project::ProjectId(params.project_id), &params.owner_id, params.invitee,
    )).await
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RcpParams { session_id: String, store_root: PathBuf, project_id: String, hostname: String }

async fn handle_remote_compute_plan(agent: &MvpAgent, args: &acp::ExtRequest) -> ExtResult {
    let params: RcpParams = parse_params(args)?;
    store_handler(agent, &params.session_id, params.store_root, move |s| s.remote_compute_plan(
        &xai_grok_science::project::ProjectId(params.project_id), params.hostname,
    )).await
}
