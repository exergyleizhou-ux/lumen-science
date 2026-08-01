//! Non-test ACP product entry for Lumen Science. Seam contract: S1, S2, S4.

use super::{ExtResult, parse_params, to_raw_response};
use crate::agent::MvpAgent;
use agent_client_protocol as acp;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::Duration,
};
use xai_grok_science::{
    ProjectId, RunContext, RunId, RunState, ScienceError, ScienceStore, ScienceWorkspaceCapability,
};

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

fn default_translation_table_id() -> u8 {
    1
}

fn internal(error: impl std::fmt::Display) -> acp::Error {
    acp::Error::internal_error().data(error.to_string())
}

#[cfg(unix)]
fn canonical_dir_within(path: PathBuf, workspace: &std::path::Path) -> Result<PathBuf, acp::Error> {
    canonical_dir_within_unix(path, workspace, |_| Ok(()))
}

/// Create a directory beneath a pinned workspace descriptor.
///
/// The callback is a deterministic test seam invoked after each child
/// descriptor is opened. Production supplies a no-op. Even if a pathname is
/// renamed and replaced by a symlink in that seam, subsequent operations stay
/// relative to the retained descriptor.
#[cfg(unix)]
fn canonical_dir_within_unix(
    path: PathBuf,
    workspace: &std::path::Path,
    mut after_component_open: impl FnMut(&std::path::Path) -> std::io::Result<()>,
) -> Result<PathBuf, acp::Error> {
    use std::{
        ffi::{CString, OsStr},
        fs::{File, OpenOptions},
        os::{
            fd::{AsRawFd as _, FromRawFd as _},
            unix::{
                ffi::OsStrExt as _,
                fs::{MetadataExt as _, OpenOptionsExt as _},
            },
        },
        path::Component,
    };

    fn invalid(message: impl Into<String>) -> acp::Error {
        acp::Error::invalid_params().data(message.into())
    }

    fn validate_components(path: &std::path::Path) -> Result<(), acp::Error> {
        if path
            .components()
            .any(|part| matches!(part, Component::CurDir | Component::ParentDir))
        {
            return Err(invalid("science path must contain no dot components"));
        }
        Ok(())
    }

    fn relative_to_workspace(
        path: &std::path::Path,
        workspace: &std::path::Path,
    ) -> Result<PathBuf, acp::Error> {
        if !path.is_absolute() {
            validate_components(path)?;
            return Ok(path.to_path_buf());
        }
        validate_components(path)?;
        if let Ok(relative) = path.strip_prefix(workspace) {
            return Ok(relative.to_path_buf());
        }

        // Preserve platform aliases such as macOS `/var` -> `/private/var`
        // without using that pathname for any write. The canonical existing
        // ancestor is used only to derive a descriptor-relative component
        // list; mkdir/open below starts again from the pinned workspace.
        let mut existing = path;
        while std::fs::symlink_metadata(existing)
            .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
        {
            existing = existing
                .parent()
                .ok_or_else(|| invalid("science path has no existing ancestor"))?;
        }
        let canonical_existing = dunce::canonicalize(existing).map_err(internal)?;
        let existing_relative = canonical_existing
            .strip_prefix(workspace)
            .map_err(|_| invalid("science path must be inside session cwd"))?;
        let unresolved = path
            .strip_prefix(existing)
            .map_err(|_| invalid("science path cannot be resolved beneath session cwd"))?;
        let relative = existing_relative.join(unresolved);
        validate_components(&relative)?;
        Ok(relative)
    }

    fn name(name: &OsStr) -> Result<CString, acp::Error> {
        CString::new(name.as_bytes()).map_err(|_| invalid("science path component contains NUL"))
    }

    fn open_child(directory: &File, child: &OsStr) -> Result<File, acp::Error> {
        let child = name(child)?;
        // SAFETY: the retained directory descriptor and NUL-terminated child
        // name are live for the duration of the call.
        let fd = unsafe {
            libc::openat(
                directory.as_raw_fd(),
                child.as_ptr(),
                libc::O_RDONLY
                    | libc::O_DIRECTORY
                    | libc::O_CLOEXEC
                    | libc::O_NOFOLLOW
                    | libc::O_NONBLOCK,
            )
        };
        if fd < 0 {
            return Err(internal(std::io::Error::last_os_error()));
        }
        // SAFETY: a nonnegative openat result transfers one owned descriptor.
        Ok(unsafe { File::from_raw_fd(fd) })
    }

    fn create_child(directory: &File, child: &OsStr) -> Result<(), acp::Error> {
        let child = name(child)?;
        // SAFETY: the retained directory descriptor and NUL-terminated child
        // name are live for the duration of the call.
        if unsafe { libc::mkdirat(directory.as_raw_fd(), child.as_ptr(), 0o700) } == 0 {
            return Ok(());
        }
        let error = std::io::Error::last_os_error();
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            Ok(())
        } else {
            Err(internal(error))
        }
    }

    #[cfg(target_os = "macos")]
    fn handle_path(directory: &File) -> Result<PathBuf, acp::Error> {
        use std::os::unix::ffi::OsStringExt as _;

        let mut buffer = vec![0_u8; libc::PATH_MAX as usize];
        // SAFETY: F_GETPATH writes at most PATH_MAX bytes into this writable
        // buffer for the live descriptor.
        if unsafe {
            libc::fcntl(
                directory.as_raw_fd(),
                libc::F_GETPATH,
                buffer.as_mut_ptr().cast::<libc::c_char>(),
            )
        } < 0
        {
            return Err(internal(std::io::Error::last_os_error()));
        }
        let end = buffer.iter().position(|byte| *byte == 0).ok_or_else(|| {
            internal(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "directory handle path was not NUL terminated",
            ))
        })?;
        Ok(PathBuf::from(std::ffi::OsString::from_vec(
            buffer[..end].to_vec(),
        )))
    }

    #[cfg(target_os = "linux")]
    fn handle_path(directory: &File) -> Result<PathBuf, acp::Error> {
        let link = std::fs::read_link(format!("/proc/self/fd/{}", directory.as_raw_fd()))
            .map_err(internal)?;
        if link.as_os_str().as_bytes().ends_with(b" (deleted)") {
            return Err(invalid(
                "science directory was unlinked during provisioning",
            ));
        }
        dunce::canonicalize(link).map_err(internal)
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    fn handle_path(_directory: &File) -> Result<PathBuf, acp::Error> {
        Err(acp::Error::internal_error()
            .data("secure science directory handle verification is unsupported on this Unix"))
    }

    fn same_directory(left: &File, right: &File) -> Result<bool, acp::Error> {
        let left = left.metadata().map_err(internal)?;
        let right = right.metadata().map_err(internal)?;
        Ok(left.is_dir()
            && right.is_dir()
            && left.dev() == right.dev()
            && left.ino() == right.ino())
    }

    validate_components(&path)?;
    let workspace = dunce::canonicalize(workspace).map_err(internal)?;
    if !workspace.is_absolute() || !workspace.is_dir() {
        return Err(invalid(
            "science workspace must resolve to an absolute directory",
        ));
    }
    let relative = relative_to_workspace(&path, &workspace)?;

    let mut options = OpenOptions::new();
    options
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK);
    let workspace_directory = options.open(&workspace).map_err(internal)?;
    let pinned_workspace = handle_path(&workspace_directory)?;
    if pinned_workspace != workspace {
        return Err(invalid(
            "science workspace identity changed during provisioning",
        ));
    }

    let mut current = workspace_directory;
    let mut opened_relative = PathBuf::new();
    for component in relative.components() {
        let Component::Normal(component) = component else {
            return Err(invalid(
                "science path must contain only normal relative components",
            ));
        };
        create_child(&current, component)?;
        let child = open_child(&current, component)?;
        if !child.metadata().map_err(internal)?.is_dir() {
            return Err(invalid("science path component is not a directory"));
        }
        current = child;
        opened_relative.push(component);
        after_component_open(&opened_relative).map_err(internal)?;
    }

    let final_path = handle_path(&current)?;
    if !final_path.starts_with(&pinned_workspace) {
        return Err(invalid(
            "science directory escaped the pinned session workspace",
        ));
    }
    let reopened = options.open(&final_path).map_err(internal)?;
    if !same_directory(&current, &reopened)? {
        return Err(invalid(
            "science directory identity changed during final verification",
        ));
    }
    Ok(final_path)
}

/// Non-Unix builds fail closed until directory-relative creation and
/// reparse-point/handle-identity verification are implemented for that
/// platform. Keeping the older `create_dir_all` flow here would preserve
/// function while knowingly retaining the same TOCTOU escape.
#[cfg(not(unix))]
fn canonical_dir_within(
    _path: PathBuf,
    _workspace: &std::path::Path,
) -> Result<PathBuf, acp::Error> {
    Err(acp::Error::internal_error()
        .data("secure science directory provisioning is not implemented on this platform"))
}

/// Resolve an existing directory inside the session workspace without writing.
///
/// Read-only ACP methods must not call `create_dir_all`: a rejected or empty
/// query is not authority to leave durable state behind.
fn canonical_existing_dir_within(path: PathBuf, workspace: &Path) -> Result<PathBuf, acp::Error> {
    use std::path::Component;

    if path
        .components()
        .any(|part| matches!(part, Component::CurDir | Component::ParentDir))
    {
        return Err(
            acp::Error::invalid_params().data("science path must contain no dot components")
        );
    }
    let workspace = dunce::canonicalize(workspace).map_err(internal)?;
    let path = if path.is_absolute() {
        path
    } else {
        workspace.join(path)
    };
    let canonical = dunce::canonicalize(path)
        .map_err(|error| acp::Error::invalid_params().data(error.to_string()))?;
    if !canonical.is_dir() || !canonical.starts_with(&workspace) {
        return Err(acp::Error::invalid_params()
            .data("science path must be an existing directory inside session cwd"));
    }
    Ok(canonical)
}

const MAX_SEQ_SOURCE_BYTES: u64 = 32 * 1024 * 1024;

#[cfg(test)]
mod canonical_dir_tests {
    use super::{canonical_dir_within, canonical_existing_dir_within};
    use std::path::PathBuf;

    #[test]
    fn directory_confinement_checks_before_creating() {
        let workspace = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let workspace = dunce::canonicalize(workspace.path()).unwrap();
        let outside = dunce::canonicalize(outside.path()).unwrap();

        let inside = workspace.join("science-store").join("runs");
        assert_eq!(
            canonical_dir_within(inside.clone(), &workspace).unwrap(),
            inside
        );
        assert!(inside.is_dir());

        let outside_target = outside.join("must-not-be-created").join("runs");
        assert!(canonical_dir_within(outside_target.clone(), &workspace).is_err());
        assert!(
            !outside_target.exists(),
            "rejected outside path was created before confinement failed"
        );

        let dotted = workspace.join("nested").join("..").join("escaped");
        assert!(canonical_dir_within(dotted.clone(), &workspace).is_err());
        assert!(
            !workspace.join("escaped").exists(),
            "dot-component path created state"
        );

        let relative = canonical_dir_within(PathBuf::from("relative-store"), &workspace).unwrap();
        assert_eq!(relative, workspace.join("relative-store"));
        assert!(relative.is_dir());
    }

    #[test]
    fn existing_directory_confinement_never_creates_for_a_read() {
        let workspace = tempfile::tempdir().unwrap();
        let workspace = dunce::canonicalize(workspace.path()).unwrap();
        let absent = workspace.join("absent-read-root");

        assert!(canonical_existing_dir_within(absent.clone(), &workspace).is_err());
        assert!(
            !absent.exists(),
            "a read-only confinement check created its target"
        );
    }

    #[cfg(unix)]
    #[test]
    fn directory_confinement_rejects_an_existing_symlink_escape() {
        let workspace = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let workspace = dunce::canonicalize(workspace.path()).unwrap();
        let outside = dunce::canonicalize(outside.path()).unwrap();
        let link = workspace.join("outside-link");
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        let target = link.join("must-not-be-created");

        assert!(canonical_dir_within(target.clone(), &workspace).is_err());
        assert!(!outside.join("must-not-be-created").exists());
    }

    #[cfg(unix)]
    #[test]
    fn directory_confinement_keeps_creation_on_pinned_ancestor_after_swap() {
        use super::canonical_dir_within_unix;

        let workspace = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let workspace = dunce::canonicalize(workspace.path()).unwrap();
        let outside = dunce::canonicalize(outside.path()).unwrap();
        let original = workspace.join("ancestor");
        let pinned = workspace.join("ancestor-pinned");
        std::fs::create_dir(&original).unwrap();

        let mut swapped = false;
        let resolved = canonical_dir_within_unix(
            original.join("created-after-swap"),
            &workspace,
            |opened_relative| {
                if !swapped && opened_relative == std::path::Path::new("ancestor") {
                    std::fs::rename(&original, &pinned)?;
                    std::os::unix::fs::symlink(&outside, &original)?;
                    swapped = true;
                }
                Ok(())
            },
        )
        .unwrap();

        assert!(swapped, "the deterministic ancestor-swap seam did not run");
        assert_eq!(resolved, pinned.join("created-after-swap"));
        assert!(resolved.is_dir());
        assert!(
            !outside.join("created-after-swap").exists(),
            "descriptor-relative mkdir escaped through the replacement symlink"
        );
    }
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

/// Admitted ecosystem capability entry (Biomni UniProt first).
///
/// Identity (session/project/owner) is session-bound, not taken from capability
/// input. Capability input may only carry the typed product fields after mapping.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CapabilityRunParams {
    session_id: String,
    project_id: String,
    owner_id: String,
    store_root: PathBuf,
    artifact_root: PathBuf,
    capability_id: String,
    /// Typed capability args (e.g. { prompt, maxResults } for Biomni UniProt).
    input: serde_json::Value,
    /// Main/CLI-owned offline response bytes. This is transport data, not a
    /// renderer-selectable filesystem capability.
    fixture_data_base64: Vec<String>,
    #[serde(default = "default_approval_timeout_ms")]
    approval_timeout_ms: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EvidenceDossierParams {
    session_id: String,
    project_id: String,
    owner_id: String,
    store_root: PathBuf,
    artifact_root: PathBuf,
    source_run_ids: Vec<String>,
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
        "x.ai/science/capability_run" => handle_capability_run(agent, args).await,
        "x.ai/science/evidence_dossier" => handle_evidence_dossier(agent, args).await,
        "x.ai/science/ssh_scp_fixture" => handle_ssh_scp_fixture(agent, args).await,
        "x.ai/science/goal_host_verify" => handle_goal_host_verify(agent, args).await,
        "x.ai/science/seq_analyze" => handle_seq_analyze(agent, args).await,
        "x.ai/science/skill_quarantine_import" => handle_skill_quarantine_import(agent, args).await,
        "x.ai/science/artifact_list" => handle_artifact_list(agent, args).await,
        "x.ai/science/project_create" => handle_project_create(agent, args).await,
        "x.ai/science/project_get" => handle_project_get(agent, args).await,
        "x.ai/science/project_assert_membership" => {
            handle_project_assert_membership(agent, args).await
        }
        "x.ai/science/project_list" => handle_project_list(agent, args).await,
        "x.ai/science/project_transition" => handle_project_transition(agent, args).await,
        "x.ai/science/project_update_question" => handle_project_update_question(agent, args).await,
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
        "x.ai/science/workflow_execute" => handle_workflow_execute(agent, args).await,
        "x.ai/science/kernel_admission" => handle_kernel_admission(agent, args).await,
        "x.ai/science/multimodal_index" => handle_multimodal_index(agent, args).await,
        "x.ai/science/review_record" => handle_review_record(agent, args).await,
        "x.ai/science/collaboration_invite" => handle_collaboration_invite(agent, args).await,
        "x.ai/science/remote_compute_plan" => handle_remote_compute_plan(agent, args).await,
        _ => Err(acp::Error::method_not_found()),
    }
}

// ── Durable artifact listing (AUTH-7) ───────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ArtifactListParams {
    session_id: String,
    owner_id: String,
    store_root: PathBuf,
    project_id: String,
    run_id: String,
}

#[derive(Debug, Serialize)]
struct ArtifactListItem {
    artifact_id: String,
    path: PathBuf,
    label: String,
    mime_type: String,
    bytes: u64,
    sha256: String,
    project_id: String,
    run_id: String,
    owner_id: String,
}

/// Reopen and verify every artifact before returning any preview capability.
///
/// This deliberately returns an all-or-nothing list. A single missing,
/// replaced, symlinked or hash-drifted artifact makes the whole query fail;
/// returning the remaining entries would let a partially corrupted run look
/// complete in the desktop.
fn verified_artifact_list(
    store: &ScienceStore,
    store_root: &Path,
    project_id: &ProjectId,
    run_id: &RunId,
    owner_id: &str,
    session_id: &str,
    workspace: &Path,
) -> Result<Vec<ArtifactListItem>, ScienceError> {
    let run = store.load_run(run_id)?;
    if run.context.project_id != *project_id
        || run.context.owner_id != owner_id
        || run.context.session_id != session_id
        || run.context.workspace_root != workspace
    {
        return Err(ScienceError::Ownership);
    }
    if run.state != RunState::Succeeded {
        return Err(ScienceError::Invalid(
            "artifacts are listable only for a succeeded run".into(),
        ));
    }

    let artifact_root = store_root.join("runs").join(&run_id.0).join("artifacts");
    let canonical_artifact_root = dunce::canonicalize(&artifact_root)?;
    if !canonical_artifact_root.starts_with(workspace) {
        return Err(ScienceError::Invalid(
            "artifact root resolved outside session workspace".into(),
        ));
    }

    let mut items = Vec::new();
    for artifact in store.artifacts(run_id)? {
        let bytes = store.artifact_bytes(project_id, run_id, owner_id, &artifact.relative_path)?;
        if bytes.len() as u64 != artifact.bytes
            || format!("{:x}", Sha256::digest(&bytes)) != artifact.sha256
        {
            return Err(ScienceError::Invalid(
                "registered science artifact hash or length mismatch".into(),
            ));
        }
        let path = dunce::canonicalize(canonical_artifact_root.join(&artifact.relative_path))?;
        if !path.starts_with(&canonical_artifact_root) {
            return Err(ScienceError::Invalid(
                "artifact resolved outside its run root".into(),
            ));
        }
        items.push(ArtifactListItem {
            artifact_id: artifact.sha256.clone(),
            path,
            label: artifact.relative_path.to_string_lossy().into_owned(),
            mime_type: artifact.mime,
            bytes: artifact.bytes,
            sha256: artifact.sha256,
            project_id: project_id.0.clone(),
            run_id: run_id.0.clone(),
            owner_id: owner_id.to_owned(),
        });
    }
    Ok(items)
}

#[cfg(test)]
mod artifact_list_tests {
    use super::verified_artifact_list;
    use std::{collections::BTreeMap, path::Path};
    use xai_grok_science::{CallId, ProjectId, RunContext, RunId, RunState, ScienceStore};

    fn context(workspace: &Path, run_id: &RunId, project_id: &ProjectId) -> RunContext {
        RunContext {
            run_id: run_id.clone(),
            project_id: project_id.clone(),
            session_id: "session-1".into(),
            owner_id: "owner-1".into(),
            workspace_root: workspace.to_path_buf(),
            provider: "offline-test".into(),
            approval_policy: "test".into(),
            tool_profile: "artifact-list-test".into(),
            artifact_root: workspace.join("science-store").join("runs"),
            environment: BTreeMap::new(),
        }
    }

    #[test]
    fn listing_is_verified_and_bound_to_run_identity() {
        let workspace = tempfile::tempdir().unwrap();
        let workspace = dunce::canonicalize(workspace.path()).unwrap();
        let store_root = workspace.join("science-store");
        let store = ScienceStore::new(&store_root);
        let run_id = RunId::new("run-1");
        let project_id = ProjectId::new("project-1");
        store
            .create_run(context(&workspace, &run_id, &project_id))
            .unwrap();
        let call_id = CallId::new("call-1");
        store
            .request_approval(xai_grok_science::Approval {
                project_id: project_id.clone(),
                run_id: run_id.clone(),
                call_id: call_id.clone(),
                owner_id: "owner-1".into(),
                decision: xai_grok_science::ApprovalDecision::Pending,
                decided_at: None,
            })
            .unwrap();
        store
            .transition(&run_id, RunState::AwaitingApproval, None)
            .unwrap();
        store
            .decide_approval(
                &project_id,
                &run_id,
                "owner-1",
                &call_id,
                xai_grok_science::ApprovalDecision::Allow,
            )
            .unwrap();
        store.transition(&run_id, RunState::Running, None).unwrap();
        let artifact = store
            .put_artifact(
                &project_id,
                &run_id,
                "owner-1",
                call_id,
                Path::new("report.md"),
                b"verified report\n",
                "text/markdown",
                "report",
            )
            .unwrap();
        store.transition_succeeded_verified(&run_id).unwrap();

        let listed = verified_artifact_list(
            &store,
            &store_root,
            &project_id,
            &run_id,
            "owner-1",
            "session-1",
            &workspace,
        )
        .unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].artifact_id, artifact.sha256);
        assert_eq!(listed[0].sha256, artifact.sha256);
        assert!(listed[0].path.starts_with(&workspace));

        for (project, owner, session, bound_workspace) in [
            (
                ProjectId::new("other-project"),
                "owner-1",
                "session-1",
                workspace.as_path(),
            ),
            (
                project_id.clone(),
                "other-owner",
                "session-1",
                workspace.as_path(),
            ),
            (
                project_id.clone(),
                "owner-1",
                "other-session",
                workspace.as_path(),
            ),
        ] {
            assert!(
                verified_artifact_list(
                    &store,
                    &store_root,
                    &project,
                    &run_id,
                    owner,
                    session,
                    bound_workspace,
                )
                .is_err(),
                "forged identity listed an artifact"
            );
        }

        let other_workspace = tempfile::tempdir().unwrap();
        let other_workspace = dunce::canonicalize(other_workspace.path()).unwrap();
        assert!(
            verified_artifact_list(
                &store,
                &store_root,
                &project_id,
                &run_id,
                "owner-1",
                "session-1",
                &other_workspace,
            )
            .is_err(),
            "forged workspace listed an artifact"
        );

        std::fs::write(
            store_root
                .join("runs")
                .join(&run_id.0)
                .join("artifacts")
                .join("report.md"),
            b"tampered report\n",
        )
        .unwrap();
        assert!(
            verified_artifact_list(
                &store,
                &store_root,
                &project_id,
                &run_id,
                "owner-1",
                "session-1",
                &workspace,
            )
            .is_err(),
            "hash-drifted bytes were returned"
        );
    }

    #[test]
    fn non_succeeded_runs_never_list_artifacts() {
        let workspace = tempfile::tempdir().unwrap();
        let workspace = dunce::canonicalize(workspace.path()).unwrap();
        let store_root = workspace.join("science-store");
        let store = ScienceStore::new(&store_root);
        let run_id = RunId::new("run-denied");
        let project_id = ProjectId::new("project-1");
        store
            .create_run(context(&workspace, &run_id, &project_id))
            .unwrap();
        let call_id = CallId::new("call-denied");
        store
            .request_approval(xai_grok_science::Approval {
                project_id: project_id.clone(),
                run_id: run_id.clone(),
                call_id: call_id.clone(),
                owner_id: "owner-1".into(),
                decision: xai_grok_science::ApprovalDecision::Pending,
                decided_at: None,
            })
            .unwrap();
        store
            .transition(&run_id, RunState::AwaitingApproval, None)
            .unwrap();
        store
            .decide_approval(
                &project_id,
                &run_id,
                "owner-1",
                &call_id,
                xai_grok_science::ApprovalDecision::Deny,
            )
            .unwrap();
        store
            .transition(&run_id, RunState::Denied, Some("refused".into()))
            .unwrap();

        assert!(
            verified_artifact_list(
                &store,
                &store_root,
                &project_id,
                &run_id,
                "owner-1",
                "session-1",
                &workspace,
            )
            .is_err()
        );
        assert!(store.artifacts(&run_id).unwrap().is_empty());
    }
}

async fn handle_artifact_list(agent: &MvpAgent, args: &acp::ExtRequest) -> ExtResult {
    let params: ArtifactListParams = parse_params(args)?;
    if params.owner_id.is_empty() || params.project_id.is_empty() || params.run_id.is_empty() {
        return Err(acp::Error::invalid_params().data("ownerId, projectId and runId are required"));
    }
    let session_id = acp::SessionId::new(params.session_id);
    let handle = agent
        .get_session_handle(&session_id)
        .ok_or_else(|| acp::Error::invalid_params().data("session not found"))?;
    handle
        .science_feature_gates
        .require(xai_grok_science::features::ScienceFeature::ResearchProject)
        .map_err(internal)?;
    let workspace = dunce::canonicalize(&handle.info.cwd).map_err(internal)?;
    let store_root = canonical_existing_dir_within(params.store_root, &workspace)?;
    let store = ScienceStore::new_confined(&store_root, &workspace).map_err(internal)?;
    let project_id = ProjectId::new(params.project_id);
    let run_id = RunId::new(params.run_id);
    let items = match store.load_run(&run_id) {
        Ok(_) => verified_artifact_list(
            &store,
            &store_root,
            &project_id,
            &run_id,
            &params.owner_id,
            session_id.0.as_ref(),
            &workspace,
        ),
        // A newly-created project has no runs yet. Return the honest empty
        // list only after checking its durable owner record; otherwise a
        // missing run would become a project-existence or ownership probe.
        Err(ScienceError::Io(ref error)) if error.kind() == std::io::ErrorKind::NotFound => {
            let project_store =
                xai_grok_science::project::ProjectStore::new_confined(&store_root, &workspace)
                    .map_err(internal)?
                    .with_gates(handle.science_feature_gates.clone());
            match project_store.assert_project_owner(
                &xai_grok_science::project::ProjectId(project_id.0.clone()),
                &params.owner_id,
            ) {
                // `default` is the desktop catalog's explicit no-run sentinel.
                // The durable project aggregate does not mint a default run,
                // so this exception must remain narrower than "any missing
                // run": an arbitrary absent run id is not evidence that the
                // caller is opening a newly-created project.
                Ok(()) if run_id.0 == "default" => Ok(Vec::new()),
                _ => Err(ScienceError::Ownership),
            }
        }
        Err(error) => Err(error),
    }
    .map_err(internal)?;
    to_raw_response(&items)
}

// ── WP-2 product path: ResearchProject + EvidenceGraph + Claims ──
//
// Every mutating entry here routes through the SessionActor
// (`MvpAgent::run_science_project_mutation`), which owns the permission
// request, the durable run record, and the record write. None of them may
// construct a ProjectStore and mutate it on this request task: that would put
// execution authority in the ACP adapter, which is exactly the seam this
// product path exists to keep closed. Read-only entries below still build a
// store directly, which is fine — they take no authority.

/// Shared driver for the four mutating WP-2 entries.
///
/// Resolves the session, pins the store roots inside its workspace, builds the
/// run context, and hands the typed mutation to the actor. `operationId` is
/// the caller's idempotency key: replaying one returns the first outcome
/// instead of applying the mutation twice. `expectedRevision` is a
/// compare-and-swap against `ProjectStore::project_revision`; omit it only
/// when the caller accepts last-writer-wins.
async fn run_project_mutation(
    agent: &MvpAgent,
    session_id: String,
    owner_id: String,
    store_root: PathBuf,
    artifact_root: Option<PathBuf>,
    operation_id: String,
    expected_revision: Option<String>,
    approval_timeout_ms: u64,
    mut mutation: xai_grok_science::project::ProjectMutation,
) -> Result<xai_grok_science::project::MutationOutcome, acp::Error> {
    if owner_id.is_empty() {
        return Err(acp::Error::invalid_params().data("ownerId is required"));
    }
    if !(1..=300_000).contains(&approval_timeout_ms) {
        return Err(acp::Error::invalid_params().data("approvalTimeoutMs must be in 1..=300000"));
    }
    let session_id = acp::SessionId::new(session_id);
    let handle = agent
        .get_session_handle(&session_id)
        .ok_or_else(|| acp::Error::invalid_params().data("session not found"))?;
    // Fail a disabled mutation before path provisioning creates even an empty
    // store/run directory. The SessionActor repeats this check as the sole
    // execution authority; this adapter-side preflight only keeps rejected
    // requests observationally read-only.
    handle
        .science_feature_gates
        .require_all(mutation.required_features())
        .map_err(internal)?;
    let workspace = dunce::canonicalize(&handle.info.cwd).map_err(internal)?;
    // A migration source is required to exist before the actor can capture
    // and admit it. Resolve that store read-only so an absent source cannot
    // leave even an empty store/runs tree behind. The SessionActor still
    // repeats the authoritative source, ownership and byte-boundary checks.
    let is_migration = matches!(
        mutation,
        xai_grok_science::project::ProjectMutation::ProjectMigrate { .. }
    );
    let project_root = if is_migration {
        canonical_existing_dir_within(store_root, &workspace)?
    } else {
        canonical_dir_within(store_root, &workspace)?
    };
    let run_root = if is_migration {
        canonical_existing_dir_within(project_root.join("runs"), &workspace)?
    } else {
        canonical_dir_within(project_root.join("runs"), &workspace)?
    };
    if let Some(root) = artifact_root {
        use std::path::Component;
        if root
            .components()
            .any(|part| matches!(part, Component::CurDir | Component::ParentDir))
        {
            return Err(acp::Error::invalid_params()
                .data("artifactRoot for a project mutation must contain no dot components"));
        }
        let requested = if root.is_absolute() {
            root
        } else {
            workspace.join(root)
        };
        if requested != run_root {
            return Err(acp::Error::invalid_params()
                .data("artifactRoot for a project mutation must equal storeRoot/runs"));
        }
    }
    let mut authority_run_id = RunId::new_v7();
    match &mut mutation {
        xai_grok_science::project::ProjectMutation::ReviewRecord {
            authority_run_id: bound_run_id,
            ..
        }
        | xai_grok_science::project::ProjectMutation::ProjectMigrate {
            authority_run_id: bound_run_id,
            ..
        } => *bound_run_id = authority_run_id.0.clone(),
        _ => {}
    }
    let mut request = xai_grok_science::project::MutationRequest {
        operation_id,
        session_id: session_id.0.to_string(),
        owner_id,
        expected_revision,
        mutation,
    };
    if matches!(
        request.mutation,
        xai_grok_science::project::ProjectMutation::ReviewRecord { .. }
    ) {
        // A retry before the review ledger exists must reopen one authority
        // run. The normalized request fingerprint excludes the actor-minted
        // authority id, so compute it first and only then bind the mutation.
        authority_run_id = RunId::new(format!(
            "review-authority-{}",
            request.replay_fingerprint().map_err(internal)?
        ));
        let xai_grok_science::project::ProjectMutation::ReviewRecord {
            authority_run_id: bound_run_id,
            ..
        } = &mut request.mutation
        else {
            unreachable!("review variant checked above");
        };
        *bound_run_id = authority_run_id.0.clone();
    }
    if matches!(
        request.mutation,
        xai_grok_science::project::ProjectMutation::ProjectMigrate { .. }
    ) {
        // A process stop before the project journal exists must not orphan a
        // random Running+Allow authority and mint a second one on retry.
        // The normalized request fingerprint excludes the authority id, so
        // every retry deterministically reopens the same durable run.
        authority_run_id = RunId::new(format!(
            "migration-authority-{}",
            request.replay_fingerprint().map_err(internal)?
        ));
        let xai_grok_science::project::ProjectMutation::ProjectMigrate {
            authority_run_id: bound_run_id,
            ..
        } = &mut request.mutation
        else {
            unreachable!("migration variant checked above");
        };
        *bound_run_id = authority_run_id.0.clone();
    }
    // A migration owns artifacts in its newly-created project, so reserve its
    // deterministic target before the durable authority run is opened.
    // Other creates remain filed under their operation until a project id is
    // minted by the store.
    let run_project = request
        .migration_target_project_id()
        .map_err(internal)?
        .or_else(|| request.mutation.target_project().cloned())
        .map(|project_id| project_id.0)
        .unwrap_or_else(|| format!("pending-{}", request.operation_id));
    let context = RunContext {
        run_id: authority_run_id,
        project_id: ProjectId::new(run_project),
        session_id: session_id.0.to_string(),
        owner_id: request.owner_id.clone(),
        workspace_root: workspace,
        provider: "offline-deterministic".into(),
        approval_policy: "production-session-permission".into(),
        tool_profile: "science-project-mutation-v1".into(),
        artifact_root: run_root,
        environment: BTreeMap::from([
            ("network".into(), "disabled".into()),
            ("locale".into(), "C".into()),
        ]),
    };
    agent
        .run_science_project_mutation(
            &session_id,
            ScienceStore::new_confined(&project_root, &context.workspace_root).map_err(internal)?,
            project_root,
            context,
            request,
            Duration::from_millis(approval_timeout_ms),
        )
        .await
        .map_err(internal)
}

fn mutation_response(outcome: xai_grok_science::project::MutationOutcome) -> ExtResult {
    to_raw_response(&serde_json::json!({
        "operationId": outcome.operation_id,
        "kind": outcome.kind,
        "projectId": outcome.project_id.0,
        "revision": outcome.revision,
        "replayed": outcome.replayed,
        "result": outcome.result,
        "runtimeAuthority": "SessionActor-gated ACP adapter",
    }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectCreateParams {
    session_id: String,
    owner_id: String,
    store_root: PathBuf,
    title: String,
    research_question: String,
    operation_id: String,
    #[serde(default)]
    artifact_root: Option<PathBuf>,
    #[serde(default = "default_approval_timeout_ms")]
    approval_timeout_ms: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct QuestionUpdateParams {
    session_id: String,
    owner_id: String,
    store_root: PathBuf,
    project_id: String,
    research_question: String,
    operation_id: String,
    #[serde(default)]
    expected_revision: Option<String>,
    #[serde(default)]
    artifact_root: Option<PathBuf>,
    #[serde(default = "default_approval_timeout_ms")]
    approval_timeout_ms: u64,
}

/// Refine an existing project's research question.
///
/// Same SessionActor route as every other record mutation — permission prompt,
/// idempotent operation id, optional revision CAS. The alternative was the
/// desktop keeping edits in component state, where the question a user spent
/// an hour refining vanished on tab switch and the durable record silently
/// disagreed with the screen.
async fn handle_project_update_question(agent: &MvpAgent, args: &acp::ExtRequest) -> ExtResult {
    let params: QuestionUpdateParams = parse_params(args)?;
    if params.research_question.is_empty() {
        return Err(acp::Error::invalid_params().data("researchQuestion is required"));
    }
    let outcome = run_project_mutation(
        agent,
        params.session_id,
        params.owner_id,
        params.store_root,
        params.artifact_root,
        params.operation_id,
        params.expected_revision,
        params.approval_timeout_ms,
        xai_grok_science::project::ProjectMutation::QuestionUpdate {
            project_id: xai_grok_science::project::ProjectId(params.project_id),
            research_question: params.research_question,
        },
    )
    .await?;
    mutation_response(outcome)
}

async fn handle_project_create(agent: &MvpAgent, args: &acp::ExtRequest) -> ExtResult {
    let params: ProjectCreateParams = parse_params(args)?;
    if params.title.is_empty() {
        return Err(acp::Error::invalid_params().data("title is required"));
    }
    let outcome = run_project_mutation(
        agent,
        params.session_id,
        params.owner_id,
        params.store_root,
        params.artifact_root,
        params.operation_id,
        // A create has no prior revision to compare against.
        None,
        params.approval_timeout_ms,
        xai_grok_science::project::ProjectMutation::ProjectCreate {
            title: params.title,
            research_question: params.research_question,
        },
    )
    .await?;
    mutation_response(outcome)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectGetParams {
    session_id: String,
    store_root: PathBuf,
    project_id: String,
    owner_id: String,
}

async fn handle_project_get(agent: &MvpAgent, args: &acp::ExtRequest) -> ExtResult {
    let params: ProjectGetParams = parse_params(args)?;
    if params.owner_id.is_empty() {
        return Err(acp::Error::invalid_params().data("ownerId is required"));
    }
    let session_id = acp::SessionId::new(params.session_id);
    let handle = agent
        .get_session_handle(&session_id)
        .ok_or_else(|| acp::Error::invalid_params().data("session not found"))?;
    let workspace = dunce::canonicalize(&handle.info.cwd).map_err(internal)?;
    let store_root = canonical_dir_within(params.store_root, &workspace)?;
    let store = xai_grok_science::project::ProjectStore::new_confined(&store_root, &workspace)
        .map_err(internal)?
        .with_gates(handle.science_feature_gates.clone());
    let pid = xai_grok_science::project::ProjectId(params.project_id);
    let bundle = store
        .load_bundle_for_owner(&pid, &params.owner_id)
        .map_err(internal)?;
    to_raw_response(&bundle)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectAssertMembershipParams {
    session_id: String,
    store_root: PathBuf,
    project_id: String,
    owner_id: String,
}

/// Answer whether an owner may act on a project.
///
/// The desktop has always called `project_assert_membership`; it existed in
/// neither engine, so the method registry refused it and every attempt to open
/// a project failed closed. That was the correct failure — the desktop had
/// invented the name — but it left the entire workspace unreachable.
///
/// Read-only on purpose, so it stays a plain query rather than taking the
/// SessionActor route a mutation needs. It answers from the durable record: the
/// project's stored owner is compared to the claimed one, and no other source
/// participates.
///
/// A missing project is reported as NOT a member rather than as an error. A
/// caller learning "this project does not exist" versus "you are not its owner"
/// is a probe for which projects exist, and the honest answer to both is the
/// same: you may not act on it.
async fn handle_project_assert_membership(agent: &MvpAgent, args: &acp::ExtRequest) -> ExtResult {
    let params: ProjectAssertMembershipParams = parse_params(args)?;
    if params.owner_id.is_empty() {
        return Err(acp::Error::invalid_params().data("ownerId is required"));
    }
    let session_id = acp::SessionId::new(params.session_id);
    let handle = agent
        .get_session_handle(&session_id)
        .ok_or_else(|| acp::Error::invalid_params().data("session not found"))?;
    let workspace = dunce::canonicalize(&handle.info.cwd).map_err(internal)?;
    let store_root = canonical_dir_within(params.store_root, &workspace)?;
    let store = xai_grok_science::project::ProjectStore::new_confined(&store_root, &workspace)
        .map_err(internal)?
        .with_gates(handle.science_feature_gates.clone());
    let pid = xai_grok_science::project::ProjectId(params.project_id.clone());

    let member = store.assert_project_owner(&pid, &params.owner_id).is_ok();

    to_raw_response(&serde_json::json!({
        "ok": member,
        "member": member,
        "ownerId": params.owner_id,
        "projectId": params.project_id,
        "reason": if member { "owner matches the durable project record" }
                  else { "not the owner of this project, or no such project" },
        "runtimeAuthority": "SessionActor-gated ACP adapter",
    }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectListParams {
    session_id: String,
    store_root: PathBuf,
    owner_id: String,
}

async fn handle_project_list(agent: &MvpAgent, args: &acp::ExtRequest) -> ExtResult {
    let params: ProjectListParams = parse_params(args)?;
    if params.owner_id.is_empty() {
        return Err(acp::Error::invalid_params().data("ownerId is required"));
    }
    let session_id = acp::SessionId::new(params.session_id);
    let handle = agent
        .get_session_handle(&session_id)
        .ok_or_else(|| acp::Error::invalid_params().data("session not found"))?;
    let workspace = dunce::canonicalize(&handle.info.cwd).map_err(internal)?;
    let store_root = canonical_dir_within(params.store_root, &workspace)?;
    let store = xai_grok_science::project::ProjectStore::new_confined(&store_root, &workspace)
        .map_err(internal)?
        .with_gates(handle.science_feature_gates.clone());
    let projects = store
        .list_projects_for_owner(&params.owner_id)
        .map_err(internal)?;
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
    operation_id: String,
    #[serde(default)]
    expected_revision: Option<String>,
    #[serde(default)]
    artifact_root: Option<PathBuf>,
    #[serde(default = "default_approval_timeout_ms")]
    approval_timeout_ms: u64,
}

async fn handle_project_transition(agent: &MvpAgent, args: &acp::ExtRequest) -> ExtResult {
    let params: ProjectTransitionParams = parse_params(args)?;
    let status = parse_project_status(&params.status)
        .ok_or_else(|| acp::Error::invalid_params().data("invalid status"))?;
    let outcome = run_project_mutation(
        agent,
        params.session_id,
        params.owner_id,
        params.store_root,
        params.artifact_root,
        params.operation_id,
        params.expected_revision,
        params.approval_timeout_ms,
        xai_grok_science::project::ProjectMutation::ProjectTransition {
            project_id: xai_grok_science::project::ProjectId(params.project_id),
            status,
        },
    )
    .await?;
    mutation_response(outcome)
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
    operation_id: String,
    #[serde(default)]
    expected_revision: Option<String>,
    #[serde(default)]
    artifact_root: Option<PathBuf>,
    #[serde(default = "default_approval_timeout_ms")]
    approval_timeout_ms: u64,
}

async fn handle_claim_propose(agent: &MvpAgent, args: &acp::ExtRequest) -> ExtResult {
    let params: ClaimProposeParams = parse_params(args)?;
    if params.statement.is_empty() {
        return Err(acp::Error::invalid_params().data("statement is required"));
    }
    let outcome = run_project_mutation(
        agent,
        params.session_id,
        params.owner_id,
        params.store_root,
        params.artifact_root,
        params.operation_id,
        params.expected_revision,
        params.approval_timeout_ms,
        xai_grok_science::project::ProjectMutation::ClaimPropose {
            project_id: xai_grok_science::project::ProjectId(params.project_id),
            statement: params.statement,
            proposed_by: params.proposed_by,
        },
    )
    .await?;
    mutation_response(outcome)
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
    operation_id: String,
    #[serde(default)]
    expected_revision: Option<String>,
    #[serde(default)]
    artifact_root: Option<PathBuf>,
    #[serde(default = "default_approval_timeout_ms")]
    approval_timeout_ms: u64,
}

async fn handle_evidence_attach(agent: &MvpAgent, args: &acp::ExtRequest) -> ExtResult {
    let params: EvidenceAttachParams = parse_params(args)?;
    let outcome = run_project_mutation(
        agent,
        params.session_id,
        params.owner_id,
        params.store_root,
        params.artifact_root,
        params.operation_id,
        params.expected_revision,
        params.approval_timeout_ms,
        xai_grok_science::project::ProjectMutation::EvidenceAttach {
            project_id: xai_grok_science::project::ProjectId(params.project_id),
            claim_id: params.claim_id,
            artifact_sha256: params.artifact_sha256,
            label: params.label,
            run_id: params.run_id,
        },
    )
    .await?;
    mutation_response(outcome)
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
    operation_id: String,
    source_path: PathBuf,
    #[serde(default = "default_translation_table_id")]
    translation_table_id: u8,
    #[serde(default)]
    topology: xai_grok_science::seqbench::SequenceTopology,
    #[serde(default)]
    restriction_digest_enzymes: Vec<String>,
    #[serde(default)]
    primer_candidates: Vec<String>,
    #[serde(default = "default_approval_timeout_ms")]
    approval_timeout_ms: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SkillQuarantineItemParams {
    sub_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SkillQuarantineImportParams {
    session_id: String,
    project_id: String,
    owner_id: String,
    store_root: PathBuf,
    operation_id: String,
    archive_base64: String,
    archive_sha256: String,
    archive_bytes: u64,
    items: Vec<SkillQuarantineItemParams>,
    #[serde(default = "default_approval_timeout_ms")]
    approval_timeout_ms: u64,
}

async fn handle_seq_analyze(agent: &MvpAgent, args: &acp::ExtRequest) -> ExtResult {
    let params: SeqAnalyzeParams = parse_params(args)?;
    if params.project_id.is_empty() || params.owner_id.is_empty() {
        return Err(acp::Error::invalid_params().data("projectId and ownerId are required"));
    }
    xai_grok_science::seqbench::validate_operation_id(&params.operation_id)
        .map_err(|error| acp::Error::invalid_params().data(error.to_string()))?;
    if params.artifact_root.as_os_str() != std::ffi::OsStr::new("science-store") {
        return Err(acp::Error::invalid_params()
            .data("artifactRoot must be the workspace-relative science-store authority root"));
    }
    if !(1..=300_000).contains(&params.approval_timeout_ms) {
        return Err(acp::Error::invalid_params().data("approvalTimeoutMs must be in 1..=300000"));
    }
    if !xai_grok_science::seqbench::is_supported_translation_table(params.translation_table_id) {
        return Err(acp::Error::invalid_params().data(format!(
            "translationTableId must be one of {}",
            xai_grok_science::seqbench::SUPPORTED_TRANSLATION_TABLE_IDS
                .iter()
                .map(u8::to_string)
                .collect::<Vec<_>>()
                .join(",")
        )));
    }
    let restriction_digest_enzymes =
        xai_grok_science::seqbench::canonical_restriction_digest_enzymes(
            &params.restriction_digest_enzymes,
        )
        .map_err(|error| acp::Error::invalid_params().data(error))?;
    let primer_candidates =
        xai_grok_science::primer_thermo::canonical_primer_candidates(&params.primer_candidates)
            .map_err(|error| acp::Error::invalid_params().data(error))?;
    let session_id = acp::SessionId::new(params.session_id);
    let handle = agent
        .get_session_handle(&session_id)
        .ok_or_else(|| acp::Error::invalid_params().data("session not found"))?;
    let workspace_capability =
        ScienceWorkspaceCapability::open(&handle.info.cwd).map_err(internal)?;
    let (source_path, bytes) = workspace_capability
        .snapshot_regular_bounded(&params.source_path, MAX_SEQ_SOURCE_BYTES)
        .map_err(internal)?;
    let store = workspace_capability
        .create_science_store(params.artifact_root)
        .map_err(internal)?;
    let workspace = workspace_capability.current_path().map_err(internal)?;
    let artifact_root = store.root().to_path_buf();
    let source_relative =
        xai_grok_science::seqbench::source_relative_binding(&workspace, &source_path)
            .map_err(internal)?;
    let options = xai_grok_science::seqbench::SeqAnalyzeOptions {
        translation_table_id: params.translation_table_id,
        topology: params.topology,
        restriction_digest_enzymes,
        primer_candidates,
    };
    let request_sha256 =
        xai_grok_science::seqbench::request_sha256(&source_relative, &bytes, &options)
            .map_err(internal)?;
    let source_sha256 = xai_grok_science::seqbench::hex_sha256(&bytes);
    let context = RunContext {
        run_id: xai_grok_science::seqbench::operation_run_id(&params.operation_id),
        project_id: ProjectId::new(params.project_id),
        session_id: session_id.0.to_string(),
        owner_id: params.owner_id,
        workspace_root: workspace,
        provider: "offline-deterministic".into(),
        approval_policy: "production-session-permission".into(),
        tool_profile: "science-seqbench-v4".into(),
        artifact_root: artifact_root.clone(),
        environment: BTreeMap::from([
            ("network".into(), "disabled".into()),
            ("locale".into(), "C".into()),
            (
                xai_grok_science::seqbench::OPERATION_ENV.into(),
                params.operation_id.clone(),
            ),
            (
                xai_grok_science::seqbench::REQUEST_SHA256_ENV.into(),
                request_sha256,
            ),
            (
                xai_grok_science::seqbench::SOURCE_SHA256_ENV.into(),
                source_sha256,
            ),
            (
                xai_grok_science::seqbench::SOURCE_BYTES_ENV.into(),
                bytes.len().to_string(),
            ),
            (
                xai_grok_science::seqbench::SOURCE_RELATIVE_PATH_ENV.into(),
                source_relative,
            ),
            (
                "translation_table_id".into(),
                params.translation_table_id.to_string(),
            ),
            (
                "restriction_topology".into(),
                params.topology.as_str().into(),
            ),
            (
                "restriction_digest_enzymes".into(),
                options.restriction_digest_enzymes.join(","),
            ),
            (
                xai_grok_science::seqbench::PRIMER_CANDIDATES_ENV.into(),
                options.primer_candidates.join(","),
            ),
        ]),
    };
    let result = agent
        .run_science_seq_analyze(
            &session_id,
            store,
            context,
            options,
            source_path,
            bytes,
            Duration::from_millis(params.approval_timeout_ms),
        )
        .await
        .map_err(internal)?;
    drop(workspace_capability);
    to_raw_response(&serde_json::json!({
        "run": result.run,
        "analysis": result.analysis,
        "artifacts": result.artifacts,
        "evidence": result.evidence,
        "provenance": result.provenance,
        "approvals": result.approvals,
        "recordCount": result.records,
        "replayAfter": result.replay_after,
        "operationId": params.operation_id,
        "replayed": result.replayed,
        "runtimeAuthority": "SessionActor-gated ACP adapter",
        "network": "disabled",
    }))
}

/// Import an uploaded ZIP/.skill into the ScienceStore quarantine.
///
/// The renderer cannot provide a path. Desktop main forwards canonical,
/// bounded base64 through ACP; this adapter independently decodes and hashes it
/// in memory before delegating all durable authority to SessionActor. No loose
/// archive payload is written before Allow.
async fn handle_skill_quarantine_import(agent: &MvpAgent, args: &acp::ExtRequest) -> ExtResult {
    let params: SkillQuarantineImportParams = parse_params(args)?;
    if params.project_id.is_empty() || params.owner_id.is_empty() {
        return Err(acp::Error::invalid_params().data("projectId and ownerId are required"));
    }
    if params.store_root != Path::new("science-store") {
        return Err(
            acp::Error::invalid_params().data("storeRoot must be the fixed science-store name")
        );
    }
    if !(1..=300_000).contains(&params.approval_timeout_ms) {
        return Err(acp::Error::invalid_params().data("approvalTimeoutMs must be in 1..=300000"));
    }
    if params.archive_sha256.len() != 64
        || !params
            .archive_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(acp::Error::invalid_params().data("archiveSha256 must be lowercase SHA-256"));
    }
    let limits = xai_grok_science::skill_quarantine::SkillArchiveLimits::default();
    if params.archive_bytes == 0 || params.archive_bytes > limits.max_archive_bytes as u64 {
        return Err(acp::Error::invalid_params().data(format!(
            "archiveBytes must be in 1..={}",
            limits.max_archive_bytes
        )));
    }
    let max_encoded = limits.max_archive_bytes.div_ceil(3).saturating_mul(4);
    if params.archive_base64.is_empty() || params.archive_base64.len() > max_encoded {
        return Err(acp::Error::invalid_params()
            .data("archiveBase64 is empty or exceeds the bounded ACP payload cap"));
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&params.archive_base64)
        .map_err(|_| acp::Error::invalid_params().data("archiveBase64 is malformed"))?;
    if bytes.len() as u64 != params.archive_bytes
        || bytes.len() > limits.max_archive_bytes
        || base64::engine::general_purpose::STANDARD.encode(&bytes) != params.archive_base64
        || format!("{:x}", Sha256::digest(&bytes)) != params.archive_sha256
    {
        return Err(acp::Error::invalid_params()
            .data("archiveBase64 is non-canonical or does not match its size and digest"));
    }
    let session_id = acp::SessionId::new(params.session_id);
    let handle = agent
        .get_session_handle(&session_id)
        .ok_or_else(|| acp::Error::invalid_params().data("session not found"))?;
    let workspace = dunce::canonicalize(&handle.info.cwd).map_err(internal)?;
    let artifact_root = canonical_dir_within(workspace.join(&params.store_root), &workspace)?;

    let project_id = ProjectId::new(params.project_id);
    let run_id = xai_grok_science::skill_quarantine::operation_run_id(
        &params.owner_id,
        &project_id,
        session_id.0.as_ref(),
        &params.operation_id,
    );
    let context = RunContext {
        run_id,
        project_id,
        session_id: session_id.0.to_string(),
        owner_id: params.owner_id,
        workspace_root: workspace,
        provider: "offline-deterministic".into(),
        approval_policy: "production-session-permission".into(),
        tool_profile: "science-skill-quarantine-v1".into(),
        artifact_root: artifact_root.clone(),
        environment: BTreeMap::from([
            ("network".into(), "disabled".into()),
            ("archive_sha256".into(), params.archive_sha256),
            ("archive_bytes".into(), params.archive_bytes.to_string()),
            ("transport".into(), "bounded-canonical-base64".into()),
        ]),
    };
    let result = agent
        .run_science_skill_quarantine(
            &session_id,
            ScienceStore::new_confined(&artifact_root, &context.workspace_root)
                .map_err(internal)?,
            context,
            xai_grok_science::skill_quarantine::SkillQuarantineRequest {
                operation_id: params.operation_id,
                selected_subpaths: params.items.into_iter().map(|item| item.sub_path).collect(),
            },
            bytes,
            Duration::from_millis(params.approval_timeout_ms),
        )
        .await
        .map_err(internal)?;
    to_raw_response(&serde_json::json!({
        "run": result.run,
        "operationId": result.operation_id,
        "artifacts": result.artifacts,
        "evidence": result.evidence,
        "provenance": result.provenance,
        "approvals": result.approvals,
        "replayAfter": result.replay_after,
        "status": "quarantined",
        "materialized": false,
        "enabled": false,
        "runtimeAuthority": "SessionActor-gated ACP adapter",
        "network": "disabled",
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
    let workspace = dunce::canonicalize(&handle.info.cwd).map_err(internal)?;
    let store_root = canonical_dir_within(params.store_root, &workspace)?;
    let result = agent
        .verify_science_goal(
            &session_id,
            ScienceStore::new_confined(&store_root, &workspace).map_err(internal)?,
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
    let workspace = dunce::canonicalize(&handle.info.cwd).map_err(internal)?;
    let store_root = canonical_dir_within(params.store_root, &workspace)?;
    let artifact_root = canonical_dir_within(params.artifact_root, &workspace)?;
    let canonical_file = |path: PathBuf, label: &str| -> Result<PathBuf, acp::Error> {
        let path = dunce::canonicalize(path).map_err(internal)?;
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
            let parent = dunce::canonicalize(parent).map_err(internal)?;
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
            ScienceStore::new_confined(&store_root, &context.workspace_root).map_err(internal)?,
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

/// Admitted ecosystem capability → fixed connector_fetch mapping.
///
/// Currently only `ecosystem/biomni/query_uniprot`. Does not run Biomni Python,
/// does not accept endpoint/URL/headers, and never lets the renderer choose
/// connector_id (always `uniprot`).
async fn handle_capability_run(agent: &MvpAgent, args: &acp::ExtRequest) -> ExtResult {
    let params: CapabilityRunParams = parse_params(args)?;
    let mapped = xai_grok_science::capability::map_biomni_query_uniprot(
        &params.capability_id,
        &params.input,
    )
    .map_err(|error| acp::Error::invalid_params().data(error.to_string()))?;
    use base64::Engine as _;
    let fixture_bytes = params
        .fixture_data_base64
        .iter()
        .map(|encoded| {
            base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .map_err(|_| acp::Error::invalid_params().data("fixtureDataBase64 is malformed"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    // Reuse the connector_fetch product path with the server-fixed connector.
    let fetch_params = ConnectorFetchParams {
        session_id: params.session_id,
        project_id: params.project_id,
        owner_id: params.owner_id,
        store_root: params.store_root,
        artifact_root: params.artifact_root,
        connector_id: mapped.connector_id.to_owned(),
        query: mapped.query,
        max_results: mapped.max_results,
        fixture_paths: Vec::new(),
        approval_timeout_ms: params.approval_timeout_ms,
    };
    let provenance = xai_grok_science::connectors::fetch::CapabilitySourceProvenance {
        capability_id: mapped.capability_id.to_owned(),
        repository: mapped.provenance.repository.to_owned(),
        exact_commit: mapped.provenance.exact_commit.to_owned(),
        source_path: mapped.provenance.source_path.to_owned(),
        source_sha256: mapped.provenance.source_sha256.to_owned(),
        license: mapped.provenance.license.to_owned(),
        reuse_mode: mapped.provenance.reuse_mode.to_owned(),
        lumen_executor: mapped.provenance.lumen_executor.to_owned(),
    };
    let result =
        execute_connector_fetch(agent, fetch_params, Some(provenance), Some(fixture_bytes)).await?;
    // Attach capability provenance for product audit without inventing a second authority.
    let mut value = serde_json::to_value(&result).map_err(internal)?;
    if let Some(obj) = value.as_object_mut() {
        obj.insert(
            "capability".into(),
            serde_json::json!({
                "id": mapped.capability_id,
                "source": "Biomni",
                "executor": "Rust Lumen SessionActor",
                "dataSource": "UniProt",
                "mode": "fixture/offline",
                "provenance": mapped.provenance,
                "controlledTools": mapped.controlled_tools,
            }),
        );
    }
    to_raw_response(&value)
}

/// S3 connector fetch entry: validates the connector, builds the protocol's
/// policy-gated request sequence, pairs each request with its offline
/// fixture, then drives the SessionActor begin/permission/finish protocol.
async fn handle_connector_fetch(agent: &MvpAgent, args: &acp::ExtRequest) -> ExtResult {
    let params: ConnectorFetchParams = parse_params(args)?;
    let result = execute_connector_fetch(agent, params, None, None).await?;
    to_raw_response(&result)
}

async fn execute_connector_fetch(
    agent: &MvpAgent,
    params: ConnectorFetchParams,
    capability_provenance: Option<xai_grok_science::connectors::fetch::CapabilitySourceProvenance>,
    supplied_fixture_bytes: Option<Vec<Vec<u8>>>,
) -> Result<xai_grok_science::connectors::fetch::FetchResult, acp::Error> {
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
    let supplied_count = supplied_fixture_bytes
        .as_ref()
        .map_or(params.fixture_paths.len(), Vec::len);
    if supplied_count != expected {
        return Err(acp::Error::invalid_params().data(format!(
            "connector {} requires exactly {expected} fixture exchange(s)",
            descriptor.id
        )));
    }
    let session_id = acp::SessionId::new(params.session_id);
    let handle = agent
        .get_session_handle(&session_id)
        .ok_or_else(|| acp::Error::invalid_params().data("session not found"))?;
    let workspace = dunce::canonicalize(&handle.info.cwd).map_err(internal)?;
    let store_root = canonical_dir_within(params.store_root, &workspace)?;
    let artifact_root = canonical_dir_within(params.artifact_root, &workspace)?;
    let fixture_bytes = if let Some(bytes) = supplied_fixture_bytes {
        for item in &bytes {
            if item.len() as u64 > xai_grok_science::preview::DEFAULT_MAX_BYTES {
                return Err(acp::Error::invalid_params().data("fixture exceeds the size cap"));
            }
        }
        bytes
    } else {
        let mut bytes = Vec::with_capacity(expected);
        for path in &params.fixture_paths {
            let path = dunce::canonicalize(path).map_err(internal)?;
            if !path.starts_with(&workspace) || !path.is_file() {
                return Err(acp::Error::invalid_params()
                    .data("fixturePaths must be files inside session cwd"));
            }
            let item = std::fs::read(&path).map_err(internal)?;
            if item.len() as u64 > xai_grok_science::preview::DEFAULT_MAX_BYTES {
                return Err(acp::Error::invalid_params().data("fixture exceeds the size cap"));
            }
            bytes.push(item);
        }
        bytes
    };
    // Build the protocol's policy-gated request sequence through the adapter.
    let paths = adapter
        .build_fixture_paths(&params.query, params.max_results, &fixture_bytes)
        .map_err(|error| acp::Error::invalid_params().data(error.to_string()))?;
    let mut requests = Vec::with_capacity(paths.len());
    for path in &paths {
        let req =
            xai_grok_science::connectors::validate_fixture_request(descriptor.id, path, 10_000)
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
    agent
        .run_science_fetch(
            &session_id,
            ScienceStore::new_confined(&store_root, &context.workspace_root).map_err(internal)?,
            context,
            descriptor.id.to_owned(),
            params.query,
            requests,
            fixture_bytes,
            capability_provenance,
            Duration::from_millis(params.approval_timeout_ms),
        )
        .await
        .map_err(internal)
}

/// Compose already-succeeded Science runs into a self-contained, byte-verified
/// biomedical evidence dossier. The request task resolves identities and
/// directories only; source artifact reads and all writes occur in the
/// SessionActor.
async fn handle_evidence_dossier(agent: &MvpAgent, args: &acp::ExtRequest) -> ExtResult {
    let params: EvidenceDossierParams = parse_params(args)?;
    if params.project_id.is_empty() || params.owner_id.is_empty() {
        return Err(acp::Error::invalid_params().data("projectId and ownerId are required"));
    }
    if params.source_run_ids.is_empty()
        || params.source_run_ids.len() > xai_grok_science::dossier::MAX_SOURCE_RUNS
        || params.source_run_ids.iter().any(|run_id| run_id.is_empty())
    {
        return Err(acp::Error::invalid_params().data(format!(
            "sourceRunIds must contain 1..={} non-empty ids",
            xai_grok_science::dossier::MAX_SOURCE_RUNS
        )));
    }
    if !(1..=300_000).contains(&params.approval_timeout_ms) {
        return Err(acp::Error::invalid_params().data("approvalTimeoutMs must be in 1..=300000"));
    }
    let session_id = acp::SessionId::new(params.session_id);
    let handle = agent
        .get_session_handle(&session_id)
        .ok_or_else(|| acp::Error::invalid_params().data("session not found"))?;
    handle
        .science_feature_gates
        .require_all(&[
            xai_grok_science::features::ScienceFeature::ResearchProject,
            xai_grok_science::features::ScienceFeature::EvidenceGraph,
        ])
        .map_err(internal)?;
    let workspace = dunce::canonicalize(&handle.info.cwd).map_err(internal)?;
    let store_root = canonical_existing_dir_within(params.store_root, &workspace)?;
    let artifact_root = canonical_existing_dir_within(params.artifact_root, &workspace)?;
    if artifact_root != store_root.join("runs") {
        return Err(acp::Error::invalid_params()
            .data("artifactRoot for an evidence dossier must equal storeRoot/runs"));
    }
    let source_run_ids = params
        .source_run_ids
        .into_iter()
        .map(RunId::new)
        .collect::<Vec<_>>();
    let context = RunContext {
        run_id: RunId::new_v7(),
        project_id: ProjectId::new(params.project_id),
        session_id: session_id.0.to_string(),
        owner_id: params.owner_id,
        workspace_root: workspace,
        provider: "local-store-verified".into(),
        approval_policy: "production-session-permission".into(),
        tool_profile: "science-evidence-dossier-v1".into(),
        artifact_root,
        environment: BTreeMap::from([
            ("network".into(), "disabled".into()),
            ("locale".into(), "C".into()),
        ]),
    };
    let result = agent
        .run_science_evidence_dossier(
            &session_id,
            ScienceStore::new_confined(&store_root, &context.workspace_root).map_err(internal)?,
            store_root,
            context,
            source_run_ids,
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
    let workspace = dunce::canonicalize(&handle.info.cwd).map_err(internal)?;
    let source_path = dunce::canonicalize(&params.source_path).map_err(internal)?;
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
            ScienceStore::new_confined(&store_root, &context.workspace_root).map_err(internal)?,
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
    let workspace = dunce::canonicalize(&handle.info.cwd).map_err(internal)?;
    let fixture_path = dunce::canonicalize(params.fixture_path).map_err(internal)?;
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
            ScienceStore::new_confined(&store_root, &context.workspace_root).map_err(internal)?,
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
struct EvidenceTraceParams {
    session_id: String,
    store_root: PathBuf,
    project_id: String,
    claim_id: String,
    owner_id: String,
}

async fn handle_evidence_trace(agent: &MvpAgent, args: &acp::ExtRequest) -> ExtResult {
    let params: EvidenceTraceParams = parse_params(args)?;
    if params.owner_id.is_empty() {
        return Err(acp::Error::invalid_params().data("ownerId is required"));
    }
    let session_id = acp::SessionId::new(params.session_id);
    let handle = agent
        .get_session_handle(&session_id)
        .ok_or_else(|| acp::Error::invalid_params().data("session not found"))?;
    let workspace = dunce::canonicalize(&handle.info.cwd).map_err(internal)?;
    let store_root = canonical_dir_within(params.store_root, &workspace)?;
    let store = xai_grok_science::project::ProjectStore::new_confined(&store_root, &workspace)
        .map_err(internal)?
        .with_gates(handle.science_feature_gates.clone());
    let project_id = xai_grok_science::project::ProjectId(params.project_id);
    store
        .assert_project_owner(&project_id, &params.owner_id)
        .map_err(internal)?;
    let trace = store
        .trace_evidence(&project_id, &params.claim_id)
        .map_err(internal)?;
    to_raw_response(&trace)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EvidenceCompareParams {
    session_id: String,
    store_root: PathBuf,
    project_id: String,
    claim_a: String,
    claim_b: String,
    owner_id: String,
}

async fn handle_evidence_compare(agent: &MvpAgent, args: &acp::ExtRequest) -> ExtResult {
    let params: EvidenceCompareParams = parse_params(args)?;
    if params.owner_id.is_empty() {
        return Err(acp::Error::invalid_params().data("ownerId is required"));
    }
    let session_id = acp::SessionId::new(params.session_id);
    let handle = agent
        .get_session_handle(&session_id)
        .ok_or_else(|| acp::Error::invalid_params().data("session not found"))?;
    let workspace = dunce::canonicalize(&handle.info.cwd).map_err(internal)?;
    let store_root = canonical_dir_within(params.store_root, &workspace)?;
    let store = xai_grok_science::project::ProjectStore::new_confined(&store_root, &workspace)
        .map_err(internal)?
        .with_gates(handle.science_feature_gates.clone());
    let project_id = xai_grok_science::project::ProjectId(params.project_id);
    store
        .assert_project_owner(&project_id, &params.owner_id)
        .map_err(internal)?;
    let cmp = store
        .compare_claims(&project_id, &params.claim_a, &params.claim_b)
        .map_err(internal)?;
    to_raw_response(&cmp)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EvidenceConsistencyParams {
    session_id: String,
    store_root: PathBuf,
    project_id: String,
    owner_id: String,
}

async fn handle_evidence_consistency(agent: &MvpAgent, args: &acp::ExtRequest) -> ExtResult {
    let params: EvidenceConsistencyParams = parse_params(args)?;
    if params.owner_id.is_empty() {
        return Err(acp::Error::invalid_params().data("ownerId is required"));
    }
    let session_id = acp::SessionId::new(params.session_id);
    let handle = agent
        .get_session_handle(&session_id)
        .ok_or_else(|| acp::Error::invalid_params().data("session not found"))?;
    let workspace = dunce::canonicalize(&handle.info.cwd).map_err(internal)?;
    let store_root = canonical_dir_within(params.store_root, &workspace)?;
    let store = xai_grok_science::project::ProjectStore::new_confined(&store_root, &workspace)
        .map_err(internal)?
        .with_gates(handle.science_feature_gates.clone());
    let project_id = xai_grok_science::project::ProjectId(params.project_id);
    store
        .assert_project_owner(&project_id, &params.owner_id)
        .map_err(internal)?;
    let report = store.check_consistency(&project_id).map_err(internal)?;
    to_raw_response(&report)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EvidenceReproductionParams {
    session_id: String,
    store_root: PathBuf,
    project_id: String,
    claim_id: String,
    owner_id: String,
}

async fn handle_evidence_reproduction(agent: &MvpAgent, args: &acp::ExtRequest) -> ExtResult {
    let params: EvidenceReproductionParams = parse_params(args)?;
    if params.owner_id.is_empty() {
        return Err(acp::Error::invalid_params().data("ownerId is required"));
    }
    let session_id = acp::SessionId::new(params.session_id);
    let handle = agent
        .get_session_handle(&session_id)
        .ok_or_else(|| acp::Error::invalid_params().data("session not found"))?;
    let workspace = dunce::canonicalize(&handle.info.cwd).map_err(internal)?;
    let store_root = canonical_dir_within(params.store_root, &workspace)?;
    let store = xai_grok_science::project::ProjectStore::new_confined(&store_root, &workspace)
        .map_err(internal)?
        .with_gates(handle.science_feature_gates.clone());
    let project_id = xai_grok_science::project::ProjectId(params.project_id);
    store
        .assert_project_owner(&project_id, &params.owner_id)
        .map_err(internal)?;
    let status = store
        .reproduction_status(&project_id, &params.claim_id)
        .map_err(internal)?;
    to_raw_response(&status)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectMigrateParams {
    session_id: String,
    store_root: PathBuf,
    run_id: String,
    owner_id: String,
    title: String,
    question: String,
    operation_id: String,
    #[serde(default)]
    artifact_root: Option<PathBuf>,
    #[serde(default = "default_approval_timeout_ms")]
    approval_timeout_ms: u64,
}

/// Migrate a V1 run through the same typed SessionActor mutation protocol as
/// project creation. The ACP request task resolves only paths and parameters;
/// permission, the durable run and the project-store write all remain actor
/// owned.
async fn handle_project_migrate(agent: &MvpAgent, args: &acp::ExtRequest) -> ExtResult {
    let params: ProjectMigrateParams = parse_params(args)?;
    if params.run_id.is_empty() {
        return Err(acp::Error::invalid_params().data("runId is required"));
    }
    if params.title.is_empty() {
        return Err(acp::Error::invalid_params().data("title is required"));
    }
    if params.question.is_empty() {
        return Err(acp::Error::invalid_params().data("question is required"));
    }
    let outcome = run_project_mutation(
        agent,
        params.session_id,
        params.owner_id,
        params.store_root,
        params.artifact_root,
        params.operation_id,
        None,
        params.approval_timeout_ms,
        xai_grok_science::project::ProjectMutation::ProjectMigrate {
            source_run_id: params.run_id,
            title: params.title,
            research_question: params.question,
            authority_run_id: String::new(),
        },
    )
    .await?;

    // Preserve the legacy migration fields at the top level while adding the
    // actor proof carried by every typed project mutation response.
    let mut response = outcome.result;
    let fields = response
        .as_object_mut()
        .ok_or_else(|| internal("project migration returned a non-object result"))?;
    fields.insert("operationId".into(), outcome.operation_id.into());
    fields.insert("revision".into(), outcome.revision.into());
    fields.insert("replayed".into(), outcome.replayed.into());
    fields.insert(
        "runtimeAuthority".into(),
        "SessionActor-gated ACP adapter".into(),
    );
    to_raw_response(&response)
}

// ── WP-4/5/6/7/8 preview handlers ────────────────────────────────

async fn store_handler<T: serde::Serialize>(
    agent: &MvpAgent,
    session_id: &str,
    store_root: PathBuf,
    f: impl FnOnce(
        &xai_grok_science::project::ProjectStore,
    ) -> Result<T, xai_grok_science::ScienceError>,
) -> ExtResult {
    let sid = acp::SessionId::new(session_id.to_string());
    let handle = agent
        .get_session_handle(&sid)
        .ok_or_else(|| acp::Error::invalid_params().data("session not found"))?;
    let workspace = dunce::canonicalize(&handle.info.cwd).map_err(internal)?;
    let sr = canonical_dir_within(store_root, &workspace)?;
    let store = xai_grok_science::project::ProjectStore::new_confined(&sr, &workspace)
        .map_err(internal)?
        .with_gates(handle.science_feature_gates.clone());
    let result = f(&store).map_err(internal)?;
    to_raw_response(&result)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WorkflowGenParams {
    session_id: String,
    store_root: PathBuf,
    #[serde(default)]
    project_id: String,
    #[serde(rename = "workflowSpec")]
    spec: serde_json::Value,
}

async fn handle_workflow_validate(agent: &MvpAgent, args: &acp::ExtRequest) -> ExtResult {
    let params: WorkflowGenParams = parse_params(args)?;
    let spec: xai_grok_science::workflow::WorkflowSpec =
        serde_json::from_value(params.spec).map_err(internal)?;
    store_handler(agent, &params.session_id, params.store_root, move |s| {
        s.workflow_validate(&spec)
    })
    .await
}

async fn handle_workflow_dry_run(agent: &MvpAgent, args: &acp::ExtRequest) -> ExtResult {
    let params: WorkflowGenParams = parse_params(args)?;
    let spec: xai_grok_science::workflow::WorkflowSpec =
        serde_json::from_value(params.spec).map_err(internal)?;
    // No kernel manifest is available on this read-only surface, so a
    // workflow with notebook steps is reported as blocked rather than
    // assumed to pass. See ProjectStore::workflow_dry_run.
    store_handler(agent, &params.session_id, params.store_root, move |s| {
        s.workflow_dry_run(&spec, None)
    })
    .await
}

// ── LS5-K8: workflow execution ───────────────────────────────────
//
// The only ACP entry in this file that RUNS a workflow, and therefore the only
// one that spawns an interpreter. Like the four WP-2 mutations above it takes
// no authority of its own: it parses, confines every path to the session
// workspace, and hands a typed binding to the SessionActor. It never
// constructs a WorkflowExecutor, a StepRunner or a kernel admission — those
// exist only on the far side of a permission decision, inside the actor.

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WorkflowExecuteParams {
    session_id: String,
    owner_id: String,
    store_root: PathBuf,
    /// Idempotency key. REQUIRED — there is no default, because without one a
    /// retry is indistinguishable from a second intentional execution.
    operation_id: String,
    #[serde(rename = "workflowSpec")]
    spec: serde_json::Value,
    /// Absolute path to the interpreter kernel steps run on. Never resolved
    /// from `PATH`: which binary ran is part of the evidence.
    interpreter_path: PathBuf,
    #[serde(default = "_workflow_kernel_id")]
    kernel_id: String,
    #[serde(default = "_python_kind")]
    kernel_kind: String,
    /// Explicit opt-in to `StepKind::NotebookCell`.
    ///
    /// `ExecutionPolicy::default()` omits that kind so running arbitrary code
    /// is a decision rather than a default; this is where the caller makes it,
    /// visibly, in the request. Absent, kernel steps are refused by the
    /// executor before the run is queued.
    #[serde(default)]
    allow_kernel_steps: bool,
    #[serde(default)]
    artifact_root: Option<PathBuf>,
    #[serde(default = "_probe_timeout_ms")]
    probe_timeout_ms: u64,
    #[serde(default = "default_approval_timeout_ms")]
    approval_timeout_ms: u64,
}

fn _workflow_kernel_id() -> String {
    "session-kernel".into()
}

async fn handle_workflow_execute(agent: &MvpAgent, args: &acp::ExtRequest) -> ExtResult {
    let params: WorkflowExecuteParams = parse_params(args)?;
    if params.owner_id.is_empty() {
        return Err(acp::Error::invalid_params().data("ownerId is required"));
    }
    if params.operation_id.is_empty() {
        return Err(acp::Error::invalid_params().data("operationId is required"));
    }
    if !(1..=300_000).contains(&params.approval_timeout_ms) {
        return Err(acp::Error::invalid_params().data("approvalTimeoutMs must be in 1..=300000"));
    }
    if !(1..=120_000).contains(&params.probe_timeout_ms) {
        return Err(acp::Error::invalid_params().data("probeTimeoutMs must be in 1..=120000"));
    }
    if !params.interpreter_path.is_absolute() {
        return Err(acp::Error::invalid_params().data("interpreterPath must be absolute"));
    }
    let kernel_kind = match params.kernel_kind.as_str() {
        "r" | "R" => xai_grok_science::workflow::KernelKind::R,
        "julia" => xai_grok_science::workflow::KernelKind::Julia,
        "python" => xai_grok_science::workflow::KernelKind::Python,
        other => {
            return Err(acp::Error::invalid_params().data(format!("unknown kernelKind '{other}'")));
        }
    };
    let spec: xai_grok_science::workflow::WorkflowSpec = serde_json::from_value(params.spec)
        .map_err(|error| {
            acp::Error::invalid_params()
                .data(format!("workflowSpec is not a WorkflowSpec: {error}"))
        })?;
    if spec.project_id.0.trim().is_empty() {
        return Err(acp::Error::invalid_params()
            .data("workflowSpec.projectId must name an existing owned project"));
    }

    let session_id = acp::SessionId::new(params.session_id);
    let handle = agent
        .get_session_handle(&session_id)
        .ok_or_else(|| acp::Error::invalid_params().data("session not found"))?;
    let workspace = dunce::canonicalize(&handle.info.cwd).map_err(internal)?;
    let store_root = canonical_dir_within(params.store_root, &workspace)?;
    let artifact_root = match params.artifact_root {
        Some(root) => canonical_dir_within(root, &workspace)?,
        None => canonical_dir_within(store_root.join("runs"), &workspace)?,
    };
    let workflow_run_id = xai_grok_science::workflow::run_id_for_operation(&params.operation_id)
        .map_err(|error| acp::Error::invalid_params().data(error.to_string()))?;
    let context = RunContext {
        run_id: RunId::new(workflow_run_id),
        project_id: ProjectId::new(spec.project_id.0.clone()),
        session_id: session_id.0.to_string(),
        owner_id: params.owner_id.clone(),
        workspace_root: workspace,
        provider: "offline-deterministic".into(),
        approval_policy: "production-session-permission".into(),
        tool_profile: "science-workflow-execute-v1".into(),
        artifact_root,
        environment: BTreeMap::from([
            ("network".into(), "disabled".into()),
            ("locale".into(), "C".into()),
        ]),
    };
    let binding = crate::session::commands::ScienceWorkflowBinding {
        execution: xai_grok_science::workflow::WorkflowExecutionRequest {
            operation_id: params.operation_id,
            session_id: session_id.0.to_string(),
            owner_id: params.owner_id,
            spec,
        },
        executor_root: store_root.clone(),
        kernel_id: params.kernel_id,
        kernel_kind,
        interpreter_path: params.interpreter_path,
        probe_timeout: Duration::from_millis(params.probe_timeout_ms),
        allow_kernel_steps: params.allow_kernel_steps,
    };
    let report = agent
        .run_science_workflow_execution(
            &session_id,
            ScienceStore::new_confined(&store_root, &context.workspace_root).map_err(internal)?,
            context,
            binding,
            Duration::from_millis(params.approval_timeout_ms),
        )
        .await
        .map_err(internal)?;
    workflow_execution_response(&report)
}

fn workflow_execution_response(
    report: &xai_grok_science::workflow::WorkflowRunReport,
) -> ExtResult {
    to_raw_response(&serde_json::json!({
        "operationId": report.run.operation_id,
        "runId": report.run.run_id,
        "workflowId": report.run.workflow_id,
        "projectId": report.run.project_id.0,
        "state": report.run.state,
        "stepOrder": report.run.step_order,
        "refusedSteps": report.run.refused_steps,
        "failure": report.run.failure,
        "artifactsCommitted": report.artifacts_committed,
        "stepsReused": report.steps_reused,
        "replayed": report.replayed,
        "recovered": report.recovered,
        "commits": report.commits.iter().map(|commit| serde_json::json!({
            "commitKey": commit.commit_key,
            "stepId": commit.step_id,
            "outputManifest": commit.output_manifest,
            "outputManifestHash": commit.output_manifest_hash,
            "committedByAttempt": commit.committed_by_attempt,
        })).collect::<Vec<_>>(),
        "attempts": report.attempts.iter().map(|attempt| serde_json::json!({
            "attemptId": attempt.attempt_id,
            "stepId": attempt.step_id,
            "attemptNumber": attempt.attempt_number,
            "terminalState": attempt.terminal_state,
            "errorClass": attempt.error_class,
            "errorDetail": attempt.error_detail,
        })).collect::<Vec<_>>(),
        "runtimeAuthority": "SessionActor-gated ACP adapter",
    }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct KernelAdmParams2 {
    session_id: String,
    owner_id: String,
    project_id: String,
    store_root: PathBuf,
    kernel_id: String,
    #[serde(default = "_python_kind")]
    kind: String,
    /// Absolute path to the interpreter to probe. Required: there is nothing
    /// to admit without one.
    interpreter_path: PathBuf,
    /// Optional confinement root for the resolved interpreter.
    #[serde(default)]
    allowed_root: Option<PathBuf>,
    /// Optional digest the caller asserts. VERIFIED against the probe and
    /// rejected on mismatch — it is no longer echoed back into the record.
    #[serde(default)]
    exec_hash: Option<String>,
    #[serde(default)]
    package_lock_path: Option<PathBuf>,
    #[serde(default)]
    lock_hash: Option<String>,
    #[serde(default = "_probe_timeout_ms")]
    probe_timeout_ms: u64,
    #[serde(default = "default_approval_timeout_ms")]
    approval_timeout_ms: u64,
}
fn _python_kind() -> String {
    "python".into()
}
fn _probe_timeout_ms() -> u64 {
    10_000
}

/// Resolve only request syntax and actor-owned roots here. The interpreter is
/// neither read nor executed on the ACP request task; the SessionActor opens
/// the durable run, obtains permission, and performs the identity probe.
async fn handle_kernel_admission(agent: &MvpAgent, args: &acp::ExtRequest) -> ExtResult {
    let params: KernelAdmParams2 = parse_params(args)?;
    if params.owner_id.trim().is_empty()
        || params.project_id.trim().is_empty()
        || params.kernel_id.trim().is_empty()
    {
        return Err(
            acp::Error::invalid_params().data("ownerId, projectId, and kernelId are required")
        );
    }
    if !params.interpreter_path.is_absolute() {
        return Err(acp::Error::invalid_params().data("interpreterPath must be absolute"));
    }
    let kind = match params.kind.trim().to_ascii_lowercase().as_str() {
        "python" => xai_grok_science::workflow::KernelKind::Python,
        "r" => xai_grok_science::workflow::KernelKind::R,
        "julia" => xai_grok_science::workflow::KernelKind::Julia,
        _ => {
            return Err(
                acp::Error::invalid_params().data("kind must be one of python, r, or julia")
            );
        }
    };
    if !(1..=120_000).contains(&params.probe_timeout_ms) {
        return Err(acp::Error::invalid_params().data("probeTimeoutMs must be in 1..=120000"));
    }
    if !(1..=300_000).contains(&params.approval_timeout_ms) {
        return Err(acp::Error::invalid_params().data("approvalTimeoutMs must be in 1..=300000"));
    }
    let session_id = acp::SessionId::new(params.session_id);
    let handle = agent
        .get_session_handle(&session_id)
        .ok_or_else(|| acp::Error::invalid_params().data("session not found"))?;
    let workspace = dunce::canonicalize(&handle.info.cwd).map_err(internal)?;
    let project_root = canonical_dir_within(params.store_root, &workspace)?;
    let run_root = canonical_dir_within(project_root.join("runs"), &workspace)?;
    let context = RunContext {
        run_id: RunId::new_v7(),
        project_id: ProjectId::new(params.project_id),
        session_id: session_id.0.to_string(),
        owner_id: params.owner_id,
        workspace_root: workspace,
        provider: "offline-deterministic".into(),
        approval_policy: "production-session-permission".into(),
        tool_profile: "science-kernel-admission-v1".into(),
        artifact_root: run_root,
        environment: BTreeMap::from([
            ("network".into(), "disabled".into()),
            ("locale".into(), "C".into()),
        ]),
    };
    let mut request = xai_grok_science::workflow::KernelAdmissionRequest::new(
        params.kernel_id,
        kind,
        params.interpreter_path,
    )
    .with_probe_timeout(Duration::from_millis(params.probe_timeout_ms));
    request.allowed_root = params.allowed_root;
    request.supplied_executable_hash = params.exec_hash;
    request.package_lock_path = params.package_lock_path;
    request.supplied_package_lock_hash = params.lock_hash;
    let result = agent
        .run_science_kernel_admission(
            &session_id,
            ScienceStore::new_confined(&project_root, &context.workspace_root).map_err(internal)?,
            project_root,
            context,
            request,
            Duration::from_millis(params.approval_timeout_ms),
        )
        .await
        .map_err(internal)?;
    to_raw_response(&serde_json::json!({
        "runId": result.run.context.run_id,
        "projectId": result.run.context.project_id,
        "state": result.run.state,
        "admission": result.admission,
        "artifacts": result.artifacts,
        "evidence": result.evidence,
        "provenance": result.provenance,
        "approvals": result.approvals,
        "replayAfter": result.replay_after,
        "runtimeAuthority": "SessionActor-gated ACP adapter",
    }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjRParams {
    session_id: String,
    store_root: PathBuf,
    project_id: String,
    owner_id: String,
}

async fn handle_multimodal_index(agent: &MvpAgent, args: &acp::ExtRequest) -> ExtResult {
    let params: ProjRParams = parse_params(args)?;
    if params.owner_id.is_empty() {
        return Err(acp::Error::invalid_params().data("ownerId is required"));
    }
    store_handler(agent, &params.session_id, params.store_root, move |s| {
        let project_id = xai_grok_science::project::ProjectId(params.project_id);
        s.assert_project_owner(&project_id, &params.owner_id)?;
        s.multimodal_index(&project_id)
    })
    .await
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReviewRecordParams {
    session_id: String,
    store_root: PathBuf,
    project_id: String,
    owner_id: String,
    reviewer_id: String,
    verdict: String,
    summary: String,
    run_id: String,
    artifact_sha256s: Vec<String>,
    operation_id: String,
    #[serde(default)]
    claim_id: Option<String>,
    #[serde(default)]
    expected_revision: Option<String>,
    #[serde(default)]
    artifact_root: Option<PathBuf>,
    #[serde(default = "default_approval_timeout_ms")]
    approval_timeout_ms: u64,
}

async fn handle_review_record(agent: &MvpAgent, args: &acp::ExtRequest) -> ExtResult {
    let params: ReviewRecordParams = parse_params(args)?;
    if params.project_id.is_empty()
        || params.owner_id.is_empty()
        || params.reviewer_id.is_empty()
        || params.summary.trim().is_empty()
        || params.run_id.is_empty()
        || params.artifact_sha256s.is_empty()
    {
        return Err(acp::Error::invalid_params().data(
            "projectId, ownerId, reviewerId, summary, runId and artifactSha256s are required",
        ));
    }
    let verdict =
        xai_grok_science::project::ReviewVerdict::parse(&params.verdict).map_err(internal)?;
    let outcome = run_project_mutation(
        agent,
        params.session_id,
        params.owner_id,
        params.store_root,
        params.artifact_root,
        params.operation_id,
        params.expected_revision,
        params.approval_timeout_ms,
        xai_grok_science::project::ProjectMutation::ReviewRecord {
            project_id: xai_grok_science::project::ProjectId(params.project_id),
            reviewer_id: params.reviewer_id,
            verdict,
            summary: params.summary,
            claim_id: params.claim_id,
            source_run_id: params.run_id,
            // Overwritten inside `run_project_mutation` with the actor's
            // freshly generated durable authority run id.
            authority_run_id: String::new(),
            artifact_sha256s: params.artifact_sha256s,
        },
    )
    .await?;
    mutation_response(outcome)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CollabInvParams2 {
    session_id: String,
    store_root: PathBuf,
    project_id: String,
    owner_id: String,
    invitee: String,
}

async fn handle_collaboration_invite(agent: &MvpAgent, args: &acp::ExtRequest) -> ExtResult {
    let params: CollabInvParams2 = parse_params(args)?;
    store_handler(agent, &params.session_id, params.store_root, move |s| {
        s.collaboration_invite(
            &xai_grok_science::project::ProjectId(params.project_id),
            &params.owner_id,
            params.invitee,
        )
    })
    .await
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RcpParams {
    session_id: String,
    store_root: PathBuf,
    project_id: String,
    owner_id: String,
    hostname: String,
}

async fn handle_remote_compute_plan(agent: &MvpAgent, args: &acp::ExtRequest) -> ExtResult {
    let params: RcpParams = parse_params(args)?;
    if params.owner_id.is_empty() {
        return Err(acp::Error::invalid_params().data("ownerId is required"));
    }
    store_handler(agent, &params.session_id, params.store_root, move |s| {
        let project_id = xai_grok_science::project::ProjectId(params.project_id);
        s.assert_project_owner(&project_id, &params.owner_id)?;
        s.remote_compute_plan(&project_id, params.hostname)
    })
    .await
}
