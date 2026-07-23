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
        _ => Err(acp::Error::method_not_found()),
    }
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
    let expected = xai_grok_science::connectors::fetch::expected_exchanges(descriptor.id)
        .ok_or_else(|| acp::Error::invalid_params().data("connector has no v1 operation"))?;
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
    // Build the policy-gated request sequence. The pubmed esummary path
    // depends on the ids of the esearch exchange, parsed from the staged
    // fixture (the kernel re-parses everything at finish).
    let validate = |path: &str| {
        xai_grok_science::connectors::validate_request(descriptor.id, path, false, 10_000)
            .map_err(|error| acp::Error::invalid_params().data(error.to_string()))
    };
    let requests = match descriptor.id {
        "pubmed" => {
            let search = validate(&xai_grok_science::connectors::pubmed::esearch_path(
                &params.query,
                params.max_results,
                0,
            ))?;
            let (_total, ids) =
                xai_grok_science::connectors::pubmed::parse_esearch(&fixture_bytes[0])
                    .map_err(|error| acp::Error::invalid_params().data(error.to_string()))?;
            let summary = validate(&xai_grok_science::connectors::pubmed::esummary_path(&ids))?;
            vec![search, summary]
        }
        "chembl" => {
            vec![validate(
                &xai_grok_science::connectors::chembl::search_path(
                    &params.query,
                    params.max_results,
                    0,
                ),
            )?]
        }
        other => {
            return Err(acp::Error::invalid_params().data(format!("no v1 operation for {other}")));
        }
    };
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
